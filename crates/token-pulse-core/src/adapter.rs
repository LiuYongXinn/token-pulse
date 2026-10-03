//! Explicit Codex rollout layouts; ephemeral JSON values never enter persistence.
use crate::{domain::*, error::ErrorCode};
use serde_json::{Map, Value};

pub enum AdaptedRecord {
    Observation(Box<NormalizedObservation>),
    Ignored,
    Diagnostic(AdapterDiagnostic),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterDiagnostic {
    pub position: PhysicalPosition,
    pub code: ErrorCode,
    pub reason: &'static str,
}
fn diagnostic(position: PhysicalPosition, code: ErrorCode, reason: &'static str) -> AdaptedRecord {
    AdaptedRecord::Diagnostic(AdapterDiagnostic {
        position,
        code,
        reason,
    })
}

pub fn adapt(
    bytes: &[u8],
    position: PhysicalPosition,
    context: &mut ReaderContext,
) -> AdaptedRecord {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return AdaptedRecord::Ignored;
    }
    // Fail closed across any intervening nonblank record, including malformed input.
    // The pending record survives batch/restart via the atomic ReaderContext checkpoint.
    let pending_request = context.pending_request_usage.take();
    let value: Value = match serde_json::from_slice(bytes) {
        Ok(v) => v,
        Err(_) => {
            return diagnostic(
                position,
                ErrorCode::UnsupportedFormat,
                "invalid_json_or_utf8",
            );
        }
    };
    let Some(root) = value.as_object() else {
        return diagnostic(position, ErrorCode::UnsupportedFormat, "record_not_object");
    };
    let Some(kind) = root.get("type").and_then(Value::as_str) else {
        return diagnostic(
            position,
            ErrorCode::UnsupportedFormat,
            "missing_record_type",
        );
    };
    let time = match timestamp(root.get("timestamp")) {
        Ok(t) => t,
        Err(_) => return diagnostic(position, ErrorCode::UnsupportedFormat, "invalid_timestamp"),
    };
    match kind {
        "session_meta" => {
            let Some(payload) = root.get("payload").and_then(Value::as_object) else {
                return diagnostic(
                    position,
                    ErrorCode::UnsupportedFormat,
                    "missing_session_payload",
                );
            };
            let Some(id) = payload
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty() && id.len() <= 256)
            else {
                return diagnostic(position, ErrorCode::UnsupportedFormat, "missing_session_id");
            };
            let metadata = match session_metadata(payload) {
                Ok(metadata) => metadata,
                Err(()) => {
                    return diagnostic(
                        position,
                        ErrorCode::UnsupportedFormat,
                        "invalid_session_metadata",
                    );
                }
            };
            context.provider_session_id = Some(id.into());
            context.session_key = Some(format!("codex:{id}"));
            context.metadata = metadata.clone();
            AdaptedRecord::Observation(Box::new(NormalizedObservation::SessionMetadata {
                physical_position: position,
                provider_session_id: id.into(),
                metadata,
                created_at_ms: time,
            }))
        }
        "turn_context" => {
            let Some(session) = context.session_key.clone() else {
                return diagnostic(
                    position,
                    ErrorCode::AmbiguousUsage,
                    "metadata_before_session_identity",
                );
            };
            let Some(payload) = root.get("payload").and_then(Value::as_object) else {
                return diagnostic(
                    position,
                    ErrorCode::UnsupportedFormat,
                    "missing_turn_payload",
                );
            };
            // A missing model/cwd keeps the previous effective metadata; explicit null clears it.
            let mut metadata = context.metadata.clone();
            for (key, target) in [
                ("model", &mut metadata.model),
                ("cwd", &mut metadata.cwd),
                ("turn_id", &mut metadata.turn_id),
                ("model_provider", &mut metadata.provider),
            ] {
                if payload.contains_key(key) {
                    *target = match string(payload, key) {
                        Ok(v) => v,
                        Err(()) => {
                            return diagnostic(
                                position,
                                ErrorCode::UnsupportedFormat,
                                "invalid_turn_metadata",
                            );
                        }
                    };
                }
            }
            context.metadata = metadata;
            AdaptedRecord::Observation(Box::new(NormalizedObservation::TurnMetadata {
                physical_position: position,
                session_key: session,
                metadata: context.metadata.clone(),
            }))
        }
        "token_usage_record" => {
            match request_usage(root.get("payload"), position.clone(), context) {
                Ok(evidence) => {
                    context.pending_request_usage = Some(Box::new(evidence));
                    // It is auxiliary proof, never a second consumption event.
                    AdaptedRecord::Ignored
                }
                Err(()) => diagnostic(
                    position,
                    ErrorCode::UnsupportedFormat,
                    "invalid_request_usage_record",
                ),
            }
        }
        "event_msg" => {
            let Some(payload) = root.get("payload").and_then(Value::as_object) else {
                return diagnostic(
                    position,
                    ErrorCode::UnsupportedFormat,
                    "missing_event_payload",
                );
            };
            let Some(subtype) = payload.get("type").and_then(Value::as_str) else {
                return diagnostic(position, ErrorCode::UnsupportedFormat, "missing_event_type");
            };
            if subtype != "token_count" {
                return if matches!(
                    subtype,
                    "user_message"
                        | "agent_message"
                        | "agent_reasoning"
                        | "task_started"
                        | "task_complete"
                        | "turn_aborted"
                        | "context_compacted"
                        | "item_completed"
                        | "warning"
                        | "error"
                ) {
                    AdaptedRecord::Ignored
                } else {
                    diagnostic(
                        position,
                        ErrorCode::UnsupportedFormat,
                        "unsupported_event_type",
                    )
                };
            }
            let Some(info) = payload.get("info") else {
                return diagnostic(position, ErrorCode::UnsupportedFormat, "missing_usage_info");
            };
            if info.is_null() {
                return AdaptedRecord::Ignored;
            }
            let Some(info) = info.as_object() else {
                return diagnostic(position, ErrorCode::UnsupportedFormat, "invalid_usage_info");
            };
            let last = match usage(info.get("last_token_usage")) {
                Ok(v) => v,
                Err(_) => {
                    return diagnostic(position, ErrorCode::InvalidUsage, "invalid_last_field");
                }
            };
            let cumulative = match usage(info.get("total_token_usage")) {
                Ok(v) => v,
                Err(_) => {
                    return diagnostic(
                        position,
                        ErrorCode::InvalidUsage,
                        "invalid_cumulative_field",
                    );
                }
            };
            if last.is_none() && cumulative.is_none() {
                return diagnostic(
                    position,
                    ErrorCode::UnsupportedFormat,
                    "usage_fields_missing",
                );
            }
            let Some(session) = context.session_key.clone() else {
                return diagnostic(
                    position,
                    ErrorCode::AmbiguousUsage,
                    "usage_without_session_identity",
                );
            };
            let window = match optional_integer(info.get("model_context_window")) {
                Ok(v) => v,
                Err(_) => {
                    return diagnostic(position, ErrorCode::InvalidUsage, "invalid_context_window");
                }
            };
            let request_usage = pending_request.filter(|evidence| {
                evidence.physical_position.file_generation_id == position.file_generation_id
                    && evidence.physical_position.byte_end <= position.byte_offset
                    && context.metadata.turn_id.as_ref() == Some(&evidence.turn_id)
                    && last == Some(evidence.usage)
                    && cumulative == Some(evidence.thread_usage)
            });
            let request_identity = request_usage.as_ref().and_then(|evidence| {
                context
                    .metadata
                    .provider
                    .as_ref()
                    .filter(|p| !p.is_empty() && p.len() <= 256 && !p.chars().any(char::is_control))
                    .map(|provider| VerifiedRequestIdentity {
                        namespace: format!("codex-responses:{provider}"),
                        request_id: evidence.response_id.clone(),
                    })
            });
            AdaptedRecord::Observation(Box::new(NormalizedObservation::Usage(UsageObservation {
                physical_position: position,
                session_key: session,
                event_time_ms: time,
                request_identity,
                request_usage,
                stream_hint: None,
                last,
                cumulative,
                effective_metadata: context.metadata.clone(),
                explicit_episode_start: false,
                model_context_window: window,
            })))
        }
        "response_item" => AdaptedRecord::Ignored,
        "compacted" => AdaptedRecord::Ignored,
        _ => diagnostic(
            position,
            ErrorCode::UnsupportedFormat,
            "unsupported_record_type",
        ),
    }
}

