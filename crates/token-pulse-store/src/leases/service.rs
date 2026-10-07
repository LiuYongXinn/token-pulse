//! Two dedicated actors keep real SQLite transactions without self references.
use super::cursor::{CursorClaims, CursorSigner, QueryBinding};
use crate::{ErrorCode, Revision, StoreResult};
use rusqlite::{Connection, InterruptHandle, OpenFlags, Transaction, TransactionBehavior};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
const MAX_POSITIONS: usize = 4096;
#[derive(Clone)]
struct Position {
    revision: Revision,
    tuple: String,
}

#[derive(Clone, Copy, Debug)]
pub struct LeaseHandle {
    pub snapshot_id: [u8; 16],
    pub revision: Revision,
}
#[derive(Clone, Copy)]
struct Limits {
    total: Duration,
    idle: Duration,
    wal_bytes: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            total: Duration::from_secs(30),
            idle: Duration::from_secs(10),
            wal_bytes: 64 * 1024 * 1024,
        }
    }
}
#[derive(Clone)]
struct Reservation {
    id: [u8; 16],
    binding: QueryBinding,
    cancelled: Arc<AtomicBool>,
    positions: Arc<Mutex<BTreeMap<[u8; 32], Position>>>,
    summary: Arc<Mutex<Option<Arc<crate::query::summary_cache::ScopeSummary>>>>,
}
#[derive(Clone)]
struct Guard {
    deadline: Instant,
    wal: PathBuf,
    cancelled: Arc<AtomicBool>,
    stopping: Arc<AtomicBool>,
    wal_limit: u64,
}
impl Guard {
    fn expired(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
            || self.stopping.load(Ordering::Acquire)
            || Instant::now() >= self.deadline
            || std::fs::metadata(&self.wal).is_ok_and(|m| m.len() > self.wal_limit)
    }
}
type Task = Box<dyn FnOnce(Result<(&Transaction<'_>, Revision, &Guard), ErrorCode>) + Send>;
enum Message {
    Open {
        reservation: Reservation,
        reply: SyncSender<StoreResult<LeaseHandle>>,
    },
    Read {
        id: [u8; 16],
        binding: QueryBinding,
        task: Task,
    },
    Release {
        id: [u8; 16],
        reply: SyncSender<StoreResult<()>>,
    },
    Shutdown,
}
struct Worker {
    sender: SyncSender<Message>,
    state: Arc<Mutex<Option<Reservation>>>,
    stopping: Arc<AtomicBool>,
    interrupt: InterruptHandle,
    thread: Option<JoinHandle<()>>,
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        self.interrupt.interrupt();
        let _ = self.sender.try_send(Message::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
pub struct LeaseService {
    workers: Vec<Worker>,
    allocation: Mutex<()>,
    signer: CursorSigner,
    health: crate::integrity::Health,
}
impl LeaseService {
    pub(crate) fn new(path: &Path) -> StoreResult<Self> {
        Self::with_limits(path, Limits::default())
    }
    #[cfg(test)]
    pub(crate) fn for_testing(
        path: &Path,
        total: Duration,
        idle: Duration,
        wal_bytes: u64,
    ) -> StoreResult<Self> {
        Self::with_limits(
            path,
            Limits {
                total,
                idle,
                wal_bytes,
            },
        )
    }
    fn with_limits(path: &Path, limits: Limits) -> StoreResult<Self> {
        let mut service = Self {
            workers: Vec::with_capacity(2),
            allocation: Mutex::new(()),
            signer: CursorSigner::new()?,
            health: Default::default(),
        };
        for index in 0..2 {
            let (sender, receiver) = mpsc::sync_channel(8);
            let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
            let state = Arc::new(Mutex::new(None));
            let stopping = Arc::new(AtomicBool::new(false));
            let actor_state = state.clone();
            let actor_stop = stopping.clone();
            let path = path.to_owned();
            let thread = thread::Builder::new()
                .name(format!("token-pulse-lease-{index}"))
                .spawn(move || {
                    let opened = (|| -> StoreResult<Connection> {
                        let conn = Connection::open_with_flags(
                            &path,
                            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
                        )?;
                        crate::database::configure(&conn)?;
                        conn.pragma_update(None, "query_only", true)?;
                        Ok(conn)
                    })();
                    match opened {
                        Ok(mut conn) => {
                            if ready_sender.send(Ok(conn.get_interrupt_handle())).is_err() {
                                return;
                            }
                            actor(&mut conn, &path, receiver, actor_state, actor_stop, limits);
                        }
                        Err(error) => {
                            let _ = ready_sender.send(Err(error));
                        }
                    }
                })
                .map_err(|_| ErrorCode::DbWriteFailed)?;
            let interrupt = match ready_receiver
                .recv()
                .map_err(|_| ErrorCode::DbWriteFailed)?
            {
                Ok(handle) => handle,
                Err(error) => {
                    let _ = thread.join();
                    return Err(error);
                }
            };
            service.workers.push(Worker {
                sender,
                state,
                stopping,
                interrupt,
                thread: Some(thread),
            });
        }
        Ok(service)
    }
    pub fn signer(&self) -> &CursorSigner {
        &self.signer
    }
    pub(crate) fn set_health(&mut self, health: crate::integrity::Health) {
        self.health = health;
    }
    pub fn open(&self, binding: &QueryBinding) -> StoreResult<LeaseHandle> {
        self.health.check()?;
        let _allocation = self
            .allocation
            .lock()
            .map_err(|_| ErrorCode::DbWriteFailed)?;
        let id = self.signer.new_snapshot_id()?;
        for worker in &self.workers {
            let mut state = worker.state.lock().map_err(|_| ErrorCode::DbWriteFailed)?;
            if state.is_some() {
                continue;
            }
            let reservation = Reservation {
                id,
                binding: binding.clone(),
                cancelled: Arc::new(AtomicBool::new(false)),
                positions: Arc::new(Mutex::new(BTreeMap::new())),
                summary: Arc::new(Mutex::new(None)),
            };
            *state = Some(reservation.clone());
            drop(state);
            let (reply, receive) = mpsc::sync_channel(1);
            if worker
                .sender
                .try_send(Message::Open { reservation, reply })
                .is_err()
            {
                *worker.state.lock().map_err(|_| ErrorCode::DbWriteFailed)? = None;
                return Err(ErrorCode::SnapshotExpired.into());
            }
            return receive.recv().map_err(|_| ErrorCode::SnapshotExpired)?;
        }
        Err(ErrorCode::SnapshotExpired.into())
    }
    fn worker(&self, handle: &LeaseHandle, binding: &QueryBinding) -> StoreResult<&Worker> {
        for worker in &self.workers {
            let state = worker.state.lock().map_err(|_| ErrorCode::DbWriteFailed)?;
            if let Some(active) = state.as_ref().filter(|r| r.id == handle.snapshot_id) {
                if &active.binding != binding {
                    return Err(ErrorCode::CursorInvalid.into());
                }
                if active.cancelled.load(Ordering::Acquire) {
                    return Err(ErrorCode::SnapshotExpired.into());
                }
                return Ok(worker);
            }
        }
        Err(ErrorCode::SnapshotExpired.into())
    }
    pub fn read<T: Send + 'static>(
        &self,
        handle: &LeaseHandle,
        binding: &QueryBinding,
        query: impl FnOnce(&Transaction<'_>, Revision) -> StoreResult<T> + Send + 'static,
    ) -> StoreResult<T> {
        self.health.check()?;
        let health = self.health.clone();
        let worker = self.worker(handle, binding)?;
        let (reply, receive) = mpsc::sync_channel(1);
        let task = Box::new(
            move |context: Result<(&Transaction<'_>, Revision, &Guard), ErrorCode>| {
                let result =
                    match context {
                        Ok((tx, revision, guard)) => {
                            let result = health.check().and_then(|_| query(tx, revision)).and_then(
                                |value| {
                                    health.check()?;
                                    Ok(value)
                                },
                            );
                            if guard.expired() {
                                Err(ErrorCode::SnapshotExpired.into())
                            } else {
                                result
                            }
                        }
                        Err(code) => Err(code.into()),
                    };
                let _ = reply.send(result);
            },
        );
        worker
            .sender
            .try_send(Message::Read {
                id: handle.snapshot_id,
                binding: binding.clone(),
                task,
            })
            .map_err(|_| ErrorCode::SnapshotExpired)?;
        receive.recv().map_err(|_| ErrorCode::SnapshotExpired)?
    }
    pub(crate) fn read_summary<T: Send + 'static>(
        &self,
        handle: &LeaseHandle,
        binding: &QueryBinding,
        database: crate::Database,
        filter: token_pulse_core::protocol::UsageFilter,
        basis: token_pulse_core::protocol::PriceBasis,
        query: impl FnOnce(
            &Transaction<'_>,
            Revision,
            &crate::query::summary_cache::ScopeSummary,
        ) -> StoreResult<T>
        + Send
        + 'static,
    ) -> StoreResult<T> {
        let worker = self.worker(handle, binding)?;
        let summary = worker
            .state
            .lock()
            .map_err(|_| ErrorCode::DbWriteFailed)?
            .as_ref()
            .filter(|active| active.id == handle.snapshot_id)
            .ok_or(ErrorCode::SnapshotExpired)?
            .summary
            .clone();
        self.read(handle, binding, move |tx, revision| {
            let mut cached = summary.lock().map_err(|_| ErrorCode::DbWriteFailed)?;
            if cached.is_none() {
                *cached = Some(database.scope_summary(tx, revision, &filter, &basis)?);
            }
            query(tx, revision, cached.as_deref().ok_or(ErrorCode::DbCorrupt)?)
        })
    }
    /// Call after the page read, never reentrantly inside an actor callback.
    pub fn issue_cursor(
        &self,
        handle: &LeaseHandle,
        binding: &QueryBinding,
        tuple: &impl Serialize,
    ) -> StoreResult<String> {
        let claims = CursorClaims::new(handle.snapshot_id, tuple)?;
        let encoded = serde_json::to_string(tuple)?;
        let captured = self.read(handle, binding, |_, revision| Ok(revision))?;
        if captured != handle.revision {
            return Err(ErrorCode::CursorInvalid.into());
        }
        let worker = self.worker(handle, binding)?;
        let state = worker.state.lock().map_err(|_| ErrorCode::DbWriteFailed)?;
        let active = state
            .as_ref()
            .filter(|r| r.id == handle.snapshot_id)
            .ok_or(ErrorCode::SnapshotExpired)?;
        let mut positions = active
            .positions
            .lock()
            .map_err(|_| ErrorCode::DbWriteFailed)?;
        if positions.len() >= MAX_POSITIONS && !positions.contains_key(&claims.position_hash) {
            drop(positions);
            drop(state);
            let _ = self.release(handle, binding);
            return Err(ErrorCode::SnapshotExpired.into());
        }
        positions.entry(claims.position_hash).or_insert(Position {
            revision: captured,
            tuple: encoded,
        });
        Ok(self.signer.issue(binding, claims))
    }
    pub fn resolve_cursor<T: DeserializeOwned>(
        &self,
        token: &str,
        binding: &QueryBinding,
    ) -> StoreResult<(LeaseHandle, T)> {
        // Authenticate the complete token before even looking for a lease.
        let claims = self.signer.read(token, binding)?;
        for worker in &self.workers {
            let state = worker.state.lock().map_err(|_| ErrorCode::DbWriteFailed)?;
            let Some(active) = state.as_ref().filter(|r| r.id == claims.snapshot_id) else {
                continue;
            };
            if &active.binding != binding {
                return Err(ErrorCode::CursorInvalid.into());
            }
            let position = active
                .positions
                .lock()
                .map_err(|_| ErrorCode::DbWriteFailed)?
                .get(&claims.position_hash)
                .cloned()
                .ok_or(ErrorCode::CursorInvalid)?;
            drop(state);
            let handle = LeaseHandle {
                snapshot_id: claims.snapshot_id,
                revision: position.revision,
            };
            let captured = self.read(&handle, binding, |_, revision| Ok(revision))?;
            if captured != position.revision {
                return Err(ErrorCode::CursorInvalid.into());
            }
            let tuple =
                serde_json::from_str(&position.tuple).map_err(|_| ErrorCode::CursorInvalid)?;
            return Ok((handle, tuple));
        }
        Err(ErrorCode::SnapshotExpired.into())
    }
    pub fn release(&self, handle: &LeaseHandle, binding: &QueryBinding) -> StoreResult<()> {
        // Prevent a reclaimed slot from starting a new transaction between
        // validation and interruption of this one. Queue Release before Open.
        let allocation = self
            .allocation
            .lock()
            .map_err(|_| ErrorCode::DbWriteFailed)?;
        let worker = self.worker(handle, binding)?;
        {
            let state = worker.state.lock().map_err(|_| ErrorCode::DbWriteFailed)?;
            let active = state
                .as_ref()
                .filter(|r| r.id == handle.snapshot_id)
                .ok_or(ErrorCode::SnapshotExpired)?;
            active.cancelled.store(true, Ordering::Release);
        }
        worker.interrupt.interrupt();
        let (reply, receive) = mpsc::sync_channel(1);
        worker
            .sender
            .try_send(Message::Release {
                id: handle.snapshot_id,
                reply,
            })
            .map_err(|_| ErrorCode::SnapshotExpired)?;
        drop(allocation);
        receive.recv().map_err(|_| ErrorCode::SnapshotExpired)?
    }
}

fn clear(state: &Mutex<Option<Reservation>>, id: [u8; 16]) {
    if let Ok(mut state) = state.lock() {
        if state.as_ref().is_some_and(|r| r.id == id) {
            *state = None;
        }
    }
}
fn actor(
    conn: &mut Connection,
    path: &Path,
    receiver: mpsc::Receiver<Message>,
    state: Arc<Mutex<Option<Reservation>>>,
    stopping: Arc<AtomicBool>,
    limits: Limits,
) {
    while !stopping.load(Ordering::Acquire) {
        let Ok(message) = receiver.recv() else {
            break;
        };
        match message {
            Message::Open { reservation, reply } => {
                let id = reservation.id;
                let started = Instant::now();
                let mut wal = path.as_os_str().to_owned();
                wal.push("-wal");
                let guard = Guard {
                    deadline: started + limits.total,
                    wal: wal.into(),
                    cancelled: reservation.cancelled.clone(),
                    stopping: stopping.clone(),
                    wal_limit: limits.wal_bytes,
                };
                let progress_guard = guard.clone();
                let mut checked = Instant::now();
                let mut expired = false;
                let progress = conn.progress_handler(
                    1000,
                    Some(move || {
                        if checked.elapsed() >= Duration::from_millis(100) {
                            expired = progress_guard.expired();
                            checked = Instant::now();
                        }
                        expired
                            || progress_guard.cancelled.load(Ordering::Acquire)
                            || progress_guard.stopping.load(Ordering::Acquire)
                    }),
                );
                if let Err(error) = progress {
                    clear(&state, id);
                    let _ = reply.send(Err(error.into()));
                    continue;
                }
                let result = (|| -> StoreResult<()> {
                    let tx = conn.transaction_with_behavior(TransactionBehavior::Deferred)?;
                    let revision=tx.query_row("SELECT data_revision,price_revision,settings_revision FROM app_state WHERE singleton=1",[],|r|Ok(Revision{data:r.get(0)?,price:r.get(1)?,settings:r.get(2)?}))?;
                    if guard.expired() {
                        return Err(ErrorCode::SnapshotExpired.into());
                    }
                    let _ = reply.send(Ok(LeaseHandle {
                        snapshot_id: id,
                        revision,
                    }));
                    let mut last_active = Instant::now();
                    loop {
                        if guard.expired() || last_active.elapsed() >= limits.idle {
                            break;
                        }
                        let wait = Duration::from_millis(100)
                            .min(guard.deadline.saturating_duration_since(Instant::now()))
                            .min(limits.idle.saturating_sub(last_active.elapsed()));
                        let message = match receiver.recv_timeout(wait) {
                            Ok(message) => message,
                            Err(mpsc::RecvTimeoutError::Timeout) => continue,
                            Err(_) => break,
                        };
                        match message {
                            Message::Read {
                                id: request_id,
                                binding,
                                task,
                            } => {
                                if request_id != id {
                                    task(Err(ErrorCode::SnapshotExpired));
                                } else if binding != reservation.binding {
                                    task(Err(ErrorCode::CursorInvalid));
                                } else if guard.expired() {
                                    task(Err(ErrorCode::SnapshotExpired));
                                } else {
                                    task(Ok((&tx, revision, &guard)));
                                    last_active = Instant::now();
                                }
                            }
                            Message::Release {
                                id: release_id,
                                reply,
                            } => {
                                if release_id == id {
                                    tx.progress_handler(0, None::<fn() -> bool>)?;
                                    tx.rollback()?;
                                    clear(&state, id);
                                    let _ = reply.send(Ok(()));
                                    return Ok(());
                                }
                                let _ = reply.send(Err(ErrorCode::SnapshotExpired.into()));
                            }
                            Message::Open {
                                reservation: other,
                                reply,
                            } => {
                                clear(&state, other.id);
                                let _ = reply.send(Err(ErrorCode::SnapshotExpired.into()));
                            }
                            Message::Shutdown => break,
                        }
                    }
                    tx.progress_handler(0, None::<fn() -> bool>)?;
                    tx.rollback()?;
                    Ok(())
                })();
                if let Err(error) = result {
                    let _ = reply.send(Err(error));
                }
                if conn.progress_handler(0, None::<fn() -> bool>).is_err() {
                    clear(&state, id);
                    break;
                }
                // An interrupted BEGIN/read error may have reached Transaction's Drop.
                // Ensure a failed rollback never poisons the next lease on this actor.
                if !conn.is_autocommit() {
                    let _ = conn.execute_batch("ROLLBACK");
                }
                clear(&state, id);
            }
            Message::Read { task, .. } => task(Err(ErrorCode::SnapshotExpired)),
            Message::Release { reply, .. } => {
                let _ = reply.send(Ok(()));
            }
            Message::Shutdown => break,
        }
    }
}

#[cfg(test)]
mod tests;
