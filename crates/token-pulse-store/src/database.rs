use crate::{ErrorCode, StoreResult, aggregate, migration};
use rusqlite::{Connection, OpenFlags, Transaction, TransactionBehavior, params};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Revision {
    pub data: i64,
    pub price: i64,
    pub settings: i64,
}
#[derive(Debug, Clone)]
pub struct SourceRecord {
    pub source_id: String,
    pub root_path: String,
    pub directory_identity: Option<String>,
    pub kind: String,
    pub enabled: bool,
    pub created_at_ms: i64,
}
type Task = Box<dyn FnOnce(&mut Connection) + Send>;
type UsageListener = Arc<dyn Fn(token_pulse_core::query::UsageRevision) + Send + Sync>;
enum Message {
    Run(Task),
    Shutdown,
}
struct Inner {
    sender: SyncSender<Message>,
    thread: Option<JoinHandle<()>>,
    readers: ReaderPool,
    usage_readers: ReaderPool,
    interactive_readers: ReaderPool,
    light_readers: ReaderPool,
    path: PathBuf,
    leases: crate::leases::LeaseService,
    usage_listener: Mutex<Option<UsageListener>>,
    summaries: crate::query::summary_cache::SummaryCache,
}
impl Drop for Inner {
    fn drop(&mut self) {
        let _ = self.sender.send(Message::Shutdown);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}
struct ReaderPool {
    connections: Mutex<Vec<Connection>>,
    available: Condvar,
}
struct Reader<'a> {
    pool: &'a ReaderPool,
    connection: Option<Connection>,
}
impl Drop for Reader<'_> {
    fn drop(&mut self) {
        if let Some(conn) = self.connection.take() {
            if let Ok(mut pool) = self.pool.connections.lock() {
                pool.push(conn);
                self.pool.available.notify_one();
            }
        }
    }
}
impl ReaderPool {
    fn take(&self) -> StoreResult<Reader<'_>> {
        let mut connections = self
            .connections
            .lock()
            .map_err(|_| ErrorCode::DbWriteFailed)?;
        if connections.is_empty() {
            let (guard, wait) = self
                .available
                .wait_timeout_while(connections, Duration::from_secs(5), |c| c.is_empty())
                .map_err(|_| ErrorCode::DbWriteFailed)?;
            connections = guard;
            if wait.timed_out() && connections.is_empty() {
                return Err(ErrorCode::SnapshotExpired.into());
            }
        }
        Ok(Reader {
            pool: self,
            connection: connections.pop(),
        })
    }
}

