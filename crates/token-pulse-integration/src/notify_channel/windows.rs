//! Current-user protected named pipe. All clients, frames and waits are bounded.
mod security;
use super::{MAX_WAKE_FRAME_BYTES, NotifyCapability, WakeError};
use std::{
    collections::VecDeque,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use token_pulse_core::notify::NotifyWakeHint;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::windows::named_pipe::{ClientOptions, NamedPipeServer},
    sync::oneshot,
    time::timeout,
};
use windows_sys::Win32::{
    Foundation::{ERROR_BROKEN_PIPE, ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_NOT_CONNECTED},
    Storage::FileSystem::SECURITY_IDENTIFICATION,
};

const CLIENT_DEADLINE: Duration = Duration::from_millis(300);
const SERVER_DEADLINE: Duration = Duration::from_millis(500);
const DEDUP_LIFETIME: Duration = Duration::from_secs(60);
const MAX_RECENT_HINTS: usize = 128;

fn channel_name(capability: &NotifyCapability) -> Result<String, WakeError> {
    use sha2::{Digest, Sha256};
    let user = security::user_sid().map_err(|_| WakeError::Io)?;
    // Namespace by the user without exposing the SID as transport diagnostics.
    let user_hash = format!("{:x}", Sha256::digest(user.as_bytes()));
    Ok(format!(
        r"\\.\pipe\TokenPulse.Notify.{}.{}",
        &user_hash[..32],
        capability.registration_id()
    ))
}

