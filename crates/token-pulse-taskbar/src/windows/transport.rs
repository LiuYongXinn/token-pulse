use super::{HostProcess, security};
use crate::{
    Envelope, HostMessage, HostReply, HostSession, MAX_FRAME_BYTES, PROTOCOL_VERSION, WireError,
};
use serde::{Serialize, de::DeserializeOwned};
use std::{io::ErrorKind, os::windows::io::AsRawHandle, path::Path, time::Duration};
use token_pulse_core::numeric::DecimalInt;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::windows::named_pipe::{ClientOptions, NamedPipeServer},
    time::timeout,
};
use windows_sys::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::SECURITY_IDENTIFICATION,
    System::Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId},
};

pub const IO_TIMEOUT: Duration = Duration::from_secs(5);
pub const HEARTBEAT_DEADLINE: Duration = Duration::from_secs(15);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportError {
    Io(ErrorKind),
    Protocol(WireError),
    PeerMismatch,
    Timeout,
    InvalidStartup,
    Spawn,
    Native,
}
impl From<std::io::Error> for TransportError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.kind())
    }
}
impl From<WireError> for TransportError {
    fn from(error: WireError) -> Self {
        Self::Protocol(error)
    }
}
#[derive(Clone)]
pub struct Startup {
    pub instance: String,
    pub nonce: String,
    pub parent_pid: u32,
}
impl Startup {
    pub fn new() -> Self {
        Self {
            instance: uuid::Uuid::new_v4().simple().to_string(),
            nonce: format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            ),
            parent_pid: std::process::id(),
        }
    }
    pub fn validate(&self) -> Result<(), TransportError> {
        if self.parent_pid == 0
            || self.instance.len() != 32
            || !self.instance.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err(TransportError::InvalidStartup);
        }
        HostSession::new(self.instance.clone(), self.nonce.clone())
            .map_err(|_| TransportError::InvalidStartup)?;
        Ok(())
    }
    pub fn channel_name(&self) -> String {
        format!(r"\\.\pipe\TokenPulse.Taskbar.{}", self.instance)
    }
    pub fn arguments(&self) -> [String; 6] {
        [
            "--host-instance".into(),
            self.instance.clone(),
            "--nonce".into(),
            self.nonce.clone(),
            "--parent-pid".into(),
            self.parent_pid.to_string(),
        ]
    }
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, TransportError> {
        let args: Vec<_> = args.into_iter().take(7).collect();
        if args.len() != 6
            || args[0] != "--host-instance"
            || args[2] != "--nonce"
            || args[4] != "--parent-pid"
        {
            return Err(TransportError::InvalidStartup);
        }
        let startup = Self {
            instance: args[1].clone(),
            nonce: args[3].clone(),
            parent_pid: args[5]
                .parse()
                .map_err(|_| TransportError::InvalidStartup)?,
        };
        startup.validate()?;
        Ok(startup)
    }
    fn envelope<T>(&self, sequence: i128, body: T) -> Result<Envelope<T>, TransportError> {
        Ok(Envelope {
            protocol_version: PROTOCOL_VERSION,
            host_instance_id: self.instance.clone(),
            nonce: self.nonce.clone(),
            sequence: DecimalInt::from_nonnegative(sequence)
                .map_err(|_| TransportError::InvalidStartup)?,
            body,
        })
    }
}
impl Default for Startup {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn write<W: AsyncWrite + Unpin, T: Serialize>(
    pipe: &mut W,
    frame: &Envelope<T>,
) -> Result<(), TransportError> {
    let mut bytes = vec![];
    crate::write_frame(&mut bytes, frame)?;
    timeout(IO_TIMEOUT, async {
        pipe.write_all(&bytes).await?;
        pipe.flush().await
    })
    .await
    .map_err(|_| TransportError::Timeout)??;
    Ok(())
}
pub async fn read<R: AsyncRead + Unpin, T: DeserializeOwned>(
    pipe: &mut R,
) -> Result<Option<Envelope<T>>, TransportError> {
    let mut prefix = [0; 4];
    if pipe.read(&mut prefix[..1]).await? == 0 {
        return Ok(None);
    }
    pipe.read_exact(&mut prefix[1..]).await?;
    let length = u32::from_le_bytes(prefix) as usize;
    if length == 0 {
        return Err(WireError::InvalidFrame.into());
    }
    if length > MAX_FRAME_BYTES {
        return Err(WireError::TooLarge.into());
    }
    let mut bytes = vec![0; length];
    pipe.read_exact(&mut bytes).await?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| WireError::InvalidFrame.into())
}
pub fn verify_peer<H: AsRawHandle>(
    pipe: &H,
    expected: u32,
    server_end: bool,
) -> Result<(), TransportError> {
    let handle: HANDLE = pipe.as_raw_handle().cast();
    let mut actual = 0;
    let success = unsafe {
        if server_end {
            GetNamedPipeClientProcessId(handle, &mut actual)
        } else {
            GetNamedPipeServerProcessId(handle, &mut actual)
        }
    };
    if success == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    if actual != expected {
        return Err(TransportError::PeerMismatch);
    }
    Ok(())
}
pub fn create_server(startup: &Startup) -> Result<NamedPipeServer, TransportError> {
    startup.validate()?;
    Ok(security::create_server(&startup.channel_name())?)
}

