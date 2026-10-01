//! A single background executor drains durable requests, with no Tauri dependency.
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use token_pulse_core::error::ErrorCode;
use token_pulse_store::{Database, StoreResult};
pub struct JobService {
    stopping: Arc<AtomicBool>,
    wake: mpsc::SyncSender<()>,
    thread: Mutex<Option<JoinHandle<()>>>,
}
impl Drop for JobService {
    fn drop(&mut self) {
        self.shutdown();
    }
}
pub fn now_ms() -> StoreResult<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ErrorCode::InvalidQuery)?;
    Ok(i64::try_from(duration.as_millis()).map_err(|_| ErrorCode::NumericOverflow)?)
}
impl JobService {
    pub fn start(database: Database) -> StoreResult<Self> {
        Self::start_with_notify(database, Arc::new(|| {}))
    }
    pub fn start_with_notify(
        database: Database,
        on_finished: Arc<dyn Fn() + Send + Sync>,
    ) -> StoreResult<Self> {
        now_ms()?;
        let stopping = Arc::new(AtomicBool::new(false));
        let stop = stopping.clone();
        let (wake, receiver) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("tokenpulse-jobs".into())
            .spawn(move || {
                let mut checked_upgrade: Option<Instant> = None;
                while !stop.load(Ordering::Acquire) {
                    if checked_upgrade.is_none_or(|at| at.elapsed() >= Duration::from_secs(10)) {
                        if let Ok(at) = now_ms() {
                            let _ = database.enqueue_accounting_upgrade(at);
                        }
                        checked_upgrade = Some(Instant::now());
                    }
                    if let Ok(Some(job)) = database.next_queued_rebuild() {
                        let result = crate::replay::execute_rebuild_controlled(
                            &database,
                            &job.job_id,
                            || {
                                if stop.load(Ordering::Acquire) {
                                    Some(ErrorCode::JobInterrupted)
                                } else {
                                    now_ms().err().map(|e| e.code)
                                }
                            },
                            || now_ms().unwrap_or(job.updated_at_ms.value()),
                        );
                        on_finished();
                        // Recheck after each publication so independent old
                        // ledgers are migrated without waiting ten seconds.
                        checked_upgrade = None;
                        if result.is_err() {
                            let _ = receiver.recv_timeout(Duration::from_millis(250));
                        }
                        continue;
                    }
                    let _ = receiver.recv_timeout(Duration::from_millis(250));
                }
            })
            .map_err(|_| ErrorCode::DbWriteFailed)?;
        Ok(Self {
            stopping,
            wake,
            thread: Mutex::new(Some(thread)),
        })
    }
    pub fn wake(&self) {
        let _ = self.wake.try_send(());
    }
    pub fn shutdown(&self) {
        self.stopping.store(true, Ordering::Release);
        self.wake();
        if let Ok(mut thread) = self.thread.lock() {
            if let Some(handle) = thread.take() {
                let _ = handle.join();
            }
        }
    }
}
