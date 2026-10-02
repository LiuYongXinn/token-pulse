//! Main-window-only reviewed notify operations. No arbitrary frontend command/path inputs.
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex},
};
use tauri::{State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use token_pulse_core::{
    error::{AppError, ErrorCode, ErrorDetail},
    notify_integration::*,
    numeric::DecimalInt,
    privacy::{DisplayPolicyStamp, PrivacyState, PrivateResponse},
    protocol::{Response, validate_request_id},
};
use token_pulse_integration::{
    notify_config::{ConfigError, windows::ConfigFileError},
    notify_manager::{NotifyManager, NotifyOperation, OperationError},
    notify_registry::{
        RegistryError,
        windows::{NotifyRegistry, RetireError},
    },
    notify_service::NotifyServiceError,
};

pub(super) struct NotifyOperations {
    manager: NotifyManager,
    revisions: BTreeMap<String, (DecimalInt, DecimalInt)>,
}
pub(super) type SharedOperations = Arc<Mutex<Result<NotifyOperations, NotifyIssue>>>;
pub(super) fn initialize(directory: &Path) -> SharedOperations {
    let result = (|| {
        Ok(NotifyOperations {
            manager: NotifyManager::new(
                NotifyRegistry::open(directory).map_err(registry_issue)?,
                std::env::current_exe().map_err(|_| NotifyIssue::Unavailable)?,
            ),
            revisions: BTreeMap::new(),
        })
    })();
    Arc::new(Mutex::new(result))
}
#[derive(Clone, Copy, Debug)]
enum Failure {
    Code(ErrorCode),
    Issue(NotifyIssue),
}
impl From<ErrorCode> for Failure {
    fn from(value: ErrorCode) -> Self {
        Self::Code(value)
    }
}
impl From<OperationError> for Failure {
    fn from(value: OperationError) -> Self {
        Self::Issue(operation_issue(value))
    }
}
fn app_error(value: Failure, id: &str) -> Box<AppError> {
    match value {
        Failure::Code(code) => Box::new(AppError::new(code, id.into())),
        Failure::Issue(issue) => {
            let mut error = AppError::new(ErrorCode::NotifyIntegrationFailed, id.into());
            error.retryable = matches!(
                issue,
                NotifyIssue::Busy
                    | NotifyIssue::Unavailable
                    | NotifyIssue::ChannelUnavailable
                    | NotifyIssue::WorkerUnavailable
            );
            // Only a finite enum crosses IPC, never OS/parser/config diagnostic text.
            error.details.insert(
                "notify_issue".into(),
                ErrorDetail::Text(
                    serde_json::to_value(issue)
                        .expect("finite issue")
                        .as_str()
                        .expect("issue enum")
                        .into(),
                ),
            );
            Box::new(error)
        }
    }
}
fn main_only(label: &str, id: &str) -> Result<(), Box<AppError>> {
    validate_request_id(id)
        .map_err(|code| Box::new(AppError::new(code, "invalid-request".into())))?;
    if label != "main" {
        return Err(Box::new(AppError::new(
            ErrorCode::PermissionDenied,
            id.into(),
        )));
    }
    Ok(())
}
fn operate<T>(
    operations: &SharedOperations,
    f: impl FnOnce(&mut NotifyOperations) -> Result<T, Failure>,
) -> Result<T, Failure> {
    let mut guard = operations
        .lock()
        .map_err(|_| Failure::Issue(NotifyIssue::WorkerUnavailable))?;
    f(guard.as_mut().map_err(|issue| Failure::Issue(*issue))?)
}
fn registry_issue(error: RegistryError) -> NotifyIssue {
    match error {
        RegistryError::Io => NotifyIssue::Unavailable,
        RegistryError::UnsafePath => NotifyIssue::UnsafePath,
        RegistryError::UnsafePermissions => NotifyIssue::UnsafePermissions,
        RegistryError::InvalidRecord => NotifyIssue::InvalidRegistration,
        RegistryError::InvalidMarker => NotifyIssue::InvalidMarker,
        RegistryError::TooLarge | RegistryError::LimitReached => NotifyIssue::LimitReached,
        RegistryError::AlreadyExists => NotifyIssue::AlreadyExists,
        RegistryError::NotFound => NotifyIssue::NotFound,
        RegistryError::Unauthorized => NotifyIssue::OwnershipChanged,
    }
}
fn config_issue(error: ConfigFileError) -> NotifyIssue {
    match error {
        ConfigFileError::Config(value) => match value {
            ConfigError::TooLarge => NotifyIssue::LimitReached,
            ConfigError::InvalidToml => NotifyIssue::InvalidConfig,
            ConfigError::InvalidNotify | ConfigError::InvalidManagedCommand => {
                NotifyIssue::InvalidNotify
            }
            ConfigError::AlreadyManaged => NotifyIssue::AlreadyManaged,
            ConfigError::StalePlan => NotifyIssue::ConfigChanged,
            ConfigError::OwnershipConflict => NotifyIssue::OwnershipChanged,
            ConfigError::InvalidRestoreRecord => NotifyIssue::InvalidRegistration,
        },
        ConfigFileError::UnsafePath => NotifyIssue::UnsafePath,
        ConfigFileError::UnsafeFile => NotifyIssue::UnsafeFile,
        ConfigFileError::Busy => NotifyIssue::Busy,
        ConfigFileError::PermissionDenied => NotifyIssue::PermissionDenied,
        ConfigFileError::Unsupported => NotifyIssue::TransactionUnavailable,
        ConfigFileError::Io => NotifyIssue::Unavailable,
    }
}
fn operation_issue(error: OperationError) -> NotifyIssue {
    match error {
        OperationError::Registry(value) => registry_issue(value),
        OperationError::Config(value) => config_issue(value),
        OperationError::Retirement(value) => match value {
            RetireError::Registry(value) => registry_issue(value),
            RetireError::Config(value) => config_issue(value),
            RetireError::ActiveConfiguration => NotifyIssue::ActiveConfiguration,
        },
        OperationError::NoOriginalCommand => NotifyIssue::NoOriginalCommand,
        OperationError::PlanNotFound => NotifyIssue::PlanNotFound,
        OperationError::PlanExpired => NotifyIssue::PlanExpired,
        OperationError::PlanLimit => NotifyIssue::PlanLimit,
        OperationError::ConfigApplyCleanupFailed(_, _) => NotifyIssue::CleanupFailed,
    }
}
fn service_issue(error: NotifyServiceError) -> NotifyIssue {
    match error {
        NotifyServiceError::Registry(value) => registry_issue(value),
        NotifyServiceError::ConfigUnreadable => NotifyIssue::Unavailable,
        NotifyServiceError::WrongExecutable => NotifyIssue::WrongExecutable,
        NotifyServiceError::ChannelUnavailable => NotifyIssue::ChannelUnavailable,
        NotifyServiceError::WorkerUnavailable => NotifyIssue::WorkerUnavailable,
    }
}
#[tauri::command]
pub async fn get_notify_integrations(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request_id: String,
) -> Result<PrivateResponse<NotifyIntegrationsSnapshot>, Box<AppError>> {
    main_only(window.label(), &request_id)?;
    let (ready, listener_count, service_issue) = match state.notify.lock() {
        Ok(service) => match service.as_ref() {
            Ok(service) => {
                let status = service.status();
                (
                    status.ready,
                    status.listener_count,
                    status.last_error.map(service_issue),
                )
            }
            Err(error) => (false, None, Some(registry_issue(*error))),
        },
        Err(_) => (false, None, Some(NotifyIssue::WorkerUnavailable)),
    };
    let operations = state.notify_operations.clone();
    let rows = tauri::async_runtime::spawn_blocking(move || {
        operate(&operations, |owner| Ok(owner.manager.registrations()?))
    })
    .await
    .map_err(|_| app_error(Failure::Issue(NotifyIssue::WorkerUnavailable), &request_id))?;
    let (registrations, registry_issue) = match rows {
        Ok(rows) => (
            Some(
                rows.into_iter()
                    .map(|row| NotifyIntegrationRow {
                        registration_id: row.registration_id,
                        home_path: row.home.map(|p| p.to_string_lossy().into_owned()),
                        configured: row.configured,
                        current_executable: row.current_executable,
                        chain_original: row.chain_original,
                        issue: row.error.map(operation_issue),
                    })
                    .collect(),
            ),
            None,
        ),
        Err(Failure::Issue(issue)) => (None, Some(issue)),
        Err(Failure::Code(_)) => (None, Some(NotifyIssue::Unavailable)),
    };
    Ok(PrivateResponse::new(
        request_id,
        NotifyIntegrationsSnapshot {
            ready,
            listener_count,
            service_issue,
            registrations,
            registry_issue,
            redacted: false,
        },
        state.privacy.clone(),
    ))
}
fn visible<T>(
    privacy: &PrivacyState,
    f: impl FnOnce(&DisplayPolicyStamp) -> Result<T, Failure>,
) -> Result<T, Failure> {
    privacy.with_visible_operation(f)
}
#[tauri::command]
pub async fn prepare_notify_integration(
    window: WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, super::RuntimeState>,
    request: NotifyPrepareAction,
    request_id: String,
) -> Result<PrivateResponse<Option<NotifyConfigPreview>>, Box<AppError>> {
    main_only(window.label(), &request_id)?;
    request
        .validate()
        .map_err(|code| app_error(code.into(), &request_id))?;
    let privacy = state.privacy.clone();
    let policy_before = privacy
        .current()
        .map_err(|code| app_error(code.into(), &request_id))?;
    if policy_before.privacy {
        return Err(app_error(ErrorCode::PermissionDenied.into(), &request_id));
    }
    let operations = state.notify_operations.clone();
    let db = state
        .database
        .as_ref()
        .cloned()
        .map_err(|e| app_error(e.code.into(), &request_id))?;
    #[cfg(all(debug_assertions, windows))]
    let fixture_home = if std::env::args().any(|a| a == "--native-smoke")
        && std::env::args().any(|a| a == "--native-notify-dialogs-smoke")
        && state
            .data_directory
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("native-probe-"))
    {
        Some(state.data_directory.join("synthetic-notify-dialog-home"))
    } else {
        None
    };
    let result = tauri::async_runtime::spawn_blocking(
        move || -> Result<Option<NotifyConfigPreview>, Failure> {
            // Never hold the policy/manager mutex while a system dialog waits for a human.
            let selected = if matches!(request, NotifyPrepareAction::ChooseHome { .. }) {
                let Some(folder) = app
                    .dialog()
                    .file()
                    .set_parent(&window)
                    .set_title("选择要接入通知的 Codex Home")
                    .blocking_pick_folder()
                else {
                    return Ok(None);
                };
                Some(folder.into_path().map_err(|_| ErrorCode::InvalidQuery)?)
            } else {
                None
            };
            #[cfg(all(debug_assertions, windows))]
            if let Some(expected) = fixture_home {
                let expected = expected
                    .canonicalize()
                    .map_err(|_| ErrorCode::PermissionDenied)?;
                if matches!(request, NotifyPrepareAction::EnableSource { .. }) {
                    return Err(ErrorCode::PermissionDenied.into());
                }
                if let Some(home) = &selected {
                    if home
                        .canonicalize()
                        .map_err(|_| ErrorCode::PermissionDenied)?
                        != expected
                    {
                        return Err(ErrorCode::PermissionDenied.into());
                    }
                }
            }
            visible(&privacy, |stamp| {
                if stamp.settings_revision != policy_before.settings_revision {
                    return Err(ErrorCode::StaleConfirmation.into());
                }
                let sources = db.sources_snapshot().map_err(|e| Failure::Code(e.code))?;
                operate(&operations, |owner| {
                    owner
                        .revisions
                        .retain(|id, _| owner.manager.has_current_plan(id));
                    let preview = match request {
                        NotifyPrepareAction::EnableSource {
                            source_id,
                            chain_original,
                        } => {
                            let source = sources
                                .sources
                                .iter()
                                .find(|s| {
                                    s.source_id == source_id
                                        && !s.removed
                                        && s.origin != token_pulse_core::sources::SourceOrigin::Wsl
                                })
                                .ok_or(ErrorCode::InvalidQuery)?;
                            owner
                                .manager
                                .prepare_enable(Path::new(&source.root_path), chain_original)?
                        }
                        NotifyPrepareAction::ChooseHome { chain_original } => {
                            owner.manager.prepare_enable(
                                selected.as_deref().ok_or(ErrorCode::InvalidQuery)?,
                                chain_original,
                            )?
                        }
                        NotifyPrepareAction::Disable { registration_id } => {
                            owner.manager.prepare_disable(&registration_id)?
                        }
                    };
                    // Expired manager drafts are bounded independently; avoid an unbounded revision map.
                    owner.revisions.insert(
                        preview.plan_id.clone(),
                        (
                            sources.settings_revision.clone(),
                            stamp.settings_revision.clone(),
                        ),
                    );
                    Ok(Some(NotifyConfigPreview {
                        plan_id: preview.plan_id,
                        registration_id: preview.registration_id,
                        operation: match preview.operation {
                            NotifyOperation::Enable => NotifyConfigOperation::Enable,
                            NotifyOperation::Disable => NotifyConfigOperation::Disable,
                        },
                        home_path: Some(preview.home.to_string_lossy().into_owned()),
                        before_notify: preview.before_value,
                        after_notify: preview.after_value,
                        creates_config: preview.creates_config,
                        can_chain_original: preview.can_chain_original,
                        chain_original: preview.chain_original,
                        settings_revision: sources.settings_revision,
                        expires_in_seconds: 120,
                        redacted: false,
                    }))
                })
            })
        },
    )
    .await
    .map_err(|_| app_error(Failure::Issue(NotifyIssue::WorkerUnavailable), &request_id))?
    .map_err(|failure| app_error(failure, &request_id))?;
    Ok(PrivateResponse::new(
        request_id,
        result,
        state.privacy.clone(),
    ))
}
#[tauri::command]
pub async fn apply_notify_integration(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    plan_id: String,
    request_id: String,
) -> Result<PrivateResponse<NotifyApplyResult>, Box<AppError>> {
    main_only(window.label(), &request_id)?;
    validate_notify_id(&plan_id).map_err(|code| app_error(code.into(), &request_id))?;
    let operations = state.notify_operations.clone();
    let privacy = state.privacy.clone();
    let db = state
        .database
        .as_ref()
        .cloned()
        .map_err(|e| app_error(e.code.into(), &request_id))?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        visible(&privacy, |stamp| {
            operate(&operations, |owner| {
                let binding = owner
                    .revisions
                    .get(&plan_id)
                    .ok_or(Failure::Issue(NotifyIssue::PlanNotFound))?;
                if stamp.settings_revision != binding.1
                    || db
                        .sources_snapshot()
                        .map_err(|e| Failure::Code(e.code))?
                        .settings_revision
                        != binding.0
                {
                    owner.revisions.remove(&plan_id);
                    owner.manager.release(&plan_id);
                    return Err(ErrorCode::StaleConfirmation.into());
                }
                let outcome = owner.manager.apply(&plan_id)?;
                owner.revisions.remove(&plan_id);
                Ok(NotifyApplyResult {
                    registration_id: outcome.registration_id,
                    configured: Some(outcome.configured),
                    retired: !outcome.configured && outcome.cleanup_error.is_none(),
                    cleanup_issue: outcome.cleanup_error.map(operation_issue),
                })
            })
        })
    })
    .await
    .map_err(|_| app_error(Failure::Issue(NotifyIssue::WorkerUnavailable), &request_id))?
    .map_err(|failure| app_error(failure, &request_id))?;
    reload(&state);
    Ok(PrivateResponse::new(
        request_id,
        result,
        state.privacy.clone(),
    ))
}
#[tauri::command]
pub async fn retire_notify_integration(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    registration_id: String,
    request_id: String,
) -> Result<PrivateResponse<NotifyApplyResult>, Box<AppError>> {
    main_only(window.label(), &request_id)?;
    validate_notify_id(&registration_id).map_err(|code| app_error(code.into(), &request_id))?;
    let operations = state.notify_operations.clone();
    let privacy = state.privacy.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        visible(&privacy, |_| {
            operate(&operations, |owner| {
                owner.manager.retire_inactive(&registration_id)?;
                Ok(NotifyApplyResult {
                    registration_id,
                    configured: None,
                    retired: true,
                    cleanup_issue: None,
                })
            })
        })
    })
    .await
    .map_err(|_| app_error(Failure::Issue(NotifyIssue::WorkerUnavailable), &request_id))?
    .map_err(|failure| app_error(failure, &request_id))?;
    reload(&state);
    Ok(PrivateResponse::new(
        request_id,
        result,
        state.privacy.clone(),
    ))
}
#[tauri::command]
pub async fn release_notify_preview(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    plan_id: String,
    request_id: String,
) -> Result<Response<()>, Box<AppError>> {
    main_only(window.label(), &request_id)?;
    validate_notify_id(&plan_id).map_err(|code| app_error(code.into(), &request_id))?;
    let operations = state.notify_operations.clone();
    tauri::async_runtime::spawn_blocking(move || {
        operate(&operations, |owner| {
            owner.revisions.remove(&plan_id);
            owner.manager.release(&plan_id);
            Ok(())
        })
    })
    .await
    .map_err(|_| app_error(Failure::Issue(NotifyIssue::WorkerUnavailable), &request_id))?
    .map_err(|failure| app_error(failure, &request_id))?;
    Ok(Response::new(request_id, ()))
}
fn reload(state: &super::RuntimeState) {
    if let Ok(owner) = state.notify.lock() {
        if let Ok(service) = owner.as_ref() {
            service.reload();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finite_errors_distinguish_busy_stale_provider_and_successful_cleanup_failure() {
        assert_eq!(
            operation_issue(OperationError::Config(ConfigFileError::Busy)),
            NotifyIssue::Busy
        );
        assert_eq!(
            operation_issue(OperationError::Config(ConfigFileError::Unsupported)),
            NotifyIssue::TransactionUnavailable
        );
        assert_eq!(
            operation_issue(OperationError::Config(ConfigFileError::Config(
                ConfigError::StalePlan
            ))),
            NotifyIssue::ConfigChanged
        );
        assert_eq!(
            operation_issue(OperationError::ConfigApplyCleanupFailed(
                ConfigFileError::Io,
                RetireError::Registry(RegistryError::Io)
            )),
            NotifyIssue::CleanupFailed
        );
        let error = app_error(Failure::Issue(NotifyIssue::Busy), "notify-test");
        assert!(error.retryable);
        assert_eq!(
            serde_json::to_value(error).unwrap()["details"],
            serde_json::json!({"notify_issue":"busy"})
        );
    }
    #[test]
    fn all_management_operations_use_main_label_and_validated_request_identity() {
        assert!(main_only("main", "notify-request").is_ok());
        for label in ["mini", "floating", "arbitrary", ""] {
            assert_eq!(
                main_only(label, "notify-request").unwrap_err().code,
                ErrorCode::PermissionDenied
            );
        }
        assert_eq!(
            main_only("main", "").unwrap_err().code,
            ErrorCode::InvalidQuery
        );
    }
}
