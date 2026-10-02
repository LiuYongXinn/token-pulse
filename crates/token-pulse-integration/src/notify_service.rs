//! Owns notify listeners and consumes dirty bits into ordinary log reconciliation only.
use crate::{
    notify_channel::{NotifyCapability, windows::NotifyListener},
    notify_config::{owns_current_notify, windows::read_config},
    notify_registry::{NotifyRegistration, RegistryError, windows::NotifyRegistry},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotifyServiceError {
    Registry(RegistryError),
    ConfigUnreadable,
    WrongExecutable,
    OriginalChainUnavailable,
    ChannelUnavailable,
    WorkerUnavailable,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NotifyServiceStatus {
    pub ready: bool,
    /// Unknown until enumeration; counts native listeners, not accounts or statistics.
    pub listener_count: Option<u8>,
    pub last_error: Option<NotifyServiceError>,
}
pub struct NotifyService {
    stopping: Arc<AtomicBool>,
    reload: Arc<AtomicBool>,
    status: Arc<Mutex<NotifyServiceStatus>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}
impl NotifyService {
    /// `reconcile` must enqueue and return; it must not scan or write usage synchronously.
    pub fn start(
        registry: NotifyRegistry,
        executable: PathBuf,
        reconcile: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self, RegistryError> {
        let stopping = Arc::new(AtomicBool::new(false));
        let reload = Arc::new(AtomicBool::new(true));
        let status = Arc::new(Mutex::new(NotifyServiceStatus::default()));
        let worker_stop = stopping.clone();
        let worker_reload = reload.clone();
        let worker_status = status.clone();
        let worker = thread::Builder::new()
            .name("tokenpulse-notify-owner".into())
            .spawn(move || {
                let _guard = match registry.service_guard() {
                    Ok(guard) => guard,
                    Err(error) => {
                        publish(
                            &worker_status,
                            false,
                            None,
                            Some(NotifyServiceError::Registry(error)),
                        );
                        return;
                    }
                };
                if let Err(error) = registry.recover_claims() {
                    publish(
                        &worker_status,
                        false,
                        None,
                        Some(NotifyServiceError::Registry(error)),
                    );
                    return;
                }
                let online = Arc::new(AtomicBool::new(false));
                let owner = thread::current();
                let mut listeners = BTreeMap::<String, (NotifyCapability, NotifyListener)>::new();
                let mut last_refresh = Instant::now();
                let mut last_error = None;
                let mut known_ids = Vec::new();
                while !worker_stop.load(Ordering::Acquire) {
                    let ids = match registry.registration_ids() {
                        Ok(ids) => ids,
                        Err(error) => {
                            publish(
                                &worker_status,
                                false,
                                Some(listeners.len() as u8),
                                Some(NotifyServiceError::Registry(error)),
                            );
                            thread::park_timeout(Duration::from_secs(1));
                            continue;
                        }
                    };
                    if worker_reload.swap(false, Ordering::AcqRel)
                        || ids != known_ids
                        || (last_error.is_some()
                            && last_refresh.elapsed() >= Duration::from_secs(5))
                    {
                        last_error = None;
                        let mut valid_ids = Vec::new();
                        for id in &ids {
                            let record = match registry.get(id) {
                                Ok(record) => record,
                                Err(error) => {
                                    last_error = Some(NotifyServiceError::Registry(error));
                                    continue;
                                }
                            };
                            match active(&record, &executable) {
                                Ok(false) => continue,
                                Err(error) => {
                                    last_error = Some(error);
                                    continue;
                                }
                                Ok(true) => {}
                            }
                            valid_ids.push(id.clone());
                            if listeners
                                .get(id)
                                .is_some_and(|(cap, _)| cap.matches(record.capability()))
                            {
                                continue;
                            }
                            listeners.remove(id); // Close old nonce / owned instance before replacement.
                            let flag = online.clone();
                            let owner = owner.clone();
                            let wake = Arc::new(move |_hint| {
                                flag.store(true, Ordering::Release);
                                owner.unpark();
                            });
                            match NotifyListener::start(record.capability().clone(), wake) {
                                Ok(listener) => {
                                    listeners.insert(
                                        id.clone(),
                                        (record.capability().clone(), listener),
                                    );
                                }
                                Err(_) => last_error = Some(NotifyServiceError::ChannelUnavailable),
                            }
                        }
                        listeners.retain(|id, _| valid_ids.contains(id));
                        known_ids = ids.clone();
                        last_refresh = Instant::now();
                    }
                    let mut claims = Vec::new();
                    for id in &ids {
                        let claim = match registry.claim_pending(id) {
                            Ok(Some(claim)) => claim,
                            Ok(None) => continue,
                            Err(error) => {
                                last_error = Some(NotifyServiceError::Registry(error));
                                continue;
                            }
                        };
                        let checked = registry
                            .get(id)
                            .map_err(NotifyServiceError::Registry)
                            .and_then(|record| active(&record, &executable));
                        match checked {
                            Ok(true) => {
                                claims.push(claim);
                                worker_reload.store(true, Ordering::Release);
                            }
                            Ok(false) => {
                                if let Err(error) = claim.complete() {
                                    last_error = Some(NotifyServiceError::Registry(error));
                                }
                            }
                            Err(error) => {
                                last_error = Some(error);
                                drop(claim);
                            }
                        }
                    }
                    if worker_stop.load(Ordering::Acquire) {
                        break;
                    }
                    if online.swap(false, Ordering::AcqRel) || !claims.is_empty() {
                        reconcile();
                        for claim in claims {
                            if let Err(error) = claim.complete() {
                                last_error = Some(NotifyServiceError::Registry(error));
                            }
                        }
                    }
                    publish(
                        &worker_status,
                        true,
                        Some(listeners.len() as u8),
                        last_error,
                    );
                    thread::park_timeout(Duration::from_secs(1));
                }
                drop(listeners);
                publish(&worker_status, false, Some(0), last_error);
            })
            .map_err(|_| RegistryError::Io)?;
        Ok(Self {
            stopping,
            reload,
            status,
            worker: Mutex::new(Some(worker)),
        })
    }
    pub fn status(&self) -> NotifyServiceStatus {
        self.status
            .lock()
            .map(|status| *status)
            .unwrap_or(NotifyServiceStatus {
                last_error: Some(NotifyServiceError::WorkerUnavailable),
                ..Default::default()
            })
    }
    pub fn reload(&self) {
        self.reload.store(true, Ordering::Release);
        if let Ok(worker) = self.worker.lock() {
            if let Some(worker) = worker.as_ref() {
                worker.thread().unpark();
            }
        }
    }
    pub fn shutdown(&self) {
        self.stopping.store(true, Ordering::Release);
        if let Ok(mut worker) = self.worker.lock() {
            if let Some(worker) = worker.take() {
                worker.thread().unpark();
                let _ = worker.join();
            }
        }
    }
}
impl Drop for NotifyService {
    fn drop(&mut self) {
        self.shutdown();
    }
}
fn active(record: &NotifyRegistration, executable: &Path) -> Result<bool, NotifyServiceError> {
    if Path::new(&record.restore_record().installed_arguments()[0]) != executable {
        return Err(NotifyServiceError::WrongExecutable);
    }
    let bytes =
        read_config(record.codex_home()).map_err(|_| NotifyServiceError::ConfigUnreadable)?;
    let owned = owns_current_notify(&bytes, record.restore_record())
        .map_err(|_| NotifyServiceError::ConfigUnreadable)?;
    if owned && record.chain_original() {
        return Err(NotifyServiceError::OriginalChainUnavailable);
    }
    Ok(owned)
}
fn publish(
    status: &Mutex<NotifyServiceStatus>,
    ready: bool,
    count: Option<u8>,
    error: Option<NotifyServiceError>,
) {
    if let Ok(mut status) = status.lock() {
        *status = NotifyServiceStatus {
            ready,
            listener_count: count,
            last_error: error,
        };
    }
}
