use crate::{collect_file, id};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use token_pulse_core::{
    error::ErrorCode,
    scheduling::{WorkPriority, WorkQueue},
    sources::*,
};
use token_pulse_store::{Database, StoreResult};

pub struct CollectorOptions {
    pub watcher: bool,
    pub active_poll: Duration,
    pub manifest_poll: Duration,
}
impl Default for CollectorOptions {
    fn default() -> Self {
        Self {
            watcher: true,
            active_poll: Duration::from_secs(60),
            manifest_poll: Duration::from_secs(600),
        }
    }
}
#[derive(Debug, Clone, Default)]
pub struct CollectorStatus {
    pub enabled_sources: usize,
    pub queue_length: usize,
    pub suspended: bool,
    pub commits: u64,
    pub last_commit_at_ms: Option<i64>,
    pub error: Option<ErrorCode>,
}
enum Command {
    Hint(String, PathBuf),
}
pub struct CollectorService {
    status: Arc<Mutex<CollectorStatus>>,
    thread: Mutex<Option<JoinHandle<()>>>,
    stopping: Arc<AtomicBool>,
    desired_suspend: Arc<AtomicBool>,
    requested_reconcile: Arc<AtomicBool>,
}
impl Drop for CollectorService {
    fn drop(&mut self) {
        self.shutdown();
    }
}
impl CollectorService {
    pub fn start(database: Database, options: CollectorOptions) -> StoreResult<Self> {
        if options.active_poll.is_zero() || options.manifest_poll.is_zero() {
            return Err(ErrorCode::InvalidQuery.into());
        }
        let (sender, receiver) = mpsc::sync_channel(1024);
        let status = Arc::new(Mutex::new(CollectorStatus::default()));
        let thread_status = status.clone();
        let hints = sender.clone();
        let stopping = Arc::new(AtomicBool::new(false));
        let desired_suspend = Arc::new(AtomicBool::new(false));
        let requested_reconcile = Arc::new(AtomicBool::new(false));
        let stop = stopping.clone();
        let desired = desired_suspend.clone();
        let requested = requested_reconcile.clone();
        let thread = thread::Builder::new()
            .name("tokenpulse-collector".into())
            .spawn(move || {
                let overflow = Arc::new(AtomicBool::new(false));
                let mut queue = WorkQueue::default();
                let mut known: BTreeMap<String, KnownFile> = BTreeMap::new();
                let mut scans: VecDeque<(String, SourceScanner)> = VecDeque::new();
                let mut watchers = Vec::<RecommendedWatcher>::new();
                let mut abilities: BTreeMap<String, SourceCapabilities> = BTreeMap::new();
                let mut scan_health: BTreeMap<String, SourceReadability> = BTreeMap::new();
                let mut manifest_at = Instant::now();
                let mut active_at = Instant::now();
                let mut reconcile = true;
                let mut suspended = false;
                let mut retries: BTreeMap<String, (usize, Instant)> = BTreeMap::new();
                loop {
                    if stop.load(Ordering::Acquire) {
                        return;
                    }
                    if desired.load(Ordering::Acquire) != suspended {
                        suspended = desired.load(Ordering::Acquire);
                        watchers.clear();
                        if !suspended {
                            scans.clear();
                            reconcile = true;
                        }
                    }
                    if requested.swap(false, Ordering::AcqRel) {
                        scans.clear();
                        reconcile = true;
                    }
                    let mut did_work = false;
                    for _ in 0..1024 {
                        let Ok(command) = receiver.try_recv() else {
                            break;
                        };
                        match command {
                            Command::Hint(source, path) => {
                                let key = file_key(&source, &path);
                                retries.remove(&key);
                                known.entry(key.clone()).or_insert(KnownFile {
                                    source,
                                    path,
                                    stamp: None,
                                });
                                queue.enqueue(
                                    key,
                                    WorkPriority::Live,
                                    Instant::now(),
                                    Duration::from_millis(300),
                                );
                            }
                        }
                    }
                    let now = Instant::now();
                    reconcile |=
                        overflow.swap(false, Ordering::Relaxed) || queue.take_reconcile_required();
                    if !suspended
                        && (reconcile || now.duration_since(manifest_at) >= options.manifest_poll)
                        && scans.is_empty()
                    {
                        reconcile = false;
                        manifest_at = now;
                        scans.clear();
                        watchers.clear();
                        match database.enabled_sources() {
                            Ok(sources) => {
                                if let Ok(mut s) = thread_status.lock() {
                                    s.enabled_sources = sources.len();
                                }
                                let enabled: Vec<_> =
                                    sources.iter().map(|s| s.source_id.clone()).collect();
                                known.retain(|_, file| enabled.contains(&file.source));
                                for source in sources {
                                    let root = PathBuf::from(&source.root_path);
                                    let origin = if source.kind == "wsl" {
                                        SourceOrigin::Wsl
                                    } else {
                                        SourceOrigin::Custom
                                    };
                                    let capability =
                                        abilities.entry(source.source_id.clone()).or_default();
                                    if options.watcher {
                                        let sender = hints.clone();
                                        let full = overflow.clone();
                                        let source_id = source.source_id.clone();
                                        let watch_root = root.clone();
                                        let watcher = notify::recommended_watcher(
                                            move |event: notify::Result<notify::Event>| match event
                                            {
                                                Ok(event) => {
                                                    if event.need_rescan()
                                                        || event.paths.len() > 128
                                                    {
                                                        full.store(true, Ordering::Relaxed);
                                                        return;
                                                    }
                                                    for path in event.paths {
                                                        if ![
                                                            watch_root.join("sessions"),
                                                            watch_root.join("archived_sessions"),
                                                        ]
                                                        .iter()
                                                        .any(|tree| path.starts_with(tree))
                                                        {
                                                            continue;
                                                        }
                                                        if path.extension().is_some_and(|s| {
                                                            s.eq_ignore_ascii_case("jsonl")
                                                        }) && [
                                                            watch_root.join("sessions"),
                                                            watch_root.join("archived_sessions"),
                                                        ]
                                                        .iter()
                                                        .any(|tree| path.starts_with(tree))
                                                        {
                                                            if sender
                                                                .try_send(Command::Hint(
                                                                    source_id.clone(),
                                                                    path,
                                                                ))
                                                                .is_err()
                                                            {
                                                                full.store(true, Ordering::Relaxed);
                                                            }
                                                        } else if path.is_dir() {
                                                            full.store(true, Ordering::Relaxed);
                                                        }
                                                    }
                                                }
                                                Err(_) => full.store(true, Ordering::Relaxed),
                                            },
                                        );
                                        match watcher.and_then(|mut watcher| {
                                            watcher.watch(&root, RecursiveMode::Recursive)?;
                                            Ok(watcher)
                                        }) {
                                            Ok(watcher) => {
                                                capability.watcher = CapabilityState::Available;
                                                watchers.push(watcher);
                                            }
                                            Err(_) => {
                                                capability.watcher = CapabilityState::Unavailable
                                            }
                                        }
                                    } else {
                                        capability.watcher = CapabilityState::Unavailable;
                                    }
                                    if let Ok(scanner) = SourceScanner::new(&root, origin) {
                                        let mut readable = 0;
                                        let mut failures = 0;
                                        for tree in
                                            [root.join("sessions"), root.join("archived_sessions")]
                                        {
                                            match fs::read_dir(tree) {
                                                Ok(_) => readable += 1,
                                                Err(error)
                                                    if error.kind()
                                                        != std::io::ErrorKind::NotFound =>
                                                {
                                                    failures += 1
                                                }
                                                _ => {}
                                            }
                                        }
                                        let state = match (readable > 0, failures > 0) {
                                            (true, true) => SourceReadability::PartiallyReadable,
                                            (true, false) => SourceReadability::Readable,
                                            (false, true) => SourceReadability::Unreadable,
                                            _ => SourceReadability::AwaitingDirectory,
                                        };
                                        scan_health.insert(source.source_id.clone(), state);
                                        if let Err(error) = database.update_source_runtime(
                                            source.source_id.clone(),
                                            state,
                                            capability.clone(),
                                            Some(epoch_ms()),
                                            None,
                                        ) {
                                            set_error(&thread_status, error.code);
                                        }
                                        scans.push_back((source.source_id, scanner));
                                    }
                                }
                            }
                            Err(error) => set_error(&thread_status, error.code),
                        }
                    }
                    if !suspended {
                        // Feed a bounded slice per turn; import never occupies the entire live queue.
                        if queue.len() < 2048 {
                            if let Some((source, mut scanner)) = scans.pop_front() {
                                did_work = true;
                                let mut complete = false;
                                for _ in 0..128 {
                                    match scanner.next() {
                                        Some(Ok(file)) => {
                                            let key = file_key(&source, &file.path);
                                            known.entry(key.clone()).or_insert(KnownFile {
                                                source: source.clone(),
                                                path: file.path,
                                                stamp: None,
                                            });
                                            queue.enqueue(
                                                key,
                                                WorkPriority::Historical,
                                                now,
                                                Duration::ZERO,
                                            );
                                        }
                                        Some(Err(issue)) => {
                                            let health = scan_health
                                                .entry(source.clone())
                                                .or_insert(issue.readability);
                                            if issue.readability == SourceReadability::Unreadable {
                                                *health = if *health == SourceReadability::Readable
                                                {
                                                    SourceReadability::PartiallyReadable
                                                } else {
                                                    SourceReadability::Unreadable
                                                };
                                            }
                                            let capability =
                                                abilities.get(&source).cloned().unwrap_or_default();
                                            let _ = database.update_source_runtime(
                                                source.clone(),
                                                *health,
                                                capability,
                                                Some(epoch_ms()),
                                                None,
                                            );
                                        }
                                        None => {
                                            complete = true;
                                            break;
                                        }
                                    }
                                }
                                if !complete {
                                    scans.push_back((source, scanner));
                                }
                            }
                        }
                        if now.duration_since(active_at) >= options.active_poll {
                            active_at = now;
                            if abilities
                                .values()
                                .any(|a| a.watcher == CapabilityState::Unavailable)
                            {
                                reconcile = true;
                            }
                            for (key, file) in &known {
                                let current = stamp(&file.path);
                                if current.is_none()
                                    || current.is_some_and(|(_, time)| time.is_none())
                                    || current != file.stamp
                                {
                                    queue.enqueue(
                                        key.clone(),
                                        WorkPriority::Live,
                                        now,
                                        Duration::ZERO,
                                    );
                                }
                            }
                        }
                        for (key, (_, due)) in &retries {
                            if *due <= now {
                                queue.enqueue(key.clone(), WorkPriority::Live, now, Duration::ZERO);
                            }
                        }
                        if let Some(key) = queue.pop_ready(now) {
                            if retries.get(&key).is_some_and(|(_, due)| *due > now) {
                                thread::sleep(Duration::from_millis(25));
                                continue;
                            }
                            if let Some(file) = known.get_mut(&key) {
                                did_work = true;
                                let before = stamp(&file.path);
                                match collect_file(&database, &file.source, &file.path, epoch_ms())
                                {
                                    Ok(receipt) => {
                                        retries.remove(&key);
                                        file.stamp = before;
                                        let capability =
                                            abilities.entry(file.source.clone()).or_default();
                                        capability.byte_seek = CapabilityState::Available;
                                        capability.physical_identity = CapabilityState::Available;
                                        let _ = database.update_source_runtime(
                                            file.source.clone(),
                                            scan_health
                                                .get(&file.source)
                                                .copied()
                                                .unwrap_or(SourceReadability::Readable),
                                            capability.clone(),
                                            Some(epoch_ms()),
                                            Some(epoch_ms()),
                                        );
                                        if receipt.has_more {
                                            queue.enqueue(
                                                key,
                                                WorkPriority::Historical,
                                                now,
                                                Duration::ZERO,
                                            );
                                        }
                                        if let Ok(mut s) = thread_status.lock() {
                                            s.commits = s.commits.saturating_add(1);
                                            s.last_commit_at_ms = Some(epoch_ms());
                                            s.error = None;
                                        }
                                    }
                                    Err(error) => {
                                        let attempts = retries
                                            .get(&key)
                                            .map(|(attempts, _)| attempts + 1)
                                            .unwrap_or(0)
                                            .min(4);
                                        let delay = [1, 2, 5, 15, 60][attempts];
                                        let jitter = u16::from_str_radix(&key[key.len() - 4..], 16)
                                            .unwrap_or(0)
                                            % 251;
                                        retries.insert(
                                            key,
                                            (
                                                attempts,
                                                Instant::now()
                                                    + Duration::from_secs(delay)
                                                    + Duration::from_millis(u64::from(jitter)),
                                            ),
                                        );
                                        set_error(&thread_status, error.code);
                                        let capability = abilities
                                            .get(&file.source)
                                            .cloned()
                                            .unwrap_or_default();
                                        let _ = database.update_source_runtime(
                                            file.source.clone(),
                                            SourceReadability::Unreadable,
                                            capability,
                                            Some(epoch_ms()),
                                            None,
                                        );
                                    }
                                }
                            }
                        }
                    }
                    if let Ok(mut s) = thread_status.lock() {
                        s.queue_length = queue.len();
                        s.suspended = suspended;
                    }
                    // Sleep at most 25 ms; control messages are handled before every bounded batch.
                    if !did_work {
                        thread::sleep(Duration::from_millis(25));
                    } else {
                        thread::yield_now();
                    }
                }
            })
            .map_err(|_| ErrorCode::DbWriteFailed)?;
        Ok(Self {
            status,
            thread: Mutex::new(Some(thread)),
            stopping,
            desired_suspend,
            requested_reconcile,
        })
    }
    pub fn status(&self) -> CollectorStatus {
        self.status
            .lock()
            .map(|s| s.clone())
            .unwrap_or_else(|_| CollectorStatus {
                error: Some(ErrorCode::DbWriteFailed),
                ..Default::default()
            })
    }
    pub fn shutdown(&self) {
        if let Ok(mut thread) = self.thread.lock() {
            if let Some(thread) = thread.take() {
                self.stopping.store(true, Ordering::Release);
                let _ = thread.join();
            }
        }
    }
    pub fn reconcile(&self) {
        self.requested_reconcile.store(true, Ordering::Release);
    }
    pub fn suspend(&self) {
        self.desired_suspend.store(true, Ordering::Release);
    }
    pub fn resume(&self) {
        self.desired_suspend.store(false, Ordering::Release);
        self.reconcile();
    }
}
struct KnownFile {
    source: String,
    path: PathBuf,
    stamp: Option<(u64, Option<SystemTime>)>,
}
fn file_key(source: &str, path: &std::path::Path) -> String {
    id("work", &format!("{source}:{}", path.display()))
}
fn stamp(path: &std::path::Path) -> Option<(u64, Option<SystemTime>)> {
    fs::metadata(path)
        .ok()
        .map(|m| (m.len(), m.modified().ok()))
}
fn epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .expect("Windows clock within supported epoch")
}
fn set_error(status: &Mutex<CollectorStatus>, code: ErrorCode) {
    if let Ok(mut s) = status.lock() {
        s.error = Some(code);
    }
}
