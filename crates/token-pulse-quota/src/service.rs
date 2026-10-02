//! Serialized account owner. Independent of the usage database and UI visibility.
use crate::{AccountRequest, NativeService, ProtocolEvent, RpcReply, StdioSession};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use token_pulse_core::{
    error::ErrorCode,
    numeric::{DecimalInt, EpochMs},
    protocol::{QuotaSnapshot, QuotaState},
    quota::{
        AccountAvailability, QuotaChanged, QuotaCoordinator, QuotaReadToken, QuotaRefreshDecision,
        QuotaTime,
    },
};

#[derive(Clone, Copy)]
pub enum DisplayEntry {
    Main = 1,
    Mini = 2,
    Taskbar = 4,
}
pub struct RefreshReceipt {
    pub decision: QuotaRefreshDecision,
    pub snapshot: QuotaSnapshot,
}
enum Action {
    Unavailable {
        epoch: String,
    },
    Connect {
        spec: Arc<NativeService>,
        epoch: String,
    },
    Disconnect {
        epoch: String,
    },
    Select {
        id: String,
        epoch: String,
        revision: DecimalInt,
    },
    Refresh,
}
enum Receipt {
    Snapshot(QuotaSnapshot),
    Refresh(RefreshReceipt),
}
struct Command {
    action: Action,
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
    reply: SyncSender<Result<Receipt, ErrorCode>>,
}
struct Flags {
    stopping: AtomicBool,
    suspended: AtomicBool,
    refresh: AtomicBool,
    visible: AtomicU8,
}
pub struct AccountQuotaService {
    commands: SyncSender<Command>,
    snapshot: Arc<Mutex<QuotaSnapshot>>,
    flags: Arc<Flags>,
    worker: Mutex<Option<JoinHandle<()>>>,
}
impl AccountQuotaService {
    /// The callback receives no raw message, identity, URL, token, path or usage data.
    pub fn start(
        instance: &str,
        changed: Arc<dyn Fn(QuotaChanged) + Send + Sync>,
    ) -> Result<Self, ErrorCode> {
        let coordinator = QuotaCoordinator::new(instance)?;
        let snapshot = Arc::new(Mutex::new(coordinator.snapshot()));
        let flags = Arc::new(Flags {
            stopping: AtomicBool::new(false),
            suspended: AtomicBool::new(false),
            refresh: AtomicBool::new(false),
            visible: AtomicU8::new(0),
        });
        let (commands, receiver) = mpsc::sync_channel(16);
        let mut driver = Driver {
            coordinator,
            clock: Clock(Instant::now()),
            session: None,
            desired: None,
            launch_pending: false,
            transport_epoch: String::new(),
            reads: BTreeMap::new(),
            account_proven: false,
            account_pending: false,
            forced: false,
            reset_seen: BTreeMap::new(),
            retry_at: None,
            failures: 0,
            snapshot: snapshot.clone(),
            flags: flags.clone(),
            changed,
        };
        let worker = thread::Builder::new()
            .name("tokenpulse-quota".into())
            .spawn(move || driver.run(receiver))
            .map_err(|_| ErrorCode::QuotaServiceUnavailable)?;
        Ok(Self {
            commands,
            snapshot,
            flags,
            worker: Mutex::new(Some(worker)),
        })
    }
    pub fn snapshot(&self) -> Result<QuotaSnapshot, ErrorCode> {
        self.snapshot
            .lock()
            .map(|s| s.clone())
            .map_err(|_| ErrorCode::QuotaServiceUnavailable)
    }
    fn request(&self, action: Action) -> Result<Receipt, ErrorCode> {
        if self.flags.stopping.load(Ordering::Acquire) {
            return Err(ErrorCode::QuotaDisconnected);
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let (reply, answer) = mpsc::sync_channel(1);
        self.commands
            .try_send(Command {
                action,
                deadline: Instant::now() + Duration::from_secs(12),
                cancelled: cancelled.clone(),
                reply,
            })
            .map_err(|_| ErrorCode::QuotaServiceUnavailable)?;
        let result = answer.recv_timeout(Duration::from_secs(12));
        if result.is_err() {
            cancelled.store(true, Ordering::Release);
        }
        result.map_err(|_| ErrorCode::QuotaTimeout)?
    }
    /// Called only after explicit backend configuration / consent. No automatic detection launch.
    pub fn connect(
        &self,
        spec: NativeService,
        expected_epoch: &str,
    ) -> Result<QuotaSnapshot, ErrorCode> {
        match self.request(Action::Connect {
            spec: Arc::new(spec),
            epoch: expected_epoch.into(),
        })? {
            Receipt::Snapshot(s) => Ok(s),
            _ => Err(ErrorCode::QuotaProtocolError),
        }
    }
    /// A configured startup target failed local validation; do not silently look connected.
    pub fn report_unavailable(&self, expected_epoch: &str) -> Result<QuotaSnapshot, ErrorCode> {
        match self.request(Action::Unavailable {
            epoch: expected_epoch.into(),
        })? {
            Receipt::Snapshot(s) => Ok(s),
            _ => Err(ErrorCode::QuotaProtocolError),
        }
    }
    pub fn disconnect(&self, expected_epoch: &str) -> Result<QuotaSnapshot, ErrorCode> {
        match self.request(Action::Disconnect {
            epoch: expected_epoch.into(),
        })? {
            Receipt::Snapshot(s) => Ok(s),
            _ => Err(ErrorCode::QuotaProtocolError),
        }
    }
    pub fn select_limit(
        &self,
        id: &str,
        expected_epoch: &str,
        expected_revision: DecimalInt,
    ) -> Result<QuotaSnapshot, ErrorCode> {
        match self.request(Action::Select {
            id: id.into(),
            epoch: expected_epoch.into(),
            revision: expected_revision,
        })? {
            Receipt::Snapshot(s) => Ok(s),
            _ => Err(ErrorCode::QuotaProtocolError),
        }
    }
    pub fn refresh(&self) -> Result<RefreshReceipt, ErrorCode> {
        match self.request(Action::Refresh)? {
            Receipt::Refresh(s) => Ok(s),
            _ => Err(ErrorCode::QuotaProtocolError),
        }
    }
    pub fn set_visible(&self, entry: DisplayEntry, visible: bool) {
        let mask = entry as u8;
        let previous = if visible {
            self.flags.visible.fetch_or(mask, Ordering::AcqRel)
        } else {
            self.flags.visible.fetch_and(!mask, Ordering::AcqRel)
        };
        if visible && previous == 0 {
            self.flags.refresh.store(true, Ordering::Release);
        }
    }
    pub fn suspend(&self) {
        self.flags.suspended.store(true, Ordering::Release);
    }
    pub fn resume(&self) {
        self.flags.suspended.store(false, Ordering::Release);
        self.flags.refresh.store(true, Ordering::Release);
    }
    pub fn shutdown(&self) {
        self.flags.stopping.store(true, Ordering::Release);
        if let Ok(mut worker) = self.worker.lock() {
            if let Some(worker) = worker.take() {
                let _ = worker.join();
            }
        }
    }
}
impl Drop for AccountQuotaService {
    fn drop(&mut self) {
        self.shutdown();
    }
}
struct Clock(Instant);
impl Clock {
    fn now(&self) -> Result<QuotaTime, ErrorCode> {
        let wall = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => i64::try_from(d.as_millis()).map_err(|_| ErrorCode::NumericOverflow)?,
            Err(e) => {
                -i64::try_from(e.duration().as_millis()).map_err(|_| ErrorCode::NumericOverflow)?
            }
        };
        Ok(QuotaTime {
            wall: EpochMs::new(wall)?,
            monotonic_ms: u64::try_from(self.0.elapsed().as_millis())
                .map_err(|_| ErrorCode::NumericOverflow)?,
        })
    }
}
enum Pending {
    Account { epoch: String },
    Limits { token: QuotaReadToken },
}
struct Driver {
    coordinator: QuotaCoordinator,
    clock: Clock,
    session: Option<StdioSession>,
    desired: Option<Arc<NativeService>>,
    launch_pending: bool,
    transport_epoch: String,
    reads: BTreeMap<String, Pending>,
    account_proven: bool,
    account_pending: bool,
    forced: bool,
    reset_seen: BTreeMap<String, EpochMs>,
    retry_at: Option<Instant>,
    failures: usize,
    snapshot: Arc<Mutex<QuotaSnapshot>>,
    flags: Arc<Flags>,
    changed: Arc<dyn Fn(QuotaChanged) + Send + Sync>,
}
impl Driver {
    fn publish(&self) {
        let current = self.coordinator.snapshot();
        let notification = QuotaChanged::from(&current);
        let changed = if let Ok(mut cached) = self.snapshot.lock() {
            if cached.quota_revision != current.quota_revision {
                *cached = current;
                true
            } else {
                false
            }
        } else {
            false
        };
        if changed {
            (self.changed)(notification);
        }
    }
    fn clear_transport(&mut self) {
        self.session.take();
        self.reads.clear();
        self.account_proven = false;
        self.account_pending = false;
        self.reset_seen.clear();
    }
    fn identity_reset(&mut self) -> Result<(), ErrorCode> {
        self.reads.clear();
        self.account_proven = false;
        self.reset_seen.clear();
        self.forced = false;
        self.coordinator.account_changed(self.clock.now()?)?;
        self.publish();
        self.account_pending = true;
        Ok(())
    }
    fn query_account(&mut self) -> Result<(), ErrorCode> {
        let token = self
            .session
            .as_mut()
            .ok_or(ErrorCode::QuotaDisconnected)?
            .send(AccountRequest::ReadAccount)?;
        self.reads.insert(
            token.request_id,
            Pending::Account {
                epoch: self.coordinator.epoch(),
            },
        );
        self.account_pending = false;
        Ok(())
    }
    fn failure(&mut self, error: ErrorCode) -> Result<(), ErrorCode> {
        self.clear_transport();
        let error = controlled(error);
        self.coordinator
            .connection_failed(&self.coordinator.epoch(), error, self.clock.now()?)?;
        if matches!(
            error,
            ErrorCode::QuotaTimeout
                | ErrorCode::QuotaServiceUnavailable
                | ErrorCode::QuotaProtocolError
        ) && self.desired.is_some()
        {
            self.failures = (self.failures + 1).min(4);
            self.retry_at =
                Some(Instant::now() + Duration::from_secs([5, 15, 30, 60][self.failures - 1]));
        } else {
            self.retry_at = None;
        }
        self.launch_pending = false;
        self.publish();
        Ok(())
    }
    fn launch(&mut self) -> Result<(), ErrorCode> {
        self.launch_pending = false;
        let Some(spec) = self.desired.clone() else {
            return Ok(());
        };
        // A fresh transport cannot reuse unproven cached account identity.
        self.transport_epoch = self.coordinator.epoch();
        match StdioSession::launch(&spec, &self.transport_epoch) {
            Ok(session) => self.session = Some(session),
            Err(code) => self.failure(code)?,
        }
        Ok(())
    }
    fn refresh(&mut self, explicit: bool) -> Result<QuotaRefreshDecision, ErrorCode> {
        if self.flags.suspended.load(Ordering::Acquire) {
            return Err(ErrorCode::QuotaServiceUnavailable);
        }
        if !self.account_proven || self.session.is_none() {
            return Err(match self.coordinator.snapshot().state {
                QuotaState::AuthorizationRequired => ErrorCode::QuotaAuthRequired,
                QuotaState::Unsupported => ErrorCode::QuotaUnsupported,
                _ => ErrorCode::QuotaDisconnected,
            });
        }
        let decision = self.coordinator.begin_refresh(
            self.clock.now()?,
            explicit,
            self.flags.visible.load(Ordering::Acquire) != 0,
        )?;
        if let QuotaRefreshDecision::Started(token) = &decision {
            match self
                .session
                .as_mut()
                .expect("checked session")
                .send(AccountRequest::ReadLimits)
            {
                Ok(rpc) => {
                    self.reads.insert(
                        rpc.request_id,
                        Pending::Limits {
                            token: token.clone(),
                        },
                    );
                    self.forced = false;
                }
                Err(code) => {
                    self.failure(code)?;
                    return Err(controlled(code));
                }
            }
        }
        self.publish();
        Ok(decision)
    }
    fn action(&mut self, action: Action) -> Result<Receipt, ErrorCode> {
        let result = match action {
            Action::Unavailable { epoch } => {
                if epoch != self.coordinator.epoch() {
                    return Err(ErrorCode::RevisionConflict);
                }
                self.clear_transport();
                self.desired = None;
                self.retry_at = None;
                self.launch_pending = false;
                let epoch = self.coordinator.begin_connection(self.clock.now()?)?;
                self.coordinator.connection_failed(
                    &epoch,
                    ErrorCode::QuotaServiceUnavailable,
                    self.clock.now()?,
                )?;
                Receipt::Snapshot(self.coordinator.snapshot())
            }
            Action::Connect { spec, epoch } => {
                if epoch != self.coordinator.epoch() {
                    return Err(ErrorCode::RevisionConflict);
                }
                self.clear_transport();
                self.desired = Some(spec);
                self.retry_at = None;
                self.failures = 0;
                self.forced = false;
                self.coordinator.begin_connection(self.clock.now()?)?;
                self.launch_pending = true;
                Receipt::Snapshot(self.coordinator.snapshot())
            }
            Action::Disconnect { epoch } => {
                if epoch != self.coordinator.epoch() {
                    return Err(ErrorCode::RevisionConflict);
                }
                self.clear_transport();
                self.desired = None;
                self.retry_at = None;
                self.launch_pending = false;
                self.forced = false;
                self.coordinator.disconnect(self.clock.now()?)?;
                Receipt::Snapshot(self.coordinator.snapshot())
            }
            Action::Select {
                id,
                epoch,
                revision,
            } => {
                if epoch != self.coordinator.epoch()
                    || revision != self.coordinator.snapshot().quota_revision
                {
                    return Err(ErrorCode::RevisionConflict);
                }
                self.coordinator.select_limit(&id)?;
                self.reset_seen.clear();
                Receipt::Snapshot(self.coordinator.snapshot())
            }
            Action::Refresh => Receipt::Refresh(RefreshReceipt {
                decision: self.refresh(true)?,
                snapshot: self.coordinator.snapshot(),
            }),
        };
        self.publish();
        Ok(result)
    }
    fn event(&mut self, event: ProtocolEvent) -> Result<(), ErrorCode> {
        match event {
            ProtocolEvent::AccountChanged => self.identity_reset()?,
            ProtocolEvent::LimitsUpdated(update) => {
                if !self.coordinator.notification(
                    &self.coordinator.epoch(),
                    update,
                    self.clock.now()?,
                )? && self.account_proven
                {
                    self.forced = true;
                }
            }
            ProtocolEvent::Reply { token, result } => {
                if token.connection_epoch != self.transport_epoch {
                    return Ok(());
                }
                if matches!(result, Ok(RpcReply::Initialized)) {
                    self.account_pending = true;
                } else if let Some(pending) = self.reads.remove(&token.request_id) {
                    match pending {
                        Pending::Account { epoch } => match result {
                            Ok(RpcReply::Account(availability)) => {
                                self.account_proven =
                                    availability == AccountAvailability::QuotaEligible;
                                self.coordinator.account_result(
                                    &epoch,
                                    availability,
                                    self.clock.now()?,
                                )?;
                                if self.account_proven {
                                    self.forced = true;
                                }
                            }
                            Err(code) => self.failure(code)?,
                            _ => self.failure(ErrorCode::QuotaProtocolError)?,
                        },
                        Pending::Limits { token } => match result {
                            Ok(RpcReply::Limits(book)) => {
                                self.coordinator
                                    .read_succeeded(&token, book, self.clock.now()?)?;
                                self.failures = 0;
                            }
                            Err(code) => {
                                let error = controlled(code);
                                self.coordinator
                                    .read_failed(&token, error, self.clock.now()?)?;
                                if matches!(
                                    error,
                                    ErrorCode::QuotaUnsupported | ErrorCode::QuotaAuthRequired
                                ) {
                                    self.account_proven = false;
                                }
                            }
                            _ => {
                                self.coordinator.read_failed(
                                    &token,
                                    ErrorCode::QuotaProtocolError,
                                    self.clock.now()?,
                                )?;
                            }
                        },
                    }
                } else if let Err(code) = result {
                    // Only initialization lacks a driver read record.
                    self.failure(code)?;
                }
            }
            ProtocolEvent::TimedOut { token } => {
                if token.connection_epoch != self.transport_epoch {
                    return Ok(());
                }
                match self.reads.remove(&token.request_id) {
                    Some(Pending::Limits { token }) => {
                        self.coordinator.read_failed(
                            &token,
                            ErrorCode::QuotaTimeout,
                            self.clock.now()?,
                        )?;
                    }
                    _ => self.failure(ErrorCode::QuotaTimeout)?,
                }
            }
        }
        self.publish();
        Ok(())
    }
    fn step(&mut self) -> Result<(), ErrorCode> {
        self.coordinator.tick(self.clock.now()?)?;
        if self.flags.refresh.swap(false, Ordering::AcqRel) {
            self.forced = true;
        }
        if self.flags.suspended.load(Ordering::Acquire) {
            self.publish();
            return Ok(());
        }
        if self.retry_at.is_some_and(|at| Instant::now() >= at) {
            self.retry_at = None;
            self.coordinator.begin_connection(self.clock.now()?)?;
            self.launch_pending = true;
            self.publish();
        }
        if self.launch_pending {
            self.launch()?;
        }
        if self.account_pending && self.session.as_ref().is_some_and(StdioSession::is_ready) {
            self.query_account()?;
        }
        let snapshot = self.coordinator.snapshot();
        let now = self.clock.now()?;
        // Trigger once for each actual reset in the selected bucket. Never refill values.
        for window in &snapshot.windows {
            if let Some(reset) = window.resets_at_ms {
                if reset <= now.wall && self.reset_seen.get(&window.window_id) != Some(&reset) {
                    self.reset_seen.insert(window.window_id.clone(), reset);
                    self.forced = true;
                }
            }
        }
        if self.account_proven {
            let _ = self.refresh(self.forced)?;
        }
        self.publish();
        Ok(())
    }
    fn run(&mut self, commands: Receiver<Command>) {
        while !self.flags.stopping.load(Ordering::Acquire) {
            for command in commands.try_iter().take(16) {
                let result = if command.cancelled.load(Ordering::Acquire)
                    || Instant::now() >= command.deadline
                {
                    Err(ErrorCode::QuotaTimeout)
                } else {
                    self.action(command.action)
                };
                let _ = command.reply.send(result);
            }
            if self.flags.stopping.load(Ordering::Acquire) {
                break;
            }
            if let Err(code) = self.step() {
                let _ = self.failure(code);
            }
            if let Some(session) = &mut self.session {
                let event = session.next_event(Duration::from_millis(100));
                match event {
                    Ok(Some(event)) => {
                        if let Err(code) = self.event(event) {
                            let _ = self.failure(code);
                        }
                    }
                    Ok(None) => {}
                    Err(code) => {
                        let _ = self.failure(code);
                    }
                }
            } else {
                thread::park_timeout(Duration::from_millis(50));
            }
        }
        self.clear_transport();
        self.desired = None;
        if let Ok(at) = self.clock.now() {
            let _ = self.coordinator.disconnect(at);
        }
        self.publish();
    }
}
fn controlled(error: ErrorCode) -> ErrorCode {
    match error {
        ErrorCode::QuotaTimeout
        | ErrorCode::QuotaProtocolError
        | ErrorCode::QuotaServiceUnavailable
        | ErrorCode::QuotaUnsupported
        | ErrorCode::QuotaAuthRequired => error,
        _ => ErrorCode::QuotaServiceUnavailable,
    }
}