/// The callback must only enqueue reconciliation and return; it runs on the dedicated worker.
pub struct NotifyListener {
    stop: Option<oneshot::Sender<()>>,
    worker: Option<thread::JoinHandle<()>>,
}
impl NotifyListener {
    pub fn start(
        capability: NotifyCapability,
        wake: Arc<dyn Fn(NotifyWakeHint) + Send + Sync>,
    ) -> Result<Self, WakeError> {
        let name = channel_name(&capability)?;
        let (stop_tx, mut stop_rx) = oneshot::channel();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let worker = thread::Builder::new().name("tokenpulse-notify".into()).spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(runtime) => runtime,
                Err(_) => { let _ = ready_tx.send(Err(WakeError::Io)); return; }
            };
            runtime.block_on(async move {
                let mut pipe = match security::create_server(&name, true) {
                    Ok(pipe) => pipe,
                    Err(_) => { let _ = ready_tx.send(Err(WakeError::Io)); return; }
                };
                if ready_tx.send(Ok(())).is_err() { return; }
                let mut recent: VecDeque<(NotifyWakeHint, Instant)> = VecDeque::new();
                loop {
                    tokio::select! {
                        _ = &mut stop_rx => break,
                        result = pipe.connect() => if let Err(error) = result {
                            if matches!(error.raw_os_error(), Some(code) if code == ERROR_NO_DATA as i32 || code == ERROR_PIPE_NOT_CONNECTED as i32 || code == ERROR_BROKEN_PIPE as i32) {
                                // A queued client may have timed out before this instance is accepted.
                                pipe = tokio::select! {
                                    _ = &mut stop_rx => break,
                                    next = next_instance(&name) => match next { Ok(next) => next, Err(_) => break },
                                };
                                continue;
                            }
                            break;
                        },
                    }
                    // Keep one fresh waiting instance. A disconnected Tokio/Mio handle retains
                    // its completed IO state and must not be reused for the next client's frame.
                    let next = tokio::select! {
                        _ = &mut stop_rx => break,
                        next = next_instance(&name) => match next { Ok(next) => next, Err(_) => break },
                    };
                    tokio::select! {
                        _ = &mut stop_rx => break,
                        _ = timeout(SERVER_DEADLINE, receive(&mut pipe, &capability, &wake, &mut recent)) => {},
                    }
                    if let Err(error) = pipe.disconnect() {
                        if error.raw_os_error() != Some(ERROR_PIPE_NOT_CONNECTED as i32) { break; }
                    }
                    pipe = next;
                }
            });
        }).map_err(|_| WakeError::Io)?;
        match ready_rx.recv_timeout(Duration::from_secs(1)) {
            Ok(Ok(())) => Ok(Self {
                stop: Some(stop_tx),
                worker: Some(worker),
            }),
            status => {
                let _ = stop_tx.send(());
                // A timed-out initializer owns its handles; dropping JoinHandle does not block UI.
                Err(match status {
                    Ok(Err(error)) => error,
                    _ => WakeError::Timeout,
                })
            }
        }
    }
}
impl Drop for NotifyListener {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

async fn next_instance(name: &str) -> std::io::Result<NamedPipeServer> {
    loop {
        match security::create_server(name, false) {
            Ok(pipe) => return Ok(pipe),
            Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                // Old kernel instances waiting for IO / handle cleanup may fill the eight slots.
                // Keep the owned namespace and let the outer stop select cancel this wait.
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(error) => return Err(error),
        }
    }
}

async fn receive(
    pipe: &mut NamedPipeServer,
    capability: &NotifyCapability,
    wake: &Arc<dyn Fn(NotifyWakeHint) + Send + Sync>,
    recent: &mut VecDeque<(NotifyWakeHint, Instant)>,
) -> Result<(), WakeError> {
    let mut prefix = [0; 4];
    pipe.read_exact(&mut prefix)
        .await
        .map_err(|_| WakeError::Io)?;
    let length = u32::from_le_bytes(prefix) as usize;
    if length == 0 || length > MAX_WAKE_FRAME_BYTES {
        return Err(WakeError::TooLarge);
    }
    let mut bytes = vec![0; length];
    pipe.read_exact(&mut bytes)
        .await
        .map_err(|_| WakeError::Io)?;
    match capability.decode_hint(&bytes) {
        Ok(hint) => {
            let now = Instant::now();
            while recent
                .front()
                .is_some_and(|(_, time)| now.duration_since(*time) >= DEDUP_LIFETIME)
            {
                recent.pop_front();
            }
            if !recent.iter().any(|(previous, _)| previous == &hint) {
                if recent.len() == MAX_RECENT_HINTS {
                    recent.pop_front();
                }
                recent.push_back((hint.clone(), now));
                // Callback receives the validated minimal type, never raw transport text.
                wake(hint);
            }
            pipe.write_all(&[1]).await.map_err(|_| WakeError::Io)?;
        }
        Err(_) => {
            pipe.write_all(&[0]).await.map_err(|_| WakeError::Io)?;
        }
    }
    // DisconnectNamedPipe discards unread output. Wait for receipt instead of racing the ACK.
    let mut receipt = [0];
    pipe.read_exact(&mut receipt)
        .await
        .map_err(|_| WakeError::Io)?;
    if receipt != [2] {
        return Err(WakeError::InvalidFrame);
    }
    Ok(())
}

/// Suitable for headless use; pipe connection / IO waits are limited to 300 ms, with no GUI or scan.
pub async fn send_hint(
    capability: &NotifyCapability,
    hint: &NotifyWakeHint,
) -> Result<(), WakeError> {
    let bytes = capability.encode_hint(hint)?;
    let name = channel_name(capability)?;
    timeout(CLIENT_DEADLINE, async {
        let mut pipe = loop {
            match ClientOptions::new()
                .security_qos_flags(SECURITY_IDENTIFICATION)
                .open(&name)
            {
                Ok(pipe) => break pipe,
                Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                    tokio::time::sleep(Duration::from_millis(10)).await
                }
                Err(_) => return Err(WakeError::Io),
            }
        };
        pipe.write_all(&(bytes.len() as u32).to_le_bytes())
            .await
            .map_err(|_| WakeError::Io)?;
        pipe.write_all(&bytes).await.map_err(|_| WakeError::Io)?;
        let mut ack = [0];
        pipe.read_exact(&mut ack).await.map_err(|_| WakeError::Io)?;
        pipe.write_all(&[2]).await.map_err(|_| WakeError::Io)?;
        // Keep the client alive until the server consumes receipt and disconnects this instance.
        // Otherwise a fast next sender can open the old instance immediately before disconnect.
        let mut end = [0];
        match pipe.read(&mut end).await {
            Ok(0) => {},
            Err(error) if matches!(error.raw_os_error(), Some(code) if code == ERROR_BROKEN_PIPE as i32 || code == ERROR_PIPE_NOT_CONNECTED as i32) => {},
            _ => return Err(WakeError::InvalidFrame),
        }
        if ack == [1] {
            Ok(())
        } else {
            Err(WakeError::Unauthorized)
        }
    })
    .await
    .map_err(|_| WakeError::Timeout)?
}

