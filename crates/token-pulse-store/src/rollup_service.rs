//! Serial derived-cache worker. It never owns source paths or parser state.
use crate::{Database, ErrorCode, StoreResult};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Default, Clone)]
pub struct RollupStatus {
    pub building_ledger: Option<String>,
    pub completed_builds: u64,
    pub last_error: Option<ErrorCode>,
}

pub struct RollupService {
    stop: Arc<AtomicBool>,
    wake: mpsc::SyncSender<()>,
    worker: Mutex<Option<JoinHandle<()>>>,
    status: Arc<Mutex<RollupStatus>>,
}

fn now_ms() -> StoreResult<i64> {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ErrorCode::InvalidQuery)?
        .as_millis();
    i64::try_from(ms).map_err(|_| ErrorCode::NumericOverflow.into())
}

impl RollupService {
    pub fn start(database: Database) -> StoreResult<Self> {
        Self::start_with_interval(database, Duration::from_secs(5))
    }
    pub fn start_with_interval(database: Database, interval: Duration) -> StoreResult<Self> {
        if interval < Duration::from_millis(50) || interval > Duration::from_secs(60) {
            return Err(ErrorCode::InvalidQuery.into());
        }
        now_ms()?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let status = Arc::new(Mutex::new(RollupStatus::default()));
        let worker_status = status.clone();
        let (wake, receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("tokenpulse-rollups".into())
            .spawn(move || {
                while !stopping.load(Ordering::Acquire) {
                    let mut did_work = false;
                    let result = (|| -> StoreResult<()> {
                        did_work = database.prune_rollups_step()? > 0;
                        if stopping.load(Ordering::Acquire) {
                            return Ok(());
                        }
                        if let Some(ledger) = database.next_rollup_ledger()? {
                            if let Ok(mut status) = worker_status.lock() {
                                status.building_ledger = Some(ledger.clone());
                            }
                            let result = database.build_hourly_rollup_interruptible(
                                &ledger,
                                now_ms()?,
                                &stopping,
                            );
                            if let Ok(mut status) = worker_status.lock() {
                                status.building_ledger = None;
                                if result.is_ok() {
                                    status.completed_builds =
                                        status.completed_builds.saturating_add(1);
                                }
                            }
                            result?;
                            did_work = true;
                        }
                        Ok(())
                    })();
                    if let Ok(mut status) = worker_status.lock() {
                        status.building_ledger = None;
                        if let Err(error) = result {
                            status.last_error = Some(error.code);
                        } else if did_work {
                            status.last_error = None;
                        }
                    }
                    // No unbounded queue, per-ledger map or all-ledger materialization.
                    // Leave writer/reader time between batches of historical caches.
                    let delay = if did_work {
                        interval.min(Duration::from_millis(250))
                    } else {
                        interval
                    };
                    let _ = receiver.recv_timeout(delay);
                }
            })
            .map_err(|_| ErrorCode::DbWriteFailed)?;
        Ok(Self {
            stop,
            wake,
            worker: Mutex::new(Some(worker)),
            status,
        })
    }
    pub fn wake(&self) {
        let _ = self.wake.try_send(());
    }
    pub fn status(&self) -> RollupStatus {
        self.status
            .lock()
            .map(|status| status.clone())
            .unwrap_or(RollupStatus {
                last_error: Some(ErrorCode::DbWriteFailed),
                ..Default::default()
            })
    }
    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::Release);
        self.wake();
        if let Ok(mut worker) = self.worker.lock() {
            if let Some(handle) = worker.take() {
                let _ = handle.join();
            }
        }
    }
}
impl Drop for RollupService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests;
