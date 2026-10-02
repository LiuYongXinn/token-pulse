//! Optional Codex notify input is only a wake hint. It cannot create usage or prices.
//! Unknown fields (including all conversation text, cwd and credentials) are skipped by serde;
//! neither errors nor the resulting hint retain the raw payload. No file or command access.
use serde::{Deserialize, Serialize};

/// Internal transport boundary, distinct from the statistics IPC request frame.
pub const MAX_NOTIFY_PAYLOAD_BYTES: usize = 64 * 1024;
const MAX_ID_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyParseError {
    PayloadTooLarge,
    MalformedPayload,
    InvalidIdentity,
}
impl std::fmt::Display for NotifyParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::PayloadTooLarge => "NOTIFY_PAYLOAD_TOO_LARGE",
            Self::MalformedPayload => "NOTIFY_PAYLOAD_INVALID",
            Self::InvalidIdentity => "NOTIFY_IDENTITY_INVALID",
        })
    }
}
impl std::error::Error for NotifyParseError {}

/// This type can only be constructed through the validated allowlist reader.
/// It contains no source path, conversation text, raw JSON or executable arguments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NotifyWakeHint {
    thread_id: String,
    turn_id: Option<String>,
}
impl NotifyWakeHint {
    pub fn thread_id(&self) -> &str {
        &self.thread_id
    }
    pub fn turn_id(&self) -> Option<&str> {
        self.turn_id.as_deref()
    }
}

#[derive(Deserialize)]
struct Notification {
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "thread-id")]
    thread_id: Option<String>,
    #[serde(rename = "turn-id")]
    turn_id: Option<String>,
}
fn valid_id(value: &str) -> bool {
    // Provider identifiers are opaque. Constrain their transport representation, never use
    // them as paths/commands; unfamiliar formats can still be collected by ordinary scans.
    value.len() <= MAX_ID_BYTES
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

pub fn parse_codex_notification(
    payload: &[u8],
) -> Result<Option<NotifyWakeHint>, NotifyParseError> {
    if payload.len() > MAX_NOTIFY_PAYLOAD_BYTES {
        return Err(NotifyParseError::PayloadTooLarge);
    }
    // Deserialize directly into the three allowed fields instead of a Value or raw payload
    // container. serde's IgnoredAny discards unknown nested content while checking JSON syntax.
    let notification: Notification =
        serde_json::from_slice(payload).map_err(|_| NotifyParseError::MalformedPayload)?;
    if notification.kind != "agent-turn-complete" {
        return Ok(None);
    }
    let thread_id = notification
        .thread_id
        .ok_or(NotifyParseError::InvalidIdentity)?;
    if !valid_id(&thread_id)
        || notification
            .turn_id
            .as_deref()
            .is_some_and(|id| !valid_id(id))
    {
        return Err(NotifyParseError::InvalidIdentity);
    }
    Ok(Some(NotifyWakeHint {
        thread_id,
        turn_id: notification.turn_id,
    }))
}