fn request_usage(
    value: Option<&Value>,
    physical_position: PhysicalPosition,
    context: &ReaderContext,
) -> Result<RequestUsageEvidence, ()> {
    let payload = value.and_then(Value::as_object).ok_or(())?;
    let identity = |key| -> Result<String, ()> {
        payload
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
            .map(str::to_owned)
            .ok_or(())
    };
    let thread = identity("thread_id")?;
    let turn_id = identity("turn_id")?;
    // Validate the documented layout, but retain only identities needed for association.
    identity("session_id")?;
    identity("root_turn_id")?;
    if context.provider_session_id.as_ref() != Some(&thread)
        // Collector can resolve the logical session key to a canonical alias.
        || context.session_key.as_ref().is_none_or(|key| key.is_empty())
        || context.metadata.turn_id.as_ref() != Some(&turn_id)
    {
        return Err(());
    }
    physical_position.validate().map_err(|_| ())?;
    let response_usage = usage(payload.get("usage"))?.ok_or(())?;
    let thread_usage = usage(payload.get("thread_token_usage"))?.ok_or(())?;
    let turn_usage = usage(payload.get("turn_token_usage"))?.ok_or(())?;
    for vector in [response_usage, thread_usage, turn_usage] {
        vector.validated_total().map_err(|_| ())?.ok_or(())?;
    }
    if response_usage.input_total.is_none() || response_usage.output_total.is_none() {
        return Err(());
    }
    Ok(RequestUsageEvidence {
        response_id: identity("response_id")?,
        turn_id,
        usage: response_usage,
        thread_usage,
        physical_position,
    })
}
fn string(map: &Map<String, Value>, key: &str) -> Result<Option<String>, ()> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.len() <= 32768 => Ok(Some(s.clone())),
        _ => Err(()),
    }
}
fn session_metadata(payload: &Map<String, Value>) -> Result<EffectiveMetadata, ()> {
    Ok(EffectiveMetadata {
        provider: string(payload, "model_provider")?,
        cwd: string(payload, "cwd")?,
        parent_provider_id: string(payload, "forked_from_id")?,
        ..EffectiveMetadata::default()
    })
}
fn timestamp(value: Option<&Value>) -> Result<Option<i64>, ()> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => chrono::DateTime::parse_from_rfc3339(s)
            .map(|dt| Some(dt.timestamp_millis()))
            .map_err(|_| ()),
        _ => Err(()),
    }
}
fn optional_integer(value: Option<&Value>) -> Result<Option<i64>, ()> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_i64().map(Some).ok_or(()),
    }
}
fn usage(value: Option<&Value>) -> Result<Option<UsageVector>, ()> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            let map = value.as_object().ok_or(())?;
            let allowed = [
                "input_tokens",
                "cached_input_tokens",
                "cache_write_input_tokens",
                "cache_write_tokens",
                "output_tokens",
                "reasoning_output_tokens",
                "total_tokens",
            ];
            if map.keys().any(|key| !allowed.contains(&key.as_str())) {
                return Err(());
            }
            Ok(Some(UsageVector {
                input_total: optional_integer(map.get("input_tokens"))?,
                cached_input: optional_integer(map.get("cached_input_tokens"))?,
                cache_write_input: cache_writes(map)?,
                output_total: optional_integer(map.get("output_tokens"))?,
                reasoning_output: optional_integer(map.get("reasoning_output_tokens"))?,
                reported_total: optional_integer(map.get("total_tokens"))?,
            }))
        }
    }
}

/// Rollout spelling and an explicitly supported compatibility spelling.
/// Two different values are ambiguous, including known versus null.
fn cache_writes(map: &Map<String, Value>) -> Result<Option<i64>, ()> {
    let canonical = optional_integer(map.get("cache_write_input_tokens"))?;
    let alias = optional_integer(map.get("cache_write_tokens"))?;
    match (
        map.contains_key("cache_write_input_tokens"),
        map.contains_key("cache_write_tokens"),
    ) {
        (true, true) if canonical != alias => Err(()),
        (true, _) => Ok(canonical),
        (_, true) => Ok(alias),
        _ => Ok(None),
    }
}
