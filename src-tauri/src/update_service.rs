//! One native update owner; verified installation uses normal Tauri exit cleanup.
use super::update_transport::{Candidate, Publication, VerifiedDownload};
use std::{
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Runtime};
use token_pulse_core::{
    error::ErrorCode,
    numeric::{DecimalInt, EpochMs},
    updates::*,
};

struct Owner {
    workflow: UpdateWorkflow,
    candidate: Option<Candidate>,
    artifact: Option<VerifiedDownload>,
}
pub(super) struct UpdateService {
    publication: Option<Publication>,
    owner: Mutex<Owner>,
    notify: Arc<dyn Fn() + Send + Sync>,
}
impl UpdateService {
    #[cfg(test)]
    pub(super) fn fixture(
        publication: Publication,
        notify: Arc<dyn Fn() + Send + Sync>,
    ) -> Arc<Self> {
        Arc::new(Self {
            publication: Some(publication),
            owner: Mutex::new(Owner {
                workflow: UpdateWorkflow::new("0.1.0".into(), None).unwrap(),
                candidate: None,
                artifact: None,
            }),
            notify,
        })
    }
    pub fn production(current_version: String, notify: Arc<dyn Fn() + Send + Sync>) -> Arc<Self> {
        let publication = Publication::production(super::update_signing::public_key());
        let unavailable = publication.as_ref().err().copied();
        Arc::new(Self {
            publication: publication.ok(),
            owner: Mutex::new(Owner {
                workflow: UpdateWorkflow::new(current_version, unavailable)
                    .expect("compiled app version"),
                candidate: None,
                artifact: None,
            }),
            notify,
        })
    }
    pub fn snapshot(&self) -> Result<UpdateSnapshot, ErrorCode> {
        let owner = self
            .owner
            .lock()
            .map_err(|_| ErrorCode::UpdateUnavailable)?;
        let snapshot = owner.workflow.snapshot();
        if snapshot.phase == UpdatePhase::ReadyToInstall
            && !owner.artifact.as_ref().is_some_and(|artifact| {
                snapshot
                    .release
                    .as_ref()
                    .is_some_and(|release| artifact.version() == release.version)
                    && artifact.len() > 0
            })
        {
            return Err(ErrorCode::UpdateUnavailable);
        }
        Ok(snapshot)
    }
    pub fn start_check<R: Runtime>(
        self: &Arc<Self>,
        app: AppHandle<R>,
    ) -> Result<UpdateSnapshot, ErrorCode> {
        let publication = self
            .publication
            .clone()
            .ok_or(ErrorCode::UpdateUnavailable)?;
        let (operation, snapshot) = {
            let mut owner = self
                .owner
                .lock()
                .map_err(|_| ErrorCode::UpdateUnavailable)?;
            let operation = owner.workflow.begin_check()?;
            owner.candidate = None;
            owner.artifact = None;
            (operation, owner.workflow.snapshot())
        };
        (self.notify)();
        let service = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            match publication.check(&app).await {
                Ok(candidate) => {
                    let result = (|| {
                        let at = now()?;
                        let mut owner = service
                            .owner
                            .lock()
                            .map_err(|_| ErrorCode::UpdateUnavailable)?;
                        owner.workflow.checked(
                            operation,
                            candidate.as_ref().map(Candidate::metadata),
                            at,
                        )?;
                        owner.candidate = candidate;
                        Ok::<_, ErrorCode>(())
                    })();
                    if result.is_ok() {
                        (service.notify)();
                    } else {
                        service.failed(operation, UpdateIssue::InvalidRelease);
                    }
                }
                Err(issue) => service.failed(operation, issue),
            }
        });
        Ok(snapshot)
    }
    pub fn start_download(
        self: &Arc<Self>,
        expected: &DecimalInt,
    ) -> Result<UpdateSnapshot, ErrorCode> {
        let (operation, candidate, snapshot) = {
            let mut owner = self
                .owner
                .lock()
                .map_err(|_| ErrorCode::UpdateUnavailable)?;
            let current = owner.workflow.snapshot();
            if &current.update_revision != expected {
                return Err(ErrorCode::RevisionConflict);
            }
            if current.phase == UpdatePhase::Unavailable {
                return Err(ErrorCode::UpdateUnavailable);
            }
            if matches!(
                current.phase,
                UpdatePhase::Checking
                    | UpdatePhase::Downloading
                    | UpdatePhase::Verifying
                    | UpdatePhase::Installing
            ) {
                return Err(ErrorCode::UpdateBusy);
            }
            let candidate = owner.candidate.clone().ok_or(ErrorCode::InvalidQuery)?;
            let operation = owner.workflow.begin_download(expected)?;
            owner.artifact = None;
            (operation, candidate, owner.workflow.snapshot())
        };
        (self.notify)();
        let service = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            let progress_error = Arc::new(Mutex::new(None));
            let chunk_error = Arc::clone(&progress_error);
            let finish_error = Arc::clone(&progress_error);
            let chunk_service = Arc::clone(&service);
            let finish_service = Arc::clone(&service);
            let result = candidate
                .download(
                    move |chunk, total| {
                        let Ok(mut failure) = chunk_error.lock() else {
                            return;
                        };
                        if failure.is_some() {
                            return;
                        }
                        let updated = (|| {
                            let mut owner = chunk_service
                                .owner
                                .lock()
                                .map_err(|_| ErrorCode::UpdateUnavailable)?;
                            owner.workflow.progress(
                                operation,
                                u64::try_from(chunk).map_err(|_| ErrorCode::NumericOverflow)?,
                                total,
                            )
                        })();
                        if let Err(error) = updated {
                            *failure = Some(error);
                        } else {
                            (chunk_service.notify)();
                        }
                    },
                    move || {
                        if finish_error.lock().is_ok_and(|failure| failure.is_some()) {
                            return;
                        }
                        let updated = (|| {
                            let mut owner = finish_service
                                .owner
                                .lock()
                                .map_err(|_| ErrorCode::UpdateUnavailable)?;
                            owner.workflow.begin_verification(operation)
                        })();
                        if updated.is_ok() {
                            (finish_service.notify)();
                        }
                    },
                )
                .await;
            // Do not release the operation while the actual transport future is still live.
            // This keeps a failed progress callback from allowing overlapping downloads.
            if progress_error
                .lock()
                .map(|failure| failure.is_some())
                .unwrap_or(true)
            {
                service.failed(operation, UpdateIssue::DownloadFailed);
                return;
            }
            match result {
                Ok(artifact) => {
                    let updated = (|| {
                        let mut owner = service
                            .owner
                            .lock()
                            .map_err(|_| ErrorCode::UpdateUnavailable)?;
                        if owner
                            .workflow
                            .snapshot()
                            .release
                            .as_ref()
                            .is_none_or(|release| artifact.version() != release.version)
                        {
                            return Err(ErrorCode::CandidateObsolete);
                        }
                        owner.workflow.verified(
                            operation,
                            u64::try_from(artifact.len())
                                .map_err(|_| ErrorCode::NumericOverflow)?,
                        )?;
                        owner.artifact = Some(artifact);
                        owner.candidate = None;
                        Ok::<_, ErrorCode>(())
                    })();
                    if updated.is_ok() {
                        (service.notify)();
                    } else {
                        service.failed(operation, UpdateIssue::DownloadFailed);
                    }
                }
                Err(issue) => service.failed(operation, issue),
            }
        });
        Ok(snapshot)
    }
    pub fn start_install<R: Runtime>(
        self: &Arc<Self>,
        app: AppHandle<R>,
        expected: &DecimalInt,
    ) -> Result<UpdateSnapshot, ErrorCode> {
        self.install(
            expected,
            super::update_installer::supported(&app),
            Arc::new(move || app.exit(0)),
        )
    }
    fn install(
        self: &Arc<Self>,
        expected: &DecimalInt,
        supported: bool,
        exit: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<UpdateSnapshot, ErrorCode> {
        let (operation, artifact, snapshot) = {
            let mut owner = self
                .owner
                .lock()
                .map_err(|_| ErrorCode::UpdateUnavailable)?;
            let current = owner.workflow.snapshot();
            if &current.update_revision != expected {
                return Err(ErrorCode::RevisionConflict);
            }
            if !supported {
                return Err(ErrorCode::UpdateUnavailable);
            }
            if !owner.artifact.as_ref().is_some_and(|artifact| {
                current
                    .release
                    .as_ref()
                    .is_some_and(|release| artifact.version() == release.version)
                    && artifact.len() > 0
            }) {
                return Err(
                    if matches!(
                        current.phase,
                        UpdatePhase::Checking
                            | UpdatePhase::Downloading
                            | UpdatePhase::Verifying
                            | UpdatePhase::Installing
                    ) {
                        ErrorCode::UpdateBusy
                    } else {
                        ErrorCode::UpdateUnavailable
                    },
                );
            }
            let operation = owner.workflow.begin_install(expected)?;
            let artifact = owner.artifact.take().ok_or(ErrorCode::UpdateUnavailable)?;
            (operation, artifact, owner.workflow.snapshot())
        };
        (self.notify)();
        let service = Arc::clone(self);
        tauri::async_runtime::spawn_blocking(move || match artifact.launch_installer() {
            Ok(()) => exit(),
            Err(issue) => service.failed(operation, issue),
        });
        Ok(snapshot)
    }
    #[cfg(test)]
    pub(super) fn install_fixture(
        self: &Arc<Self>,
        expected: &DecimalInt,
        exit: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<UpdateSnapshot, ErrorCode> {
        self.install(expected, true, exit)
    }
    fn failed(&self, operation: UpdateOperation, issue: UpdateIssue) {
        let issue = if matches!(
            issue,
            UpdateIssue::PublicationNotConfigured | UpdateIssue::UnsupportedPlatform
        ) {
            UpdateIssue::InvalidRelease
        } else {
            issue
        };
        let changed = if let Ok(mut owner) = self.owner.lock() {
            if owner.workflow.failed(operation, issue).is_ok() {
                owner.candidate = None;
                owner.artifact = None;
                true
            } else {
                false
            }
        } else {
            false
        };
        if changed {
            (self.notify)();
        }
    }
}
fn now() -> Result<EpochMs, ErrorCode> {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ErrorCode::InvalidQuery)?
        .as_millis();
    EpochMs::new(i64::try_from(ms).map_err(|_| ErrorCode::NumericOverflow)?)
}
