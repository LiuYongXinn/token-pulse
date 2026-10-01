//! Pure domain logic. This crate must not depend on Tauri or a UI runtime.

pub const API_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceState {
    NotConfigured,
    NotImplemented,
    Ready,
}
