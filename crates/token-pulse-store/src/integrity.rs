//! Current-schema foreground validation and cancellable whole-file verification.
use crate::{ErrorCode, StoreResult};
use rusqlite::{Connection, InterruptHandle, OpenFlags, TransactionBehavior};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::Instant,
};

#[derive(Clone, Default)]
pub(crate) struct Health(Arc<AtomicBool>);
impl Health {
    pub(crate) fn check(&self) -> StoreResult<()> {
        if self.0.load(Ordering::Acquire) {
            Err(ErrorCode::DbCorrupt.into())
        } else {
            Ok(())
        }
    }
}

pub(crate) fn critical_check(connection: &mut Connection) -> StoreResult<()> {
    let started = Instant::now();
    let tx = connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
    let names = tx
        .prepare("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for name in names {
        // Only these two disposable detail tables dominate the historical cache size.
        // Headers, facts, settings/privacy, prices and display recovery stay foreground.
        // Cache reads continue to validate each used row and its authoritative fingerprint.
        if matches!(name.as_str(), "event_valuations" | "valuation_cache_inputs") {
            continue;
        }
        let sql = format!("PRAGMA quick_check('{}')", name.replace('\'', "''"));
        let table_started = Instant::now();
        let result: String = tx.query_row(&sql, [], |r| r.get(0))?;
        crate::query_timing::record(
            match name.as_str() {
                "observations" => "startup_observations",
                "pending_usage" => "startup_pending",
                "diagnostics" => "startup_diagnostics",
                "file_candidate_observations" => "startup_candidates",
                "usage_events" => "startup_events",
                _ => "startup_other_tables",
            },
            table_started,
        );
        if result != "ok" {
            return Err(ErrorCode::DbCorrupt.into());
        }
    }
    tx.commit()?;
    crate::query_timing::record("startup_critical_check", started);
    Ok(())
}

pub(crate) struct Verification {
    stop: Arc<AtomicBool>,
    interrupt: InterruptHandle,
    thread: Option<JoinHandle<()>>,
}
impl Verification {
    pub(crate) fn start(path: &Path, health: Health) -> StoreResult<Self> {
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        crate::database::configure(&connection)?;
        // SQLite omits CHECK-constraint validation on a file opened read-only.
        // Open the application database normally but forbid SQL writes on this worker.
        connection.pragma_update(None, "query_only", true)?;
        let interrupt = connection.get_interrupt_handle();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let progress_stop = stop.clone();
        connection.progress_handler(1000, Some(move || progress_stop.load(Ordering::Acquire)))?;
        let thread = std::thread::Builder::new()
            .name("tokenpulse-integrity".into())
            .spawn(move || {
                if stopping.load(Ordering::Acquire) {
                    return;
                }
                let started = Instant::now();
                let result =
                    connection.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0));
                if !stopping.load(Ordering::Acquire)
                    && !matches!(result, Ok(ref value) if value == "ok")
                {
                    health.0.store(true, Ordering::Release);
                }
                crate::query_timing::record("background_integrity_check", started);
            })
            .map_err(|_| ErrorCode::DbWriteFailed)?;
        Ok(Self {
            stop,
            interrupt,
            thread: Some(thread),
        })
    }
}
impl Drop for Verification {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.interrupt.interrupt();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests;
