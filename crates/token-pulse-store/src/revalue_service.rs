//! Price-only worker; collection, parsing, quotas and source files are outside its authority.
use crate::{Database, ErrorCode, StoreResult, revalue_jobs::StoredRevalue};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use token_pulse_core::{
    jobs::CancelJobResult,
    pricing::revalue::{PriceRevalueJob, PriceRevalueRequest},
};

struct Running {
    id: String,
    cancel: Arc<AtomicBool>,
}
pub struct RevalueService {
    database: Database,
    stopping: Arc<AtomicBool>,
    running: Arc<Mutex<Option<Running>>>,
    wake: mpsc::SyncSender<()>,
    worker: Mutex<Option<JoinHandle<()>>>,
    error: Arc<Mutex<Option<ErrorCode>>>,
    changed: Arc<dyn Fn() + Send + Sync>,
}
fn now_ms() -> StoreResult<i64> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ErrorCode::InvalidQuery)?
            .as_millis(),
    )
    .map_err(|_| ErrorCode::NumericOverflow.into())
}
fn run_job(
    database: &Database,
    stored: &StoredRevalue,
    cancel: &Arc<AtomicBool>,
    stopping: &AtomicBool,
    changed: &dyn Fn(),
) -> StoreResult<()> {
    let id = &stored.job.job_id;
    let price =
        i64::try_from(stored.job.price_revision.value()).map_err(|_| ErrorCode::NumericOverflow)?;
    for ledger in database.price_revalue_plan(id)? {
        if stopping.load(Ordering::Acquire) {
            return Err(ErrorCode::JobInterrupted.into());
        }
        if cancel.load(Ordering::Acquire) {
            return Err(ErrorCode::JobCancelled.into());
        }
        let mut progress_error = None;
        let result = database.build_event_valuation_at_revision_interruptible(
            &ledger.ledger_id,
            &stored.request.basis,
            price,
            now_ms()?,
            cancel,
            |done, total| {
                if progress_error.is_some() {
                    return;
                }
                let progress = now_ms().and_then(|at| {
                    database.progress_price_revalue(
                        id.clone(),
                        ledger.ledger_id.clone(),
                        done,
                        total,
                        false,
                        at,
                    )
                });
                match progress {
                    Ok(()) => changed(),
                    Err(error) => {
                        progress_error = Some(error);
                        cancel.store(true, Ordering::Release);
                    }
                }
            },
        );
        if stopping.load(Ordering::Acquire) {
            return Err(ErrorCode::JobInterrupted.into());
        }
        if let Some(error) = progress_error {
            return Err(error);
        }
        let result = result?;
        database.progress_price_revalue(
            id.clone(),
            ledger.ledger_id,
            result.event_count,
            result.event_count,
            true,
            now_ms()?,
        )?;
        changed();
    }
    if stopping.load(Ordering::Acquire) {
        return Err(ErrorCode::JobInterrupted.into());
    }
    if cancel.load(Ordering::Acquire) {
        return Err(ErrorCode::JobCancelled.into());
    }
    Ok(())
}
impl RevalueService {
    pub fn start(database: Database, changed: Arc<dyn Fn() + Send + Sync>) -> StoreResult<Self> {
        Self::start_with_interval(database, Duration::from_secs(5), changed)
    }
    pub fn start_with_interval(
        database: Database,
        interval: Duration,
        changed: Arc<dyn Fn() + Send + Sync>,
    ) -> StoreResult<Self> {
        if interval < Duration::from_millis(50) || interval > Duration::from_secs(60) {
            return Err(ErrorCode::InvalidQuery.into());
        }
        let at = now_ms()?;
        database.interrupt_price_revalue_jobs(at)?;
        database.interrupt_valuation_builds()?;
        let stopping = Arc::new(AtomicBool::new(false));
        let running = Arc::new(Mutex::new(None::<Running>));
        let error = Arc::new(Mutex::new(None));
        let (wake, receiver) = mpsc::sync_channel(1);
        let db = database.clone();
        let stop = stopping.clone();
        let active = running.clone();
        let last_error = error.clone();
        let notify = changed.clone();
        let worker = thread::Builder::new()
            .name("tokenpulse-prices".into())
            .spawn(move || {
                while !stop.load(Ordering::Acquire) {
                    let mut did_work = false;
                    let result = (|| -> StoreResult<()> {
                        if db.enqueue_automatic_price_revalue(now_ms()?)?.is_some() {
                            notify();
                        }
                        if stop.load(Ordering::Acquire) {
                            return Ok(());
                        }
                        if let Some(stored) = db.claim_price_revalue_job(now_ms()?)? {
                            did_work = true;
                            let cancel = Arc::new(AtomicBool::new(false));
                            *active.lock().map_err(|_| ErrorCode::DbWriteFailed)? = Some(Running {
                                id: stored.job.job_id.clone(),
                                cancel: cancel.clone(),
                            });
                            notify();
                            let result = run_job(&db, &stored, &cancel, &stop, &*notify);
                            let work_error = result.err().map(|e| e.code);
                            let finish = db.finish_price_revalue_job(
                                stored.job.job_id,
                                work_error,
                                now_ms()?,
                            );
                            *active.lock().map_err(|_| ErrorCode::DbWriteFailed)? = None;
                            finish?;
                            notify();
                        }
                        Ok(())
                    })();
                    if let Ok(mut error) = last_error.lock() {
                        *error = result.err().map(|e| e.code);
                    }
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
            database,
            stopping,
            running,
            wake,
            worker: Mutex::new(Some(worker)),
            error,
            changed,
        })
    }
    pub fn start_job(
        &self,
        id: String,
        request: PriceRevalueRequest,
    ) -> StoreResult<PriceRevalueJob> {
        if self.stopping.load(Ordering::Acquire) {
            return Err(ErrorCode::JobInterrupted.into());
        }
        let job = self
            .database
            .create_price_revalue_job(id, request, now_ms()?)?;
        (self.changed)();
        self.wake();
        Ok(job)
    }
    pub fn cancel_job(&self, id: String) -> StoreResult<CancelJobResult> {
        let result = self
            .database
            .cancel_price_revalue_job(id.clone(), now_ms()?)?;
        if let Some(active) = self
            .running
            .lock()
            .map_err(|_| ErrorCode::DbWriteFailed)?
            .as_ref()
        {
            if active.id == id {
                active.cancel.store(true, Ordering::Release);
            }
        }
        (self.changed)();
        self.wake();
        Ok(result)
    }
    pub fn wake(&self) {
        let _ = self.wake.try_send(());
    }
    pub fn last_error(&self) -> Option<ErrorCode> {
        self.error
            .lock()
            .map(|e| *e)
            .unwrap_or(Some(ErrorCode::DbWriteFailed))
    }
    pub fn shutdown(&self) {
        self.stopping.store(true, Ordering::Release);
        if let Ok(active) = self.running.lock() {
            if let Some(active) = active.as_ref() {
                active.cancel.store(true, Ordering::Release);
            }
        }
        self.wake();
        if let Ok(mut worker) = self.worker.lock() {
            if let Some(handle) = worker.take() {
                let _ = handle.join();
            }
        }
    }
}
impl Drop for RevalueService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests;
