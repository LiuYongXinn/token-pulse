//! Dedicated host owner. No cross-process native call runs on Tauri's UI thread.
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use token_pulse_core::{error::ErrorCode, numeric::DecimalInt, taskbar::*};
use token_pulse_taskbar::TaskbarView;
pub struct Input {
    pub configuration: TaskbarPreferencesSnapshot,
    pub privacy: bool,
    pub view: Option<TaskbarView>,
}
pub type Reader = Arc<dyn Fn() -> Result<Input, ErrorCode> + Send + Sync>;
pub type Changed = Arc<dyn Fn(TaskbarRuntimeSnapshot) + Send + Sync>;
pub type Visible = Arc<dyn Fn(bool) + Send + Sync>;
pub type Fallback = Arc<dyn Fn(FallbackRequest) -> Result<bool, ErrorCode> + Send + Sync>;
#[cfg(windows)]
struct Callbacks {
    changed: Changed,
    visible: Visible,
    fallback: Fallback,
}
#[derive(Clone)]
pub struct FallbackRequest {
    pub show: bool,
    flags: Arc<Flags>,
    generation: u64,
    snapshot: Arc<Mutex<TaskbarRuntimeSnapshot>>,
    cancelled: Arc<AtomicBool>,
}
impl FallbackRequest {
    pub fn valid(&self) -> bool {
        !self.cancelled.load(Ordering::Acquire) && self.live_generation()
    }
    fn live_generation(&self) -> bool {
        !self.flags.stopping.load(Ordering::Acquire)
            && !self.flags.paused.load(Ordering::Acquire)
            && !self.flags.suspended.load(Ordering::Acquire)
            && self.generation == self.flags.generation.load(Ordering::Acquire)
    }
    pub fn current(&self) -> bool {
        self.valid()
            && (!self.show
                || self
                    .snapshot
                    .lock()
                    .ok()
                    .is_some_and(|s| needs_fallback(&s)))
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}
fn needs_fallback(snapshot: &TaskbarRuntimeSnapshot) -> bool {
    snapshot.state == TaskbarRuntimeState::Unavailable
        || (snapshot.state == TaskbarRuntimeState::Recovering && snapshot.issue.is_some())
}
#[cfg(windows)]
#[derive(Default)]
struct FallbackPolicy {
    attempted: bool,
    active: bool,
}
#[cfg(windows)]
impl FallbackPolicy {
    fn action(&mut self, enabled: bool, snapshot: &TaskbarRuntimeSnapshot) -> Option<bool> {
        if !enabled {
            self.attempted = false;
            self.active = false;
            return None;
        }
        if matches!(
            snapshot.state,
            TaskbarRuntimeState::Embedded | TaskbarRuntimeState::Disabled
        ) {
            self.attempted = false;
        }
        if needs_fallback(snapshot) && !self.attempted {
            Some(true)
        } else if self.active {
            Some(false)
        } else {
            None
        }
    }
    fn completed(&mut self, show: bool, result: &Result<bool, ErrorCode>) {
        if show {
            self.attempted = true;
        }
        self.active = result.as_ref().is_ok_and(|visible| *visible);
    }
}
enum Command {
    Clear(mpsc::SyncSender<Result<(), ErrorCode>>),
}
struct Flags {
    stopping: AtomicBool,
    paused: AtomicBool,
    suspended: AtomicBool,
    generation: AtomicU64,
}
pub struct TaskbarService {
    flags: Arc<Flags>,
    snapshot: Arc<Mutex<TaskbarRuntimeSnapshot>>,
    commands: mpsc::SyncSender<Command>,
    worker: Mutex<Option<JoinHandle<()>>>,
    exiting: AtomicBool,
    pub exit_ready: AtomicBool,
}
pub struct PublicationPause<'a>(&'a TaskbarService);
impl Drop for PublicationPause<'_> {
    fn drop(&mut self) {
        self.0.flags.generation.fetch_add(1, Ordering::AcqRel);
        self.0.flags.paused.store(false, Ordering::Release);
    }
}
impl TaskbarService {
    pub fn start(
        executable: std::path::PathBuf,
        reader: Reader,
        changed: Changed,
        visible: Visible,
        fallback: Fallback,
    ) -> Result<Self, ErrorCode> {
        let flags = Arc::new(Flags {
            stopping: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            suspended: AtomicBool::new(false),
            generation: AtomicU64::new(0),
        });
        let snapshot = Arc::new(Mutex::new(TaskbarRuntimeSnapshot {
            revision: DecimalInt::parse("0")?,
            state: TaskbarRuntimeState::Disabled,
            applied_settings_revision: None,
            issue: None,
            error: None,
            compact: None,
            fallback_visible: None,
            fallback_error: None,
            last_cleanup: None,
            last_snapshot_at_ms: None,
        }));
        let (commands, receiver) = mpsc::sync_channel(4);
        let owned_flags = flags.clone();
        let owned_snapshot = snapshot.clone();
        let worker = thread::Builder::new()
            .name("tokenpulse-taskbar".into())
            .spawn(move || {
                #[cfg(windows)]
                match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => {
                        runtime.block_on(run(
                            executable,
                            reader,
                            Callbacks {
                                changed,
                                visible,
                                fallback,
                            },
                            owned_flags,
                            owned_snapshot,
                            receiver,
                        ));
                        runtime.shutdown_timeout(Duration::from_secs(1));
                    }
                    Err(_) => publish(
                        &owned_snapshot,
                        &changed,
                        TaskbarRuntimeState::Unavailable,
                        None,
                        Some(TaskbarRuntimeIssue::HostUnavailable),
                        Some(ErrorCode::TaskbarEmbedFailed),
                        None,
                    ),
                }
                #[cfg(not(windows))]
                {
                    let _ = (executable, visible, fallback);
                    while !owned_flags.stopping.load(Ordering::Acquire) {
                        while let Ok(Command::Clear(reply)) = receiver.try_recv() {
                            let _ = reply.send(Ok(()));
                        }
                        if let Ok(input) = reader() {
                            publish(
                                &owned_snapshot,
                                &changed,
                                if input.configuration.preferences.enabled {
                                    TaskbarRuntimeState::Unavailable
                                } else {
                                    TaskbarRuntimeState::Disabled
                                },
                                Some(input.configuration.settings_revision),
                                input
                                    .configuration
                                    .preferences
                                    .enabled
                                    .then_some(TaskbarRuntimeIssue::UnsupportedVersion),
                                input
                                    .configuration
                                    .preferences
                                    .enabled
                                    .then_some(ErrorCode::TaskbarUnsupported),
                                None,
                            );
                        }
                        thread::sleep(Duration::from_millis(50));
                    }
                }
            })
            .map_err(|_| ErrorCode::TaskbarEmbedFailed)?;
        Ok(Self {
            flags,
            snapshot,
            commands,
            worker: Mutex::new(Some(worker)),
            exiting: AtomicBool::new(false),
            exit_ready: AtomicBool::new(false),
        })
    }
    pub fn snapshot(&self) -> Result<TaskbarRuntimeSnapshot, ErrorCode> {
        self.snapshot
            .lock()
            .map(|s| s.clone())
            .map_err(|_| ErrorCode::TaskbarEmbedFailed)
    }
    pub fn invalidate(&self) {
        self.flags.generation.fetch_add(1, Ordering::AcqRel);
    }
    /// Must be called from a blocking worker, before a privacy transaction takes its policy lock.
    pub fn pause_publication(&self) -> Result<PublicationPause<'_>, ErrorCode> {
        if self.flags.stopping.load(Ordering::Acquire)
            || self.flags.paused.swap(true, Ordering::AcqRel)
        {
            return Err(ErrorCode::TaskbarEmbedFailed);
        }
        self.invalidate();
        let (reply, answer) = mpsc::sync_channel(1);
        let pause = PublicationPause(self);
        self.commands
            .try_send(Command::Clear(reply))
            .map_err(|_| ErrorCode::TaskbarEmbedFailed)?;
        answer
            .recv_timeout(Duration::from_secs(15))
            .map_err(|_| ErrorCode::TaskbarEmbedFailed)??;
        Ok(pause)
    }
    pub fn suspend(&self) {
        self.flags.suspended.store(true, Ordering::Release);
        self.invalidate();
    }
    pub fn resume(&self) {
        self.flags.suspended.store(false, Ordering::Release);
        self.invalidate();
    }
    pub fn begin_exit(&self) -> bool {
        !self.exiting.swap(true, Ordering::AcqRel)
    }
    /// Dedicated exit waiter only; main UI must keep dispatching while the worker finishes.
    pub fn shutdown(&self) {
        self.flags.stopping.store(true, Ordering::Release);
        if let Ok(mut owned) = self.worker.lock() {
            if let Some(worker) = owned.take() {
                let _ = worker.join();
            }
        }
        self.exit_ready.store(true, Ordering::Release);
    }
}
fn publish(
    snapshot: &Mutex<TaskbarRuntimeSnapshot>,
    changed: &Changed,
    state: TaskbarRuntimeState,
    applied: Option<DecimalInt>,
    issue: Option<TaskbarRuntimeIssue>,
    error: Option<ErrorCode>,
    compact: Option<bool>,
) {
    let update = if let Ok(mut current) = snapshot.lock() {
        let mut next = TaskbarRuntimeSnapshot {
            revision: current.revision.clone(),
            state,
            applied_settings_revision: applied,
            issue,
            error,
            compact,
            fallback_visible: current.fallback_visible,
            fallback_error: current.fallback_error,
            last_cleanup: current.last_cleanup,
            last_snapshot_at_ms: current.last_snapshot_at_ms,
        };
        if *current == next {
            None
        } else {
            next.revision =
                DecimalInt::from_nonnegative(current.revision.value().saturating_add(1))
                    .expect("bounded service revision");
            *current = next.clone();
            Some(next)
        }
    } else {
        None
    };
    if let Some(update) = update {
        changed(update);
    }
}
#[cfg(windows)]
async fn run(
    executable: std::path::PathBuf,
    reader: Reader,
    callbacks: Callbacks,
    flags: Arc<Flags>,
    snapshot: Arc<Mutex<TaskbarRuntimeSnapshot>>,
    receiver: mpsc::Receiver<Command>,
) {
    let Callbacks {
        changed,
        visible,
        fallback,
    } = callbacks;
    use std::time::Instant;
    use token_pulse_taskbar::{
        HostConfiguration, HostMessage, HostReply, windows::transport::HostConnection,
    };
    let mut connection: Option<HostConnection> = None;
    let mut policy: Option<(DecimalInt, bool)> = None;
    let mut configuration = None;
    let mut pending: Option<(u64, tokio::task::JoinHandle<Result<Input, ErrorCode>>)> = None;
    let mut retired: Option<tokio::task::JoinHandle<Result<Input, ErrorCode>>> = None;
    let mut next_read = Instant::now();
    let mut retry_at = Instant::now();
    let mut failures: usize = 0;
    let mut observed_generation = flags.generation.load(Ordering::Acquire);
    let mut last_poll = Instant::now();
    let mut render_backoff = false;
    let mut native_revision = None;
    let mut fallback_enabled = None;
    let mut fallback_policy = FallbackPolicy::default();
    let mut fallback_work: Option<(
        FallbackRequest,
        tokio::task::JoinHandle<Result<bool, ErrorCode>>,
    )> = None;
    let mut next_fallback = Instant::now();
    while !flags.stopping.load(Ordering::Acquire) {
        let current_generation = flags.generation.load(Ordering::Acquire);
        if current_generation != observed_generation {
            observed_generation = current_generation;
            retry_at = Instant::now();
            next_read = Instant::now();
            failures = 0;
            render_backoff = false;
            fallback_policy.attempted = false;
        }
        while let Ok(Command::Clear(reply)) = receiver.try_recv() {
            if retired.as_ref().is_none_or(|work| work.is_finished()) {
                retired = pending.take().map(|(_, work)| work);
            }
            let result = if let (Some(host), Some((revision, privacy))) =
                (connection.as_mut(), policy.as_ref())
            {
                host.exchange(HostMessage::Privacy {
                    settings_revision: revision.clone(),
                    enabled: *privacy,
                })
                .await
                .map(|_| ())
                .map_err(|_| ErrorCode::TaskbarEmbedFailed)
            } else {
                Ok(())
            };
            if result.is_err() {
                close_host(&mut connection, &snapshot, &changed).await;
                policy = None;
                configuration = None;
            }
            publish(
                &snapshot,
                &changed,
                TaskbarRuntimeState::Recovering,
                None,
                None,
                None,
                None,
            );
            visible(false);
            let _ = reply.send(result);
        }
        if flags.suspended.load(Ordering::Acquire) {
            close_host(&mut connection, &snapshot, &changed).await;
            policy = None;
            configuration = None;
            if retired.as_ref().is_none_or(|work| work.is_finished()) {
                retired = pending.take().map(|(_, work)| work);
            }
            visible(false);
            publish(
                &snapshot,
                &changed,
                TaskbarRuntimeState::Suspended,
                None,
                None,
                None,
                None,
            );
        }
        if flags.paused.load(Ordering::Acquire) || flags.suspended.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(20)).await;
            continue;
        }
        if fallback_work
            .as_ref()
            .is_some_and(|(_, work)| work.is_finished())
        {
            let (request, work) = fallback_work.take().expect("finished fallback");
            let result = work.await.unwrap_or(Err(ErrorCode::WindowUnavailable));
            if request.valid() || (result.is_err() && request.live_generation()) {
                fallback_policy.completed(request.show, &result);
                publish_fallback(&snapshot, &changed, result);
            }
        }
        if fallback_enabled == Some(false) {
            fallback_policy = FallbackPolicy::default();
            publish_fallback(&snapshot, &changed, Ok(false));
        } else if fallback_enabled == Some(true)
            && fallback_work.is_none()
            && Instant::now() >= next_fallback
        {
            let action = snapshot
                .lock()
                .ok()
                .and_then(|s| fallback_policy.action(true, &s));
            if let Some(show) = action {
                let request = FallbackRequest {
                    show,
                    flags: flags.clone(),
                    generation: current_generation,
                    snapshot: snapshot.clone(),
                    cancelled: Arc::new(AtomicBool::new(false)),
                };
                let work_request = request.clone();
                let callback = fallback.clone();
                fallback_work = Some((
                    request,
                    tokio::task::spawn_blocking(move || callback(work_request)),
                ));
            }
            next_fallback = Instant::now() + Duration::from_secs(1);
        }
        if retired.as_ref().is_some_and(|work| work.is_finished()) {
            retired.take();
        }
        if pending.is_none() && Instant::now() >= next_read {
            let read = reader.clone();
            pending = Some((
                flags.generation.load(Ordering::Acquire),
                tokio::task::spawn_blocking(move || read()),
            ));
            next_read = Instant::now() + Duration::from_secs(1);
        }
        if pending.as_ref().is_some_and(|(_, work)| work.is_finished()) {
            let (generation, work) = pending.take().expect("finished read");
            let input = work.await.unwrap_or(Err(ErrorCode::TaskbarEmbedFailed));
            if generation != flags.generation.load(Ordering::Acquire) {
                next_read = Instant::now();
                continue;
            }
            let input = match input {
                Ok(input) => input,
                Err(error) => {
                    close_host(&mut connection, &snapshot, &changed).await;
                    policy = None;
                    configuration = None;
                    visible(false);
                    publish(
                        &snapshot,
                        &changed,
                        TaskbarRuntimeState::Unavailable,
                        None,
                        Some(TaskbarRuntimeIssue::InputUnavailable),
                        Some(error),
                        None,
                    );
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    continue;
                }
            };
            fallback_enabled = Some(
                input.configuration.preferences.enabled
                    && input.configuration.preferences.fallback_to_mini,
            );
            if !input.configuration.preferences.enabled
                || input.configuration.preferences.position != TaskbarPosition::NotificationLeft
            {
                close_host(&mut connection, &snapshot, &changed).await;
                policy = None;
                configuration = None;
                visible(false);
                failures = 0;
                let disabled = !input.configuration.preferences.enabled;
                publish(
                    &snapshot,
                    &changed,
                    if disabled {
                        TaskbarRuntimeState::Disabled
                    } else {
                        TaskbarRuntimeState::Unavailable
                    },
                    Some(input.configuration.settings_revision),
                    (!disabled).then_some(TaskbarRuntimeIssue::UnsupportedPosition),
                    (!disabled).then_some(ErrorCode::TaskbarUnsupported),
                    None,
                );
                continue;
            }
            if configuration
                .as_ref()
                .is_some_and(|old: &HostConfiguration| {
                    old.settings_revision != input.configuration.settings_revision
                })
            {
                failures = 0;
                render_backoff = false;
                retry_at = Instant::now();
            }
            if connection.is_none() && failures < 5 && Instant::now() >= retry_at {
                publish(
                    &snapshot,
                    &changed,
                    TaskbarRuntimeState::Probing,
                    None,
                    None,
                    None,
                    None,
                );
                match HostConnection::launch(&executable).await {
                    Ok(host) => {
                        connection = Some(host);
                        policy = None;
                        configuration = None;
                    }
                    Err(_) => {
                        failures = failures.saturating_add(1);
                        retry_at = Instant::now() + retry_delay(failures);
                        publish(
                            &snapshot,
                            &changed,
                            TaskbarRuntimeState::Unavailable,
                            None,
                            Some(TaskbarRuntimeIssue::HostUnavailable),
                            Some(ErrorCode::TaskbarEmbedFailed),
                            None,
                        );
                    }
                }
            }
            if flags.paused.load(Ordering::Acquire)
                || generation != flags.generation.load(Ordering::Acquire)
            {
                next_read = Instant::now();
                continue;
            }
            if render_backoff && (failures >= 5 || Instant::now() < retry_at) {
                continue;
            }
            if let Some(host) = connection.as_mut() {
                let revision = input.configuration.settings_revision.clone();
                let new_policy = (revision.clone(), input.privacy);
                let new_configuration = HostConfiguration {
                    settings_revision: revision,
                    enabled: true,
                    display: input.configuration.preferences.display,
                };
                let result = async {
                    if policy.as_ref() != Some(&new_policy) {
                        host.exchange(HostMessage::Privacy {
                            settings_revision: new_policy.0.clone(),
                            enabled: new_policy.1,
                        })
                        .await?;
                        policy = Some(new_policy);
                    }
                    if configuration.as_ref() != Some(&new_configuration) {
                        host.exchange(HostMessage::Configure {
                            configuration: new_configuration.clone(),
                        })
                        .await?;
                        configuration = Some(new_configuration);
                    }
                    if flags.paused.load(Ordering::Acquire)
                        || generation != flags.generation.load(Ordering::Acquire)
                    {
                        return Ok(None);
                    }
                    if let Some(view) = input.view {
                        let published_at = view.generated_at_ms;
                        host.exchange(HostMessage::Snapshot {
                            view: Box::new(view),
                        })
                        .await?;
                        let update = snapshot.lock().ok().map(|mut current| {
                            current.last_snapshot_at_ms = Some(published_at);
                            current.revision = DecimalInt::from_nonnegative(
                                current.revision.value().saturating_add(1),
                            )
                            .expect("bounded service revision");
                            current.clone()
                        });
                        if let Some(update) = update {
                            changed(update);
                        }
                    }
                    let HostReply::Status { status } =
                        host.exchange(HostMessage::GetStatus {}).await?
                    else {
                        return Err(token_pulse_taskbar::windows::TransportError::Native);
                    };
                    Ok(Some(status))
                }
                .await;
                match result {
                    Ok(Some(status)) => {
                        native_revision = Some(status.system_revision.clone());
                        render_backoff =
                            status.state == token_pulse_taskbar::HostDisplayState::Unavailable;
                        if render_backoff {
                            failures = failures.saturating_add(1);
                            retry_at = Instant::now() + retry_delay(failures);
                        } else {
                            failures = 0;
                        }
                        publish_host_status(&snapshot, &changed, &visible, status);
                        last_poll = Instant::now();
                    }
                    Ok(None) => {}
                    Err(_) => {
                        close_host(&mut connection, &snapshot, &changed).await;
                        policy = None;
                        configuration = None;
                        visible(false);
                        failures = failures.saturating_add(1);
                        retry_at = Instant::now() + retry_delay(failures);
                        publish(
                            &snapshot,
                            &changed,
                            TaskbarRuntimeState::Recovering,
                            None,
                            Some(TaskbarRuntimeIssue::ProtocolError),
                            Some(ErrorCode::TaskbarEmbedFailed),
                            None,
                        );
                    }
                }
            }
        }
        // A slow SQLite read must not suspend the native heartbeat or hide host/Explorer failure.
        if last_poll.elapsed() >= Duration::from_secs(5) {
            if let Some(host) = connection.as_mut() {
                match host.exchange(HostMessage::GetStatus {}).await {
                    Ok(HostReply::Status { status }) => {
                        if native_revision.as_ref() != Some(&status.system_revision)
                            || status.state == token_pulse_taskbar::HostDisplayState::Embedded
                        {
                            failures = 0;
                            render_backoff = false;
                            retry_at = Instant::now();
                            next_read = Instant::now();
                        }
                        native_revision = Some(status.system_revision.clone());
                        publish_host_status(&snapshot, &changed, &visible, status)
                    }
                    _ => {
                        close_host(&mut connection, &snapshot, &changed).await;
                        policy = None;
                        configuration = None;
                        visible(false);
                        failures = failures.saturating_add(1);
                        retry_at = Instant::now() + retry_delay(failures);
                        publish(
                            &snapshot,
                            &changed,
                            TaskbarRuntimeState::Recovering,
                            None,
                            Some(TaskbarRuntimeIssue::HostTimeout),
                            Some(ErrorCode::TaskbarEmbedFailed),
                            None,
                        );
                    }
                }
            }
            last_poll = Instant::now();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    visible(false);
    if let Some((request, _)) = fallback_work.take() {
        request.cancel();
    }
    close_host(&mut connection, &snapshot, &changed).await;
    let uncertain = snapshot.lock().ok().is_none_or(|s| {
        matches!(
            s.last_cleanup,
            Some(
                TaskbarCleanupOutcome::Failed
                    | TaskbarCleanupOutcome::Uncertain
                    | TaskbarCleanupOutcome::Timeout
                    | TaskbarCleanupOutcome::Unavailable
            )
        )
    });
    publish(
        &snapshot,
        &changed,
        if uncertain {
            TaskbarRuntimeState::Unavailable
        } else {
            TaskbarRuntimeState::Disabled
        },
        None,
        uncertain.then_some(TaskbarRuntimeIssue::CleanupUncertain),
        uncertain.then_some(ErrorCode::TaskbarEmbedFailed),
        None,
    );
}
#[cfg(windows)]
fn publish_fallback(
    snapshot: &Mutex<TaskbarRuntimeSnapshot>,
    changed: &Changed,
    result: Result<bool, ErrorCode>,
) {
    let (visible, error) = match result {
        Ok(visible) => (Some(visible), None),
        Err(error) => (None, Some(error)),
    };
    let update = snapshot.lock().ok().and_then(|mut current| {
        if current.fallback_visible == visible && current.fallback_error == error {
            return None;
        }
        current.fallback_visible = visible;
        current.fallback_error = error;
        current.revision = DecimalInt::from_nonnegative(current.revision.value().saturating_add(1))
            .expect("bounded service revision");
        Some(current.clone())
    });
    if let Some(update) = update {
        changed(update);
    }
}
fn retry_delay(failures: usize) -> Duration {
    Duration::from_millis([500, 1000, 2000, 5000, 10000][failures.saturating_sub(1).min(4)])
}
#[cfg(windows)]
fn publish_host_status(
    snapshot: &Mutex<TaskbarRuntimeSnapshot>,
    changed: &Changed,
    visible: &Visible,
    status: token_pulse_taskbar::HostStatus,
) {
    use token_pulse_taskbar::{HostDisplayState, HostFailure};
    let (state, issue, error) = match status.state {
        HostDisplayState::Disabled => (TaskbarRuntimeState::Disabled, None, None),
        HostDisplayState::WaitingSnapshot => (TaskbarRuntimeState::WaitingSnapshot, None, None),
        HostDisplayState::Embedded => (TaskbarRuntimeState::Embedded, None, None),
        HostDisplayState::Unavailable => {
            let issue = match status.failure {
                Some(HostFailure::UnsupportedVersion) => TaskbarRuntimeIssue::UnsupportedVersion,
                Some(HostFailure::MissingTaskbar) => TaskbarRuntimeIssue::MissingTaskbar,
                Some(HostFailure::UnexpectedStructure) => TaskbarRuntimeIssue::UnexpectedStructure,
                Some(HostFailure::UnsafeGeometry) => TaskbarRuntimeIssue::UnsafeGeometry,
                Some(HostFailure::InsufficientSpace) => TaskbarRuntimeIssue::InsufficientSpace,
                Some(HostFailure::BackgroundUnavailable) => {
                    TaskbarRuntimeIssue::BackgroundUnavailable
                }
                _ => TaskbarRuntimeIssue::HostUnavailable,
            };
            (
                TaskbarRuntimeState::Unavailable,
                Some(issue),
                Some(if issue == TaskbarRuntimeIssue::InsufficientSpace {
                    ErrorCode::TaskbarNoSpace
                } else {
                    ErrorCode::TaskbarEmbedFailed
                }),
            )
        }
    };
    visible(state == TaskbarRuntimeState::Embedded);
    publish(
        snapshot,
        changed,
        state,
        status.settings_revision,
        issue,
        error,
        status
            .density
            .map(|d| d != token_pulse_taskbar::display::Density::Full),
    );
}
#[cfg(windows)]
async fn close_host(
    connection: &mut Option<token_pulse_taskbar::windows::transport::HostConnection>,
    snapshot: &Mutex<TaskbarRuntimeSnapshot>,
    changed: &Changed,
) {
    use token_pulse_taskbar::windows::{RestoreDisposition, topology::ProbeError};
    let Some(mut host) = connection.take() else {
        return;
    };
    let _ = host.shutdown().await;
    let outcome = match host.last_cleanup() {
        Some(Ok(RestoreDisposition::NoRecord)) => TaskbarCleanupOutcome::NoRecord,
        Some(Ok(RestoreDisposition::Restored)) => TaskbarCleanupOutcome::Restored,
        Some(Ok(RestoreDisposition::AlreadyRestored)) => TaskbarCleanupOutcome::AlreadyRestored,
        Some(Ok(RestoreDisposition::ExternalChange)) => TaskbarCleanupOutcome::ExternalChange,
        Some(Ok(RestoreDisposition::IdentityLost)) => TaskbarCleanupOutcome::IdentityLost,
        Some(Ok(RestoreDisposition::Failed)) => TaskbarCleanupOutcome::Failed,
        Some(Ok(RestoreDisposition::Uncertain)) => TaskbarCleanupOutcome::Uncertain,
        Some(Err(ProbeError::CleanupTimeout)) => TaskbarCleanupOutcome::Timeout,
        _ => TaskbarCleanupOutcome::Unavailable,
    };
    let update = snapshot.lock().ok().and_then(|mut current| {
        if current.last_cleanup == Some(outcome) {
            return None;
        }
        current.last_cleanup = Some(outcome);
        current.revision =
            DecimalInt::from_nonnegative(current.revision.value().saturating_add(1)).ok()?;
        Some(current.clone())
    });
    if let Some(update) = update {
        changed(update);
    }
}
#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::time::Instant;
    fn state(state: TaskbarRuntimeState) -> TaskbarRuntimeSnapshot {
        TaskbarRuntimeSnapshot {
            revision: DecimalInt::parse("1").unwrap(),
            state,
            applied_settings_revision: None,
            issue: None,
            error: None,
            compact: None,
            fallback_visible: None,
            fallback_error: None,
            last_cleanup: None,
            last_snapshot_at_ms: None,
        }
    }
    #[test]
    fn fallback_policy_observes_without_reopening_hidden_window_and_recovery_renews_episode() {
        let mut policy = FallbackPolicy::default();
        let failure = state(TaskbarRuntimeState::Unavailable);
        assert_eq!(policy.action(true, &failure), Some(true));
        policy.completed(true, &Ok(true));
        assert_eq!(policy.action(true, &failure), Some(false));
        policy.completed(false, &Ok(false));
        assert_eq!(policy.action(true, &failure), None);
        assert_eq!(
            policy.action(true, &state(TaskbarRuntimeState::Recovering)),
            None
        );
        assert_eq!(
            policy.action(true, &state(TaskbarRuntimeState::Embedded)),
            None
        );
        assert_eq!(policy.action(true, &failure), Some(true));
        policy.completed(true, &Err(ErrorCode::WindowUnavailable));
        assert_eq!(policy.action(true, &failure), None);
        assert_eq!(policy.action(false, &failure), None);
        assert_eq!(policy.action(true, &failure), Some(true));
        let data = Mutex::new(failure);
        let notify: Changed = Arc::new(|_| {});
        publish_fallback(&data, &notify, Err(ErrorCode::WindowUnavailable));
        let result = data.lock().unwrap();
        assert_eq!(result.state, TaskbarRuntimeState::Unavailable);
        assert_eq!(result.fallback_visible, None);
        assert_eq!(result.fallback_error, Some(ErrorCode::WindowUnavailable));
    }
    #[test]
    fn fallback_guard_rejects_stale_generation_pause_suspend_exit_cancel_and_recovered_state() {
        let flags = Arc::new(Flags {
            stopping: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            suspended: AtomicBool::new(false),
            generation: AtomicU64::new(4),
        });
        let data = Arc::new(Mutex::new(state(TaskbarRuntimeState::Unavailable)));
        let request = FallbackRequest {
            show: true,
            flags: flags.clone(),
            generation: 4,
            snapshot: data.clone(),
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        assert!(request.current());
        for flag in [&flags.paused, &flags.suspended, &flags.stopping] {
            flag.store(true, Ordering::Release);
            assert!(!request.current());
            flag.store(false, Ordering::Release);
        }
        flags.generation.store(5, Ordering::Release);
        assert!(!request.valid());
        flags.generation.store(4, Ordering::Release);
        data.lock().unwrap().state = TaskbarRuntimeState::Embedded;
        assert!(request.valid());
        assert!(!request.current());
        request.cancel();
        assert!(!request.valid());
    }
    #[test]
    fn blocked_fallback_does_not_hold_privacy_barrier_or_publish_success_after_disable() {
        let enabled = Arc::new(AtomicBool::new(true));
        let read_enabled = enabled.clone();
        let (entered, notified) = mpsc::sync_channel(1);
        let (release, gate) = mpsc::sync_channel(1);
        let gate = Mutex::new(gate);
        let service = TaskbarService::start(
            std::path::PathBuf::from("unused-no-host.exe"),
            Arc::new(move || {
                let mut input = disabled("1");
                input.configuration.preferences.enabled = read_enabled.load(Ordering::Acquire);
                input.configuration.preferences.position = TaskbarPosition::ApplicationRight;
                Ok(input)
            }),
            Arc::new(|_| {}),
            Arc::new(|_| {}),
            Arc::new(move |request| {
                entered.send(request.clone()).unwrap();
                let _ = gate.lock().unwrap().recv();
                Ok(true)
            }),
        )
        .unwrap();
        let old = notified.recv_timeout(Duration::from_secs(3)).unwrap();
        let pause = service.pause_publication().unwrap();
        assert!(!old.current());
        enabled.store(false, Ordering::Release);
        drop(pause);
        wait(&service, |s| {
            s.state == TaskbarRuntimeState::Disabled && s.fallback_visible == Some(false)
        });
        release.send(()).unwrap();
        thread::sleep(Duration::from_millis(100));
        assert_eq!(service.snapshot().unwrap().fallback_visible, Some(false));
        service.shutdown();
    }
    #[test]
    fn fallback_is_attempted_once_then_reports_user_hide_without_reopening() {
        let shown = Arc::new(AtomicBool::new(false));
        let native_visible = shown.clone();
        let attempts = Arc::new(AtomicU64::new(0));
        let observed_attempts = attempts.clone();
        let service = TaskbarService::start(
            std::path::PathBuf::from("unused-no-host.exe"),
            Arc::new(|| {
                let mut input = disabled("1");
                input.configuration.preferences.enabled = true;
                input.configuration.preferences.position = TaskbarPosition::ApplicationRight;
                Ok(input)
            }),
            Arc::new(|_| {}),
            Arc::new(|_| {}),
            Arc::new(move |request| {
                if request.show {
                    observed_attempts.fetch_add(1, Ordering::AcqRel);
                    native_visible.store(true, Ordering::Release);
                }
                Ok(native_visible.load(Ordering::Acquire))
            }),
        )
        .unwrap();
        wait(&service, |s| s.fallback_visible == Some(true));
        shown.store(false, Ordering::Release);
        wait(&service, |s| s.fallback_visible == Some(false));
        thread::sleep(Duration::from_millis(1200));
        assert_eq!(attempts.load(Ordering::Acquire), 1);
        service.shutdown();
    }
    #[test]
    fn failed_fallback_with_cancelled_ui_dispatch_exposes_unknown_and_does_not_loop() {
        let attempts = Arc::new(AtomicU64::new(0));
        let calls = attempts.clone();
        let service = TaskbarService::start(
            std::path::PathBuf::from("unused-no-host.exe"),
            Arc::new(|| {
                let mut input = disabled("1");
                input.configuration.preferences.enabled = true;
                input.configuration.preferences.position = TaskbarPosition::ApplicationRight;
                Ok(input)
            }),
            Arc::new(|_| {}),
            Arc::new(|_| {}),
            Arc::new(move |request| {
                calls.fetch_add(1, Ordering::AcqRel);
                request.cancel();
                Err(ErrorCode::WindowUnavailable)
            }),
        )
        .unwrap();
        wait(&service, |s| {
            s.fallback_error == Some(ErrorCode::WindowUnavailable)
        });
        assert_eq!(service.snapshot().unwrap().fallback_visible, None);
        thread::sleep(Duration::from_millis(1200));
        assert_eq!(attempts.load(Ordering::Acquire), 1);
        service.shutdown();
    }
    fn disabled(revision: &str) -> Input {
        Input {
            configuration: TaskbarPreferencesSnapshot {
                preferences: Default::default(),
                settings_revision: DecimalInt::parse(revision).unwrap(),
            },
            privacy: true,
            view: None,
        }
    }
    fn wait(service: &TaskbarService, condition: impl Fn(&TaskbarRuntimeSnapshot) -> bool) {
        let until = Instant::now() + Duration::from_secs(3);
        while !condition(&service.snapshot().unwrap()) {
            assert!(Instant::now() < until, "functional service state deadline");
            thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    fn blocked_old_input_does_not_block_privacy_barrier_or_publish_after_resume() {
        let (entered, notified) = mpsc::sync_channel(1);
        let (release, gate) = mpsc::sync_channel(1);
        let gate = Mutex::new(gate);
        let reads = AtomicU64::new(0);
        let service = TaskbarService::start(
            std::path::PathBuf::from("unused-disabled-host.exe"),
            Arc::new(move || {
                if reads.fetch_add(1, Ordering::AcqRel) == 0 {
                    entered.send(()).unwrap();
                    let _ = gate.lock().unwrap().recv();
                    Ok(disabled("1"))
                } else {
                    Ok(disabled("2"))
                }
            }),
            Arc::new(|_| {}),
            Arc::new(|_| {}),
            Arc::new(|_| Ok(false)),
        )
        .unwrap();
        notified.recv_timeout(Duration::from_secs(3)).unwrap();
        let pause = service.pause_publication().unwrap();
        assert_eq!(
            service.snapshot().unwrap().state,
            TaskbarRuntimeState::Recovering
        );
        drop(pause);
        wait(&service, |s| {
            s.applied_settings_revision
                .as_ref()
                .is_some_and(|r| r.as_str() == "2")
        });
        release.send(()).unwrap();
        thread::sleep(Duration::from_millis(50));
        assert_eq!(
            service
                .snapshot()
                .unwrap()
                .applied_settings_revision
                .unwrap()
                .as_str(),
            "2"
        );
        assert!(service.begin_exit());
        assert!(!service.begin_exit());
        service.shutdown();
        assert!(service.exit_ready.load(Ordering::Acquire));
    }
    #[test]
    fn suspend_resume_and_retry_bounds_are_functional_and_do_not_start_disabled_host() {
        assert_eq!(retry_delay(1), Duration::from_millis(500));
        assert_eq!(retry_delay(5), Duration::from_secs(10));
        assert_eq!(retry_delay(usize::MAX), Duration::from_secs(10));
        let service = TaskbarService::start(
            std::path::PathBuf::from("unused-disabled-host.exe"),
            Arc::new(|| Ok(disabled("3"))),
            Arc::new(|_| {}),
            Arc::new(|_| {}),
            Arc::new(|_| Ok(false)),
        )
        .unwrap();
        wait(&service, |s| {
            s.applied_settings_revision
                .as_ref()
                .is_some_and(|r| r.as_str() == "3")
        });
        service.suspend();
        wait(&service, |s| s.state == TaskbarRuntimeState::Suspended);
        service.resume();
        wait(&service, |s| s.state == TaskbarRuntimeState::Disabled);
        assert!(service.snapshot().unwrap().error.is_none());
        service.shutdown();
    }
    #[test]
    fn failed_host_launch_stops_after_five_attempts_and_explicit_retry_renews_budget() {
        let probes = Arc::new(AtomicU64::new(0));
        let observed = probes.clone();
        let service = TaskbarService::start(
            std::path::PathBuf::from("unused-invalid-host.exe"),
            Arc::new(|| {
                let mut input = disabled("1");
                input.configuration.preferences.enabled = true;
                Ok(input)
            }),
            Arc::new(move |s| {
                if s.state == TaskbarRuntimeState::Probing {
                    observed.fetch_add(1, Ordering::AcqRel);
                }
            }),
            Arc::new(|_| {}),
            Arc::new(|_| Ok(false)),
        )
        .unwrap();
        let until = Instant::now() + Duration::from_secs(15);
        while probes.load(Ordering::Acquire) < 5 {
            assert!(Instant::now() < until, "five failure attempts deadline");
            thread::sleep(Duration::from_millis(20));
        }
        wait(&service, |s| s.state == TaskbarRuntimeState::Unavailable);
        // Cross the would-be next 10-second retry deadline; there must be no sixth launch.
        thread::sleep(Duration::from_secs(11));
        assert_eq!(probes.load(Ordering::Acquire), 5);
        service.invalidate();
        let until = Instant::now() + Duration::from_secs(3);
        while probes.load(Ordering::Acquire) < 6 {
            assert!(Instant::now() < until, "explicit retry deadline");
            thread::sleep(Duration::from_millis(20));
        }
        service.shutdown();
    }
}
