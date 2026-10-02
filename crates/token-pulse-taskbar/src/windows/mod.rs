//! Owned local Windows transport. No shell, user-log or credential access.
mod canvas;
pub mod control;
mod details_window;
pub mod guardian;
mod layout;
mod menu;
mod ownership;
mod process;
pub mod render;
mod security;
pub mod topology;
pub mod transport;
pub use layout::RestoreDisposition;
pub use layout::recover_terminated_host;
pub use process::HostProcess;
pub use transport::{Startup, TransportError, run_host};