#[cfg(test)]
mod tests {
    use super::*;
    use token_pulse_core::notify::parse_codex_notification;
    fn hint(turn: &str) -> NotifyWakeHint {
        parse_codex_notification(
            &serde_json::to_vec(&serde_json::json!({
                "type":"agent-turn-complete", "thread-id":"test-thread", "turn-id":turn
            }))
            .unwrap(),
        )
        .unwrap()
        .unwrap()
    }
    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }
    async fn open_raw(name: &str) -> tokio::net::windows::named_pipe::NamedPipeClient {
        timeout(Duration::from_secs(1), async {
            loop {
                match ClientOptions::new().open(name) {
                    Ok(pipe) => break pipe,
                    Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                    Err(_) => panic!("native fixture pipe open failed"),
                }
            }
        })
        .await
        .expect("native fixture pipe open deadline")
    }
    #[test]
    fn real_pipe_delivers_deduplicates_and_rejects_duplicate_listener() {
        let cap = NotifyCapability::new();
        let (tx, rx) = std::sync::mpsc::channel();
        let listener = NotifyListener::start(
            cap.clone(),
            Arc::new(move |hint| {
                let _ = tx.send(hint);
            }),
        )
        .unwrap();
        assert!(NotifyListener::start(cap.clone(), Arc::new(|_| {})).is_err());
        runtime().block_on(async {
            send_hint(&cap, &hint("turn-1")).await.unwrap();
            send_hint(&cap, &hint("turn-1")).await.unwrap();
            send_hint(&cap, &hint("turn-2")).await.unwrap();
        });
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            hint("turn-1")
        );
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            hint("turn-2")
        );
        assert!(rx.try_recv().is_err());
        drop(listener);
        assert!(
            runtime()
                .block_on(send_hint(&cap, &hint("turn-3")))
                .is_err()
        );
        let reopened = NotifyListener::start(cap.clone(), Arc::new(|_| {})).unwrap();
        runtime()
            .block_on(send_hint(&cap, &hint("turn-3")))
            .unwrap();
        drop(reopened);
    }
    #[test]
    fn real_pipe_rejects_foreign_nonce_and_recovers_from_oversized_or_stalled_client() {
        let cap = NotifyCapability::new();
        let (tx, rx) = std::sync::mpsc::channel();
        let listener = NotifyListener::start(
            cap.clone(),
            Arc::new(move |hint| {
                let _ = tx.send(hint);
            }),
        )
        .unwrap();
        let mut wire = serde_json::to_value(&cap).unwrap();
        wire["nonce"] = serde_json::json!("0".repeat(64));
        let impostor: NotifyCapability = serde_json::from_value(wire).unwrap();
        let name = channel_name(&cap).unwrap();
        runtime().block_on(async {
            assert_eq!(
                send_hint(&impostor, &hint("unauthorized"))
                    .await
                    .unwrap_err(),
                WakeError::Unauthorized
            );
            let mut raw = open_raw(&name).await;
            raw.write_all(&u32::MAX.to_le_bytes()).await.unwrap();
            let mut ack = [0];
            assert!(!matches!(
                timeout(Duration::from_secs(1), raw.read_exact(&mut ack)).await,
                Ok(Ok(_))
            ));
            drop(raw);
            send_hint(&cap, &hint("valid-after-oversize"))
                .await
                .unwrap();
            let _stalled = open_raw(&name).await;
            assert_eq!(
                send_hint(&cap, &hint("blocked")).await.unwrap_err(),
                WakeError::Timeout
            );
            tokio::time::sleep(SERVER_DEADLINE).await;
            send_hint(&cap, &hint("valid-after-timeout")).await.unwrap();
        });
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            hint("valid-after-oversize")
        );
        // A sender timeout means acknowledgement is unknown, not that already-written bytes
        // were cancelled. Late valid hints may wake once; they still cannot create usage.
        let next = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        if next == hint("blocked") {
            assert_eq!(
                rx.recv_timeout(Duration::from_secs(1)).unwrap(),
                hint("valid-after-timeout")
            );
        } else {
            assert_eq!(next, hint("valid-after-timeout"));
        }
        assert!(rx.try_recv().is_err());
        drop(listener);
    }
    #[test]
    fn stopping_listener_cancels_a_partial_frame_and_releases_its_name() {
        let cap = NotifyCapability::new();
        let listener = NotifyListener::start(cap.clone(), Arc::new(|_| {})).unwrap();
        runtime().block_on(async {
            let mut pipe = ClientOptions::new()
                .open(channel_name(&cap).unwrap())
                .unwrap();
            pipe.write_all(&[100, 0]).await.unwrap();
            drop(listener);
            let mut ack = [0];
            assert!(!matches!(
                timeout(Duration::from_secs(1), pipe.read_exact(&mut ack)).await,
                Ok(Ok(_))
            ));
        });
        let reopened = NotifyListener::start(cap, Arc::new(|_| {})).unwrap();
        drop(reopened);
    }
    #[test]
    fn retained_disconnected_clients_do_not_prevent_later_hints() {
        let cap = NotifyCapability::new();
        let listener = NotifyListener::start(cap.clone(), Arc::new(|_| {})).unwrap();
        let name = channel_name(&cap).unwrap();
        runtime().block_on(async {
            let mut retained = Vec::new();
            for _ in 0..6 {
                let mut raw = open_raw(&name).await;
                raw.write_all(&u32::MAX.to_le_bytes()).await.unwrap();
                let mut end = [0];
                assert!(!matches!(
                    timeout(Duration::from_secs(1), raw.read_exact(&mut end)).await,
                    Ok(Ok(_))
                ));
                retained.push(raw);
            }
            send_hint(&cap, &hint("retained-clients")).await.unwrap();
            drop(retained);
            send_hint(&cap, &hint("capacity-restored")).await.unwrap();
        });
        drop(listener);
    }
}
