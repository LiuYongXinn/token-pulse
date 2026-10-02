//! Owned local Windows transport. No shell, user-log or credential access.
pub mod control;
mod process;
mod security;
pub mod topology;
pub mod transport;
pub use process::HostProcess;
pub use transport::{Startup, TransportError, run_host};
