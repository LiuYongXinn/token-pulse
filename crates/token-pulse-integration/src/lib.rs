//! Controlled optional system integration; independent of Tauri, source logs and authentication.
pub mod notify_channel;
pub mod notify_config;
pub mod notify_invocation;
#[cfg(windows)]
pub mod notify_manager;
#[cfg(windows)]
pub mod notify_original;
pub mod notify_registry;
#[cfg(windows)]
pub mod notify_service;
