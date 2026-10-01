//! Pure domain logic. This crate must not depend on Tauri or a UI runtime.

pub const API_VERSION: u32 = 1;

pub mod domain;
pub mod error;
pub mod numeric;
pub mod protocol;

#[derive(
    Debug, Clone, Copy, serde::Serialize, serde::Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "snake_case")]
pub enum ServiceState {
    NotConfigured,
    NotImplemented,
    Ready,
    Error,
}
