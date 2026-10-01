//! Process-local query leases and integrity-protected, non-sensitive cursors.
pub mod cursor;
mod service;
pub use service::{LeaseHandle, LeaseService};
