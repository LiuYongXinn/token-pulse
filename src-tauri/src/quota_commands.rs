use tauri::{Manager, State, WebviewWindow};
use token_pulse_core::{
    error::{AppError, ErrorCode},
    privacy::PrivateResponse,
    protocol::{QuotaSnapshot, validate_request_id},
    quota::QuotaRefreshResult,
};
use token_pulse_quota::service::DisplayEntry;
fn authorized(window: &WebviewWindow, id: &str) -> Result<(), Box<AppError>> {
    validate_request_id(id)
        .map_err(|code| Box::new(AppError::new(code, "invalid-request".into())))?;
    if !matches!(window.label(), "main" | "mini") {
        return Err(Box::new(AppError::new(
            ErrorCode::PermissionDenied,
            id.into(),
        )));
    }
    Ok(())
}
#[tauri::command]
pub fn get_account_quota(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request_id: String,
) -> Result<PrivateResponse<QuotaSnapshot>, Box<AppError>> {
    authorized(&window, &request_id)?;
    update_visibility(&window);
    let quota = state
        .quota
        .as_ref()
        .map_err(|code| Box::new(AppError::new(*code, request_id.clone())))?
        .snapshot()
        .map_err(|code| Box::new(AppError::new(code, request_id.clone())))?;
    Ok(PrivateResponse::new(
        request_id,
        quota,
        state.privacy.clone(),
    ))
}
#[tauri::command]
pub async fn refresh_account_quota(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    request_id: String,
) -> Result<PrivateResponse<QuotaRefreshResult>, Box<AppError>> {
    authorized(&window, &request_id)?;
    update_visibility(&window);
    let service = state
        .quota
        .as_ref()
        .cloned()
        .map_err(|code| Box::new(AppError::new(*code, request_id.clone())))?;
    let receipt = tauri::async_runtime::spawn_blocking(move || service.refresh())
        .await
        .map_err(|_| {
            Box::new(AppError::new(
                ErrorCode::QuotaServiceUnavailable,
                request_id.clone(),
            ))
        })?
        .map_err(|code| Box::new(AppError::new(code, request_id.clone())))?;
    let result = QuotaRefreshResult::from_decision(receipt.decision, receipt.snapshot)
        .map_err(|code| Box::new(AppError::new(code, request_id.clone())))?;
    Ok(PrivateResponse::new(
        request_id,
        result,
        state.privacy.clone(),
    ))
}
pub fn update_visibility(window: &WebviewWindow) {
    update_native_visibility(&window.as_ref().window());
}
pub fn update_native_visibility(window: &tauri::Window) {
    let entry = match window.label() {
        "main" => DisplayEntry::Main,
        "mini" => DisplayEntry::Mini,
        _ => return,
    };
    let Some(state) = window.app_handle().try_state::<super::RuntimeState>() else {
        return;
    };
    let Ok(service) = &state.quota else {
        return;
    };
    #[cfg(windows)]
    let visible = window.hwnd().is_ok_and(|h| unsafe {
        let hwnd = h.0.cast();
        windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd) != 0
            && windows_sys::Win32::UI::WindowsAndMessaging::IsIconic(hwnd) == 0
    });
    #[cfg(not(windows))]
    let visible = window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(true);
    service.set_visible(entry, visible);
}
