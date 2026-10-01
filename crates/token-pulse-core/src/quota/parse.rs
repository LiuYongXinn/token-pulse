use crate::{
    error::ErrorCode,
    numeric::EpochMs,
    protocol::{QuotaLimit, QuotaWindow},
};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
pub const MAX_QUOTA_BUCKETS: usize = 128;
pub const LEGACY_LIMIT_ID: &str = "legacy";
#[derive(Debug, Clone)]
pub struct QuotaBucket {
    pub limit: QuotaLimit,
    pub windows: Vec<QuotaWindow>,
    /// A local identifier for an old server that omitted a bucket identity.
    pub legacy_identity: bool,
}
pub type QuotaBook = BTreeMap<String, QuotaBucket>;
#[derive(Debug, Clone)]
pub enum QuotaUpdate {
    Replace(QuotaBook),
    Bucket(QuotaBucket),
    Unidentified(QuotaBucket),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountAvailability {
    QuotaEligible,
    AuthorizationRequired,
    Unsupported,
}
pub fn account_availability(result: &Value) -> Result<AccountAvailability, ErrorCode> {
    let object = result.as_object().ok_or(ErrorCode::QuotaProtocolError)?;
    let requires = object
        .get("requiresOpenaiAuth")
        .and_then(Value::as_bool)
        .ok_or(ErrorCode::QuotaProtocolError)?;
    match object.get("account") {
        None | Some(Value::Null) => Ok(if requires {
            AccountAvailability::AuthorizationRequired
        } else {
            AccountAvailability::Unsupported
        }),
        Some(Value::Object(account)) => match account.get("type").and_then(Value::as_str) {
            Some("chatgpt") => Ok(AccountAvailability::QuotaEligible),
            Some(_) => Ok(AccountAvailability::Unsupported),
            None => Err(ErrorCode::QuotaProtocolError),
        },
        _ => Err(ErrorCode::QuotaProtocolError),
    }
}
fn identity(value: &str) -> Result<String, ErrorCode> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        Err(ErrorCode::QuotaProtocolError)
    } else {
        Ok(value.into())
    }
}
fn field_string(object: &Map<String, Value>, field: &str) -> Result<Option<String>, ErrorCode> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => identity(value).map(Some),
        _ => Err(ErrorCode::QuotaProtocolError),
    }
}
fn window(value: &Value, id: &str) -> Result<Option<QuotaWindow>, ErrorCode> {
    if value.is_null() {
        return Ok(None);
    }
    let object = value.as_object().ok_or(ErrorCode::QuotaProtocolError)?;
    let used = object
        .get("usedPercent")
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite());
    let duration = object
        .get("windowDurationMins")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n > 0);
    let reset = object
        .get("resetsAt")
        .and_then(Value::as_i64)
        .and_then(|n| n.checked_mul(1000))
        .and_then(|n| EpochMs::new(n).ok());
    Ok(Some(QuotaWindow {
        window_id: id.into(),
        duration_mins: duration,
        used_percent: used,
        remaining_percent: used.map(|n| (100.0 - n).clamp(0.0, 100.0)),
        resets_at_ms: reset,
    }))
}
fn bucket(value: &Value, map_id: Option<&str>) -> Result<QuotaBucket, ErrorCode> {
    let object = value.as_object().ok_or(ErrorCode::QuotaProtocolError)?;
    let declared = field_string(object, "limitId")?;
    if let (Some(mapped), Some(declared)) = (map_id, declared.as_deref()) {
        if mapped != declared {
            return Err(ErrorCode::QuotaProtocolError);
        }
    }
    let legacy_identity = map_id.is_none() && declared.is_none();
    let limit_id = identity(map_id.or(declared.as_deref()).unwrap_or(LEGACY_LIMIT_ID))?;
    let display_name = object
        .get("limitName")
        .and_then(Value::as_str)
        .filter(|name| identity(name).is_ok())
        .map(str::to_owned);
    let mut windows = Vec::new();
    for id in ["primary", "secondary"] {
        if let Some(value) = object.get(id) {
            if let Some(window) = window(value, id)? {
                windows.push(window);
            }
        }
    }
    // Discard credits, plan, account identifiers and every unrelated protocol field here.
    Ok(QuotaBucket {
        limit: QuotaLimit {
            limit_id,
            display_name,
        },
        windows,
        legacy_identity,
    })
}
fn multiple(object: &Map<String, Value>) -> Result<Option<QuotaBook>, ErrorCode> {
    let Some(value) = object.get("rateLimitsByLimitId") else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let map = value.as_object().ok_or(ErrorCode::QuotaProtocolError)?;
    if map.len() > MAX_QUOTA_BUCKETS {
        return Err(ErrorCode::QuotaProtocolError);
    }
    let mut book = BTreeMap::new();
    for (id, value) in map {
        let bucket = bucket(value, Some(id))?;
        book.insert(id.clone(), bucket);
    }
    Ok(Some(book))
}
pub fn parse_quota_read(result: &Value) -> Result<QuotaBook, ErrorCode> {
    let object = result.as_object().ok_or(ErrorCode::QuotaProtocolError)?;
    if let Some(book) = multiple(object)? {
        return Ok(book);
    }
    let value = object
        .get("rateLimits")
        .filter(|v| !v.is_null())
        .ok_or(ErrorCode::QuotaUnsupported)?;
    let bucket = bucket(value, None)?;
    Ok(BTreeMap::from([(bucket.limit.limit_id.clone(), bucket)]))
}
pub fn parse_quota_update(params: &Value) -> Result<QuotaUpdate, ErrorCode> {
    let object = params.as_object().ok_or(ErrorCode::QuotaProtocolError)?;
    if let Some(book) = multiple(object)? {
        return Ok(QuotaUpdate::Replace(book));
    }
    let bucket = bucket(
        object
            .get("rateLimits")
            .ok_or(ErrorCode::QuotaProtocolError)?,
        None,
    )?;
    Ok(if bucket.legacy_identity {
        QuotaUpdate::Unidentified(bucket)
    } else {
        QuotaUpdate::Bucket(bucket)
    })
}
/// Weekly is an actual duration, never the position or server field name of a window.
pub fn weekly_window(windows: &[QuotaWindow]) -> Option<&QuotaWindow> {
    let mut matches = windows.iter().filter(|w| w.duration_mins == Some(10080));
    let first = matches.next()?;
    if matches.next().is_none() {
        Some(first)
    } else {
        None
    }
}
pub fn shortest_window(windows: &[QuotaWindow]) -> Option<&QuotaWindow> {
    let shortest = windows
        .iter()
        .filter_map(|w| w.duration_mins.filter(|n| *n < 10080))
        .min()?;
    let mut matches = windows.iter().filter(|w| w.duration_mins == Some(shortest));
    let first = matches.next()?;
    if matches.next().is_none() {
        Some(first)
    } else {
        None
    }
}
