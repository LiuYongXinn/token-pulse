use tauri::{Emitter, Manager, State, WebviewWindow};
use token_pulse_core::{
    error::{AppError, ErrorCode},
    numeric::DecimalInt,
    pricing::{ModelAliasMutation, PriceChanged, PriceRuleMutation, PriceRulesSnapshot},
    privacy::{PrivacyState, PrivateResponse},
    protocol::validate_request_id,
};

pub(super) fn authorized(window: &WebviewWindow, id: &str) -> Result<(), Box<AppError>> {
    validate_request_id(id)
        .map_err(|code| Box::new(AppError::new(code, "invalid-request".into())))?;
    if window.label() != "main" {
        return Err(Box::new(AppError::new(
            ErrorCode::PermissionDenied,
            id.into(),
        )));
    }
    Ok(())
}
fn revision(value: &str, id: &str) -> Result<i64, Box<AppError>> {
    let value =
        DecimalInt::parse(value).map_err(|code| Box::new(AppError::new(code, id.into())))?;
    i64::try_from(value.value())
        .map_err(|_| Box::new(AppError::new(ErrorCode::NumericOverflow, id.into())))
}
pub(super) fn database(
    state: &State<'_, super::RuntimeState>,
    id: &str,
) -> Result<token_pulse_store::Database, Box<AppError>> {
    state
        .database
        .as_ref()
        .cloned()
        .map_err(|e| Box::new(AppError::new(e.code, id.into())))
}
pub(super) async fn blocking<T: Send + 'static>(
    id: &str,
    task: impl FnOnce() -> token_pulse_store::StoreResult<T> + Send + 'static,
) -> Result<T, Box<AppError>> {
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|_| Box::new(AppError::new(ErrorCode::DbWriteFailed, id.into())))?
        .map_err(|e| Box::new(AppError::new(e.code, id.into())))
}

#[tauri::command]
pub async fn get_offline_price_catalog(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    revision: Option<String>,
    request_id: String,
) -> Result<
    PrivateResponse<token_pulse_core::pricing::offline::OfflinePriceCatalogSnapshot>,
    Box<AppError>,
> {
    authorized(&window, &request_id)?;
    let requested = revision
        .as_deref()
        .map(|v| self::revision(v, &request_id))
        .transpose()?;
    let db = database(&state, &request_id)?;
    let snapshot = blocking(&request_id, move || db.offline_price_catalog_at(requested)).await?;
    Ok(PrivateResponse::new(
        request_id,
        snapshot,
        state.privacy.clone(),
    ))
}

#[tauri::command]
pub async fn get_price_rules(
    window: WebviewWindow,
    state: State<'_, super::RuntimeState>,
    revision: Option<String>,
    request_id: String,
) -> Result<PrivateResponse<PriceRulesSnapshot>, Box<AppError>> {
    authorized(&window, &request_id)?;
    let requested = revision
        .as_deref()
        .map(|v| self::revision(v, &request_id))
        .transpose()?;
    let db = database(&state, &request_id)?;
    let snapshot = blocking(&request_id, move || db.price_rules_at(requested)).await?;
    Ok(PrivateResponse::new(
        request_id,
        snapshot,
        state.privacy.clone(),
    ))
}

async fn mutate(
    app: &tauri::AppHandle,
    db: token_pulse_store::Database,
    mutation: PriceRuleMutation,
    expected: i64,
    id: String,
    policy: PrivacyState,
) -> Result<PrivateResponse<PriceRulesSnapshot>, Box<AppError>> {
    let snapshot = blocking(&id, move || {
        db.mutate_price_rule_snapshot(mutation, expected, token_pulse_collector::jobs::now_ms()?)
    })
    .await?;
    wake_revalue(app);
    // A dropped notification never rolls back a published price revision.
    let _ = app.emit(
        "price_rules_changed",
        PriceChanged {
            price_revision: snapshot.price_revision.clone(),
            all_models: true,
        },
    );
    Ok(PrivateResponse::new(id, snapshot, policy))
}

#[tauri::command]
pub async fn mutate_model_alias(
    window: WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, super::RuntimeState>,
    request: ModelAliasMutation,
    expected_price_revision: String,
    request_id: String,
) -> Result<PrivateResponse<PriceRulesSnapshot>, Box<AppError>> {
    authorized(&window, &request_id)?;
    let expected = revision(&expected_price_revision, &request_id)?;
    let db = database(&state, &request_id)?;
    let snapshot = blocking(&request_id, move || {
        db.mutate_model_alias_snapshot(request, expected, token_pulse_collector::jobs::now_ms()?)
    })
    .await?;
    wake_revalue(&app);
    let _ = app.emit(
        "price_rules_changed",
        PriceChanged {
            price_revision: snapshot.price_revision.clone(),
            all_models: true,
        },
    );
    Ok(PrivateResponse::new(
        request_id,
        snapshot,
        state.privacy.clone(),
    ))
}

fn wake_revalue(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<super::RuntimeState>() {
        if let Ok(service) = &state.revaluations {
            service.wake();
        }
    }
}

#[tauri::command]
pub async fn save_price_rule(
    window: WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, super::RuntimeState>,
    request: PriceRuleMutation,
    expected_price_revision: String,
    request_id: String,
) -> Result<PrivateResponse<PriceRulesSnapshot>, Box<AppError>> {
    authorized(&window, &request_id)?;
    if matches!(request, PriceRuleMutation::Retire { .. }) {
        return Err(Box::new(AppError::new(ErrorCode::InvalidQuery, request_id)));
    }
    let expected = revision(&expected_price_revision, &request_id)?;
    mutate(
        &app,
        database(&state, &request_id)?,
        request,
        expected,
        request_id,
        state.privacy.clone(),
    )
    .await
}

#[tauri::command]
pub async fn retire_price_rule(
    window: WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, super::RuntimeState>,
    rule_id: String,
    expected_price_revision: String,
    request_id: String,
) -> Result<PrivateResponse<PriceRulesSnapshot>, Box<AppError>> {
    authorized(&window, &request_id)?;
    let expected = revision(&expected_price_revision, &request_id)?;
    mutate(
        &app,
        database(&state, &request_id)?,
        PriceRuleMutation::Retire { rule_id },
        expected,
        request_id,
        state.privacy.clone(),
    )
    .await
}