pub struct HostConnection {
    pipe: NamedPipeServer,
    process: Option<HostProcess>,
    process_id: u32,
    startup: Startup,
    sent: i128,
    received: i128,
    closed: bool,
}
impl HostConnection {
    pub async fn launch(executable: &Path) -> Result<Self, TransportError> {
        let startup = Startup::new();
        let pipe = create_server(&startup)?;
        let process = HostProcess::spawn(executable, &startup)?;
        timeout(IO_TIMEOUT, pipe.connect())
            .await
            .map_err(|_| TransportError::Timeout)??;
        verify_peer(&pipe, process.id(), true)?;
        let mut connection = Self {
            pipe,
            process_id: process.id(),
            process: Some(process),
            startup,
            sent: 0,
            received: 0,
            closed: false,
        };
        if !matches!(
            connection.exchange(HostMessage::Hello {}).await?,
            HostReply::Ready {}
        ) {
            return Err(TransportError::PeerMismatch);
        }
        Ok(connection)
    }
    pub fn process_id(&self) -> u32 {
        self.process_id
    }
    fn close(&mut self) {
        self.closed = true;
        self.pipe.disconnect().ok();
        self.process.take(); // Stop immediately if an acknowledgement cannot be trusted.
    }
    pub async fn exchange(&mut self, message: HostMessage) -> Result<HostReply, TransportError> {
        if self.closed {
            return Err(WireError::Closed.into());
        }
        let Some(sent) = self.sent.checked_add(1) else {
            self.close();
            return Err(WireError::OutOfOrder.into());
        };
        self.sent = sent;
        let expected = match &message {
            HostMessage::Hello {} => HostReply::Ready {},
            HostMessage::Snapshot { .. } | HostMessage::Heartbeat {} => HostReply::Heartbeat {},
            HostMessage::Privacy { enabled, .. } => HostReply::PrivacyApplied { enabled: *enabled },
            HostMessage::Shutdown {} => HostReply::Stopped {},
        };
        let frame = match self.startup.envelope(self.sent, message) {
            Ok(frame) => frame,
            Err(error) => {
                self.close();
                return Err(error);
            }
        };
        let result = timeout(IO_TIMEOUT, async {
            write(&mut self.pipe, &frame).await?;
            let reply: Envelope<HostReply> = read(&mut self.pipe)
                .await?
                .ok_or(TransportError::Protocol(WireError::Closed))?;
            if reply.protocol_version != PROTOCOL_VERSION
                || reply.host_instance_id != self.startup.instance
                || reply.nonce != self.startup.nonce
            {
                return Err(TransportError::PeerMismatch);
            }
            if reply.sequence.value() <= self.received {
                return Err(WireError::OutOfOrder.into());
            }
            if reply.body != expected {
                return Err(WireError::InvalidState.into());
            }
            self.received = reply.sequence.value();
            Ok(reply.body)
        })
        .await
        .map_err(|_| TransportError::Timeout)
        .and_then(|r| r);
        if result.is_err() {
            self.close();
        }
        result
    }
    pub async fn shutdown(&mut self) -> Result<(), TransportError> {
        let reply = self.exchange(HostMessage::Shutdown {}).await?;
        self.closed = true;
        if !matches!(reply, HostReply::Stopped {}) {
            return Err(TransportError::PeerMismatch);
        }
        Ok(())
    }
}

pub async fn run_host(startup: Startup) -> Result<(), TransportError> {
    startup.validate()?;
    let mut pipe = ClientOptions::new()
        .security_qos_flags(SECURITY_IDENTIFICATION)
        .open(startup.channel_name())?;
    verify_peer(&pipe, startup.parent_pid, false)?;
    let mut session = HostSession::new(startup.instance.clone(), startup.nonce.clone())?;
    let mut native = None;
    let mut sequence = 0i128;
    loop {
        let incoming = timeout(HEARTBEAT_DEADLINE, read::<_, HostMessage>(&mut pipe))
            .await
            .map_err(|_| TransportError::Timeout)??;
        let Some(incoming) = incoming else {
            session.close();
            return Ok(());
        };
        let update_native = matches!(
            incoming.body,
            HostMessage::Snapshot { .. } | HostMessage::Privacy { .. } | HostMessage::Shutdown {}
        );
        let reply = session.apply(incoming)?;
        if matches!(reply, HostReply::Ready {}) {
            native = Some(super::control::NativeController::start()?);
        }
        if update_native {
            // Native cache changes finish on the UI thread before any privacy/shutdown ACK.
            native
                .as_ref()
                .ok_or(TransportError::Native)?
                .replace(session.view().cloned())
                .await?;
        }
        let stopped = matches!(reply, HostReply::Stopped {});
        sequence = sequence
            .checked_add(1)
            .ok_or(TransportError::Protocol(WireError::OutOfOrder))?;
        write(&mut pipe, &startup.envelope(sequence, reply)?).await?;
        if stopped {
            return Ok(());
        }
    }
}
