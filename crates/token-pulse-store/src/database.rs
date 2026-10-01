use crate::{ErrorCode, StoreResult, aggregate, migration};
use rusqlite::{Connection, OpenFlags, Transaction, TransactionBehavior, params};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
    time::Duration,
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
enum Message {
    Run(Task),
    Shutdown,
}
struct Inner {
    sender: SyncSender<Message>,
    thread: Option<JoinHandle<()>>,
    readers: ReaderPool,
    path: PathBuf,
    leases: crate::leases::LeaseService,
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
        Self::open_with_lease_service(app_data_directory, crate::leases::LeaseService::new)
    }
    #[cfg(test)]
    pub(crate) fn open_testing_leases(
        path: &Path,
        total: Duration,
        idle: Duration,
        wal: u64,
    ) -> StoreResult<Self> {
        Self::open_with_lease_service(path, |path| {
            crate::leases::LeaseService::for_testing(path, total, idle, wal)
        })
    }
    fn open_with_lease_service(
        app_data_directory: &Path,
        lease_service: impl FnOnce(&Path) -> StoreResult<crate::leases::LeaseService>,
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
                        while let Ok(message) = receiver.recv() {
                            match message {
                                Message::Run(task) => task(&mut conn),
                                Message::Shutdown => break,
                            }
                        }
                        // All accepted batches have completed before shutdown reaches this point.
                        let _ = conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE)");
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
        Ok(Self {
            inner: Arc::new(Inner {
                sender,
                thread: Some(handle),
                readers: ReaderPool {
                    connections: Mutex::new(readers),
                    available: Condvar::new(),
                },
                leases: lease_service(&path)?,
                path,
            }),
        })
    }
    pub fn path(&self) -> &Path {
        &self.inner.path
    }
    pub fn leases(&self) -> &crate::leases::LeaseService {
        &self.inner.leases
    }
    pub(crate) fn write<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut Connection) -> StoreResult<T> + Send + 'static,
    ) -> StoreResult<T> {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.inner
            .sender
            .send(Message::Run(Box::new(move |conn| {
                let _ = sender.send(operation(conn));
            })))
            .map_err(|_| ErrorCode::DbWriteFailed)?;
        receiver.recv().map_err(|_| ErrorCode::DbWriteFailed)?
    }
    /// Callback runs inside one read transaction; revision is read first to pin the snapshot.
    pub fn snapshot<T>(
        &self,
        query: impl FnOnce(&Transaction<'_>, Revision) -> StoreResult<T>,
    ) -> StoreResult<T> {
        let mut reader = self.inner.readers.take()?;
        let transaction = reader
            .connection
            .as_mut()
            .ok_or(ErrorCode::DbWriteFailed)?
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        let revision = transaction.query_row("SELECT data_revision,price_revision,settings_revision FROM app_state WHERE singleton=1", [], |r| Ok(Revision { data:r.get(0)?, price:r.get(1)?, settings:r.get(2)? }))?;
        let result = query(&transaction, revision)?;
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
pub(crate) fn configure(conn: &Connection) -> StoreResult<()> {
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.pragma_update(None, "foreign_keys", true)?;
    aggregate::register(conn)?;
    Ok(())
}