#[derive(Clone)]
pub struct Database {
    inner: Arc<Inner>,
}
impl Database {
    /// Only the trusted runtime supplies this application-owned local directory.
    pub fn open(app_data_directory: &Path) -> StoreResult<Self> {
        Self::open_with_lease_service(
            app_data_directory,
            crate::leases::LeaseService::new,
            crate::maintenance::WAL_LIMIT,
        )
    }
    #[cfg(test)]
    pub(crate) fn open_testing_leases(
        path: &Path,
        total: Duration,
        idle: Duration,
        wal: u64,
    ) -> StoreResult<Self> {
        Self::open_with_lease_service(
            path,
            |path| crate::leases::LeaseService::for_testing(path, total, idle, wal),
            wal,
        )
    }
    fn open_with_lease_service(
        app_data_directory: &Path,
        lease_service: impl FnOnce(&Path) -> StoreResult<crate::leases::LeaseService>,
        wal_limit: u64,
    ) -> StoreResult<Self> {
        if !app_data_directory.is_absolute()
            || app_data_directory.to_string_lossy().starts_with("\\\\")
        {
            return Err(ErrorCode::InvalidQuery.into());
        }
        std::fs::create_dir_all(app_data_directory).map_err(|_| ErrorCode::DbWriteFailed)?;
        let path = app_data_directory.join("token-pulse.db");
        let writer_path = path.clone();
        let (sender, receiver) = mpsc::sync_channel::<Message>(64);
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let handle = thread::Builder::new()
            .name("token-pulse-writer".into())
            .spawn(move || {
                let opened = (|| -> StoreResult<Connection> {
                    let mut conn = Connection::open(&writer_path)?;
                    configure(&conn)?;
                    migration::migrate(&mut conn, &writer_path)?;
                    Ok(conn)
                })();
                match opened {
                    Ok(mut conn) => {
                        if ready_sender.send(Ok(())).is_err() {
                            return;
                        }
                        let mut maintained = Instant::now();
                        loop {
                            match receiver.recv_timeout(Duration::from_millis(500)) {
                                Ok(Message::Run(task)) => task(&mut conn),
                                Ok(Message::Shutdown)
                                | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                                Err(mpsc::RecvTimeoutError::Timeout) => {}
                            }
                            if maintained.elapsed() >= Duration::from_millis(500) {
                                crate::maintenance::recover_if_large(
                                    &conn,
                                    &writer_path,
                                    wal_limit,
                                );
                                maintained = Instant::now();
                            }
                        }
                        // All accepted batches have completed before shutdown reaches this point.
                        let _ = crate::maintenance::checkpoint(&conn, false);
                    }
                    Err(e) => {
                        let _ = ready_sender.send(Err(e));
                    }
                }
            })
            .map_err(|_| ErrorCode::DbWriteFailed)?;
        if let Err(e) = ready_receiver
            .recv()
            .map_err(|_| ErrorCode::DbWriteFailed)?
        {
            let _ = handle.join();
            return Err(e);
        }
        let mut readers = Vec::with_capacity(2);
        for _ in 0..2 {
            let conn = Connection::open_with_flags(
                &path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )?;
            configure(&conn)?;
            conn.pragma_update(None, "query_only", true)?;
            readers.push(conn);
        }
        let pool = |size: usize| -> StoreResult<ReaderPool> {
            let mut connections = Vec::with_capacity(size);
            for _ in 0..size {
                let conn = Connection::open_with_flags(
                    &path,
                    OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
                )?;
                configure(&conn)?;
                conn.pragma_update(None, "query_only", true)?;
                connections.push(conn);
            }
            Ok(ReaderPool {
                connections: Mutex::new(connections),
                available: Condvar::new(),
            })
        };
        Ok(Self {
            inner: Arc::new(Inner {
                sender,
                thread: Some(handle),
                readers: ReaderPool {
                    connections: Mutex::new(readers),
                    available: Condvar::new(),
                },
                usage_readers: pool(2)?,
                interactive_readers: pool(2)?,
                light_readers: pool(1)?,
                leases: lease_service(&path)?,
                usage_listener: Mutex::new(None),
                summaries: Default::default(),
                path,
            }),
        })
    }
    pub fn path(&self) -> &Path {
        &self.inner.path
    }
    pub(crate) fn summary_cache(&self) -> &crate::query::summary_cache::SummaryCache {
        &self.inner.summaries
    }
    pub fn leases(&self) -> &crate::leases::LeaseService {
        &self.inner.leases
    }
    pub fn on_usage_changed(&self, listener: UsageListener) {
        if let Ok(mut slot) = self.inner.usage_listener.lock() {
            *slot = Some(listener);
        }
    }
    pub fn usage_revision(&self) -> StoreResult<token_pulse_core::query::UsageRevision> {
        self.light_snapshot(|tx, _| read_usage_revision(tx))
    }
    pub(crate) fn write<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut Connection) -> StoreResult<T> + Send + 'static,
    ) -> StoreResult<T> {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.inner
            .sender
            .send(Message::Run(Box::new(move |conn| {
                let before = read_usage_revision(conn).ok();
                let result = operation(conn);
                let after = read_usage_revision(conn).ok();
                let changed = after.filter(|revision| before.as_ref() != Some(revision));
                let _ = sender.send((result, changed));
            })))
            .map_err(|_| ErrorCode::DbWriteFailed)?;
        let (result, changed) = receiver.recv().map_err(|_| ErrorCode::DbWriteFailed)?;
        // Called after the transaction and writer operation have returned, on the
        // caller thread. Never invoke UI callbacks while holding a connection.
        if let Some(revision) = changed {
            let listener = self
                .inner
                .usage_listener
                .lock()
                .ok()
                .and_then(|slot| slot.clone());
            if let Some(listener) = listener {
                listener(revision);
            }
        }
        result
    }
    /// Callback runs inside one read transaction; revision is read first to pin the snapshot.
    pub fn snapshot<T>(
        &self,
        query: impl FnOnce(&Transaction<'_>, Revision) -> StoreResult<T>,
    ) -> StoreResult<T> {
        self.snapshot_pool(&self.inner.readers, "reader_wait", query)
    }
    pub(crate) fn usage_snapshot<T>(
        &self,
        query: impl FnOnce(&Transaction<'_>, Revision) -> StoreResult<T>,
    ) -> StoreResult<T> {
        self.snapshot_pool(&self.inner.usage_readers, "usage_reader_wait", query)
    }
    pub(crate) fn interactive_snapshot<T>(
        &self,
        query: impl FnOnce(&Transaction<'_>, Revision) -> StoreResult<T>,
    ) -> StoreResult<T> {
        self.snapshot_pool(
            &self.inner.interactive_readers,
            "interactive_reader_wait",
            query,
        )
    }
    pub(crate) fn light_snapshot<T>(
        &self,
        query: impl FnOnce(&Transaction<'_>, Revision) -> StoreResult<T>,
    ) -> StoreResult<T> {
        self.snapshot_pool(&self.inner.light_readers, "light_reader_wait", query)
    }
    fn snapshot_pool<T>(
        &self,
        pool: &ReaderPool,
        wait_stage: &'static str,
        query: impl FnOnce(&Transaction<'_>, Revision) -> StoreResult<T>,
    ) -> StoreResult<T> {
        let waited = Instant::now();
        let mut reader = pool.take()?;
        crate::query_timing::record(wait_stage, waited);
        let transaction = reader
            .connection
            .as_mut()
            .ok_or(ErrorCode::DbWriteFailed)?
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        let revision = transaction.query_row("SELECT data_revision,price_revision,settings_revision FROM app_state WHERE singleton=1", [], |r| Ok(Revision { data:r.get(0)?, price:r.get(1)?, settings:r.get(2)? }))?;
        let started = Instant::now();
        let result = query(&transaction, revision)?;
        crate::query_timing::record("snapshot_compute", started);
        transaction.commit()?;
        Ok(result)
    }
    pub fn add_source(&self, source: SourceRecord) -> StoreResult<()> {
        if source.source_id.is_empty()
            || !matches!(source.kind.as_str(), "local" | "wsl" | "mirror")
        {
            return Err(ErrorCode::InvalidQuery.into());
        }
        self.write(move |conn| {
            let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute("INSERT INTO sources(source_id,provider,root_path,directory_identity,kind,enabled,readability,capabilities_json,created_at_ms) VALUES(?1,'codex',?2,?3,?4,?5,'awaiting_directory','{}',?6)", params![source.source_id,source.root_path,source.directory_identity,source.kind,source.enabled,source.created_at_ms])?;
            transaction.execute("UPDATE app_state SET settings_revision=settings_revision+1 WHERE singleton=1", [])?;
            transaction.commit()?;
            Ok(())
        })
    }
}
pub(crate) fn read_usage_revision(
    conn: &Connection,
) -> StoreResult<token_pulse_core::query::UsageRevision> {
    let (database_id, data, price, view): (String, i64, i64, i64) = conn.query_row("SELECT database_instance_id,data_revision,price_revision,usage_view_revision FROM app_state WHERE singleton=1", [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))?;
    Ok(token_pulse_core::query::UsageRevision {
        database_id,
        data_revision: token_pulse_core::numeric::DecimalInt::from_nonnegative(data.into())?,
        price_revision: token_pulse_core::numeric::DecimalInt::from_nonnegative(price.into())?,
        usage_view_revision: token_pulse_core::numeric::DecimalInt::from_nonnegative(view.into())?,
    })
}
pub(crate) fn configure(conn: &Connection) -> StoreResult<()> {
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.pragma_update(None, "foreign_keys", true)?;
    aggregate::register(conn)?;
    Ok(())
}
