//! Pure domain logic. This crate must not depend on Tauri or a UI runtime.

pub const API_VERSION: u32 = 1;

pub mod accounting;
pub mod adapter;
pub mod calendar;
pub mod canonical;
pub mod domain;
pub mod error;
pub mod jobs;
pub mod numeric;
pub mod pricing;
pub mod protocol;
pub mod query;
pub mod reader;
pub mod scheduling;
pub mod selections;
pub mod sequence;
pub mod sources;

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
