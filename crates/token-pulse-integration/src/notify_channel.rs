//! Authenticated bounded wake hints. No command text, source paths or consumption facts.
use serde::{Deserialize, Serialize};
use token_pulse_core::notify::{NotifyWakeHint, parse_codex_notification};

#[cfg(windows)]
pub mod windows;
const VERSION: u32 = 1;
pub const MAX_WAKE_FRAME_BYTES: usize = 1024;

/// The random capability is never included in Debug or errors. Persist only in user-owned state.
#[derive(Clone, Serialize, Deserialize)]
#[serde(try_from = "CapabilityWire")]
pub struct NotifyCapability {
    registration_id: String,
    nonce: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityWire {
    registration_id: String,
    nonce: String,
}
impl TryFrom<CapabilityWire> for NotifyCapability {
    type Error = WakeError;
    fn try_from(wire: CapabilityWire) -> Result<Self, Self::Error> {
        if !hex(&wire.registration_id, 32) || !hex(&wire.nonce, 64) {
            return Err(WakeError::InvalidCapability);
        }
        Ok(Self {
            registration_id: wire.registration_id,
            nonce: wire.nonce,
        })
    }
}
impl Default for NotifyCapability {
    fn default() -> Self {
        Self::new()
    }
}
impl NotifyCapability {
    pub fn new() -> Self {
        Self {
            registration_id: uuid::Uuid::new_v4().simple().to_string(),
            nonce: format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            ),
        }
    }
    pub fn registration_id(&self) -> &str {
        &self.registration_id
    }
    pub(crate) fn matches(&self, other: &Self) -> bool {
        self.registration_id == other.registration_id && same_nonce(&self.nonce, &other.nonce)
    }

    pub fn encode_hint(&self, hint: &NotifyWakeHint) -> Result<Vec<u8>, WakeError> {
        let bytes = serde_json::to_vec(&WakeFrame {
            version: VERSION,
            registration_id: self.registration_id.clone(),
            nonce: self.nonce.clone(),
            thread_id: hint.thread_id().to_owned(),
            turn_id: hint.turn_id().map(str::to_owned),
        })
        .map_err(|_| WakeError::InvalidFrame)?;
        if bytes.len() > MAX_WAKE_FRAME_BYTES {
            return Err(WakeError::TooLarge);
        }
        Ok(bytes)
    }
    pub fn decode_hint(&self, bytes: &[u8]) -> Result<NotifyWakeHint, WakeError> {
        if bytes.len() > MAX_WAKE_FRAME_BYTES {
            return Err(WakeError::TooLarge);
        }
        let frame: WakeFrame =
            serde_json::from_slice(bytes).map_err(|_| WakeError::InvalidFrame)?;
        if frame.version != VERSION {
            return Err(WakeError::VersionMismatch);
        }
        if frame.registration_id != self.registration_id || !same_nonce(&frame.nonce, &self.nonce) {
            return Err(WakeError::Unauthorized);
        }
        // Reuse the only constructor of the validated hint, with a tiny allowlisted value.
        let payload = serde_json::to_vec(&serde_json::json!({"type":"agent-turn-complete",
            "thread-id":frame.thread_id, "turn-id":frame.turn_id}))
        .map_err(|_| WakeError::InvalidFrame)?;
        parse_codex_notification(&payload)
            .map_err(|_| WakeError::InvalidFrame)?
            .ok_or(WakeError::InvalidFrame)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WakeFrame {
    version: u32,
    registration_id: String,
    nonce: String,
    thread_id: String,
    #[serde(deserialize_with = "required_nullable_id")]
    turn_id: Option<String>,
}
fn required_nullable_id<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(d)
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WakeError {
    TooLarge,
    InvalidFrame,
    VersionMismatch,
    Unauthorized,
    InvalidCapability,
    Io,
    Timeout,
}
impl std::fmt::Display for WakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::TooLarge => "notify_wake_too_large",
            Self::InvalidFrame => "notify_wake_invalid",
            Self::VersionMismatch => "notify_wake_version_mismatch",
            Self::Unauthorized => "notify_wake_unauthorized",
            Self::InvalidCapability => "notify_wake_invalid_capability",
            Self::Io => "notify_wake_io",
            Self::Timeout => "notify_wake_timeout",
        })
    }
}
impl std::error::Error for WakeError {}
fn hex(text: &str, length: usize) -> bool {
    text.len() == length
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn same_nonce(a: &str, b: &str) -> bool {
    if a.len() != 64 || b.len() != 64 {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}
