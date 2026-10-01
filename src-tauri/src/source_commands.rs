use std::{
    path::PathBuf,
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{Manager, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use token_pulse_core::{
    error::{AppError, ErrorCode},
    numeric::DecimalInt,
    protocol::{Response, validate_request_id},
    selections::SelectedDirectory,
    sources::*,
};
use token_pulse_store::source_management::SourceMutation;

fn authorized(window: &WebviewWindow, request: &str) -> Result<(), Box<AppError>> {
    validate_request_id(request)
        .map_err(|code| Box::new(AppError::new(code, "invalid-request".into())))?;
    if window.label() != "main" {
        return Err(Box::new(AppError::new(
            ErrorCode::PermissionDenied,
            request.into(),
        )));
    }
    Ok(())
}
#[tauri::command]
pub async fn get_sources(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request_id: String,
) -> Result<Response<SourcesSnapshot>, Box<AppError>> {
    authorized(&window, &request_id)?;
    let db = state
        .database
        .as_ref()
        .map_err(|e| Box::new(AppError::new(e.code, request_id.clone())))?
        .clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || db.sources_snapshot())
        .await
        .map_err(|_| Box::new(AppError::new(ErrorCode::DbWriteFailed, request_id.clone())))?
        .map_err(|e| Box::new(AppError::new(e.code, request_id.clone())))?;
    Ok(Response::new(request_id, snapshot))
}
#[tauri::command]
pub async fn choose_source_directory(
    window: WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, super::RuntimeState>,
    kind: SourceDirectoryKind,
    request_id: String,
) -> Result<Response<Option<SourceDirectorySelection>>, Box<AppError>> {
    authorized(&window, &request_id)?;
    let selections = Arc::clone(&state.selections);
    let result = tauri::async_runtime::spawn_blocking(
        move || -> Result<Option<SourceDirectorySelection>, ErrorCode> {
            let Some(selected) = app
                .dialog()
                .file()
                .set_parent(&window)
                .set_title(if kind == SourceDirectoryKind::Wsl {
                    "选择已启用 WSL 中的 Codex Home（\\\\wsl.localhost）"
                } else {
                    "选择 Codex Home（包含 sessions 的目录）"
                })
                .blocking_pick_folder()
            else {
                return Ok(None);
            };
            let origin = if kind == SourceDirectoryKind::Wsl {
                SourceOrigin::Wsl
            } else {
                SourceOrigin::Custom
            };
            let path = selected.into_path().map_err(|_| ErrorCode::InvalidQuery)?;
            let path = displayable_path(path, origin)?;
            validate_root(&path, origin)?;
            let path = std::fs::canonicalize(path).map_err(|_| ErrorCode::SourceUnreadable)?;
            let path = displayable_path(path, origin)?;
            validate_root(&path, origin)?;
            let handle = uuid::Uuid::new_v4().to_string();
            let root_path = path.to_str().ok_or(ErrorCode::InvalidQuery)?.to_owned();
            selections
                .lock()
                .map_err(|_| ErrorCode::DbWriteFailed)?
                .insert(
                    handle.clone(),
                    window.label().into(),
                    SelectedDirectory { path, origin },
                    Instant::now(),
                )?;
            Ok(Some(SourceDirectorySelection {
                selection_handle: handle,
                root_path,
                origin,
            }))
        },
    )
    .await
    .map_err(|_| {
        Box::new(AppError::new(
            ErrorCode::WindowUnavailable,
            request_id.clone(),
        ))
    })?
    .map_err(|code| Box::new(AppError::new(code, request_id.clone())))?;
    Ok(Response::new(request_id, result))
}
fn displayable_path(path: PathBuf, origin: SourceOrigin) -> Result<PathBuf, ErrorCode> {
    let text = path.to_str().ok_or(ErrorCode::InvalidQuery)?;
    Ok(if origin == SourceOrigin::Wsl {
        PathBuf::from(
            text.strip_prefix("\\\\?\\UNC\\")
                .map(|rest| format!("\\\\{rest}"))
                .unwrap_or_else(|| text.into()),
        )
    } else {
        path
    })
}
#[tauri::command]
pub async fn manage_source(
    window: WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, super::RuntimeState>,
    action: ManageSourceAction,
    expected_settings_revision: DecimalInt,
    request_id: String,
) -> Result<Response<SourcesSnapshot>, Box<AppError>> {
    authorized(&window, &request_id)?;
    let db = state
        .database
        .as_ref()
        .map_err(|e| Box::new(AppError::new(e.code, request_id.clone())))?
        .clone();
    let selections = Arc::clone(&state.selections);
    let label = window.label().to_owned();
    let expected = i64::try_from(expected_settings_revision.value()).map_err(|_| {
        Box::new(AppError::new(
            ErrorCode::NumericOverflow,
            request_id.clone(),
        ))
    })?;
    let result = tauri::async_runtime::spawn_blocking(
        move || -> token_pulse_store::StoreResult<SourcesSnapshot> {
            let mutation = match action {
                ManageSourceAction::Add { selection_handle } => {
                    let selected = selections
                        .lock()
                        .map_err(|_| ErrorCode::DbWriteFailed)?
                        .take(&selection_handle, &label, Instant::now())?;
                    SourceMutation::Add(vec![SourceCandidate {
                        root: selected.path,
                        origin: selected.origin,
                    }])
                }
                ManageSourceAction::Pause { source_id } => SourceMutation::Pause(source_id),
                ManageSourceAction::Resume { source_id } => SourceMutation::Resume(source_id),
                ManageSourceAction::RetainRemove { source_id } => {
                    SourceMutation::RetainRemove(source_id)
                }
                ManageSourceAction::Detect {} => SourceMutation::Add(local_candidates(
                    &[],
                    std::env::var_os("CODEX_HOME").map(PathBuf::from),
                    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                        .map(PathBuf::from),
                )),
            };
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| ErrorCode::InvalidQuery)?
                .as_millis();
            let now = i64::try_from(now).map_err(|_| ErrorCode::NumericOverflow)?;
            db.mutate_sources(mutation, expected, now)?;
            db.sources_snapshot()
        },
    )
    .await
    .map_err(|_| Box::new(AppError::new(ErrorCode::DbWriteFailed, request_id.clone())))?
    .map_err(|e| Box::new(AppError::new(e.code, request_id.clone())))?;
    if let Some(state) = app.try_state::<super::RuntimeState>() {
        if let Ok(collector) = &state.collector {
            collector.reconcile();
        }
    }
    Ok(Response::new(request_id, result))
}
