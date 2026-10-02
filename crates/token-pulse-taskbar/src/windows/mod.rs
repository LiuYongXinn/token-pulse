//! Owned local Windows transport. No shell, user-log or credential access.
mod process;
mod security;
pub mod transport;
pub use process::HostProcess;
pub use transport::{Startup, TransportError, run_host};
