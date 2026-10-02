//! Owned, restricted Codex App Server connection. No log or credential-file adapter.
mod framing;
mod process;
mod protocol;
pub use process::{NativeService, StdioSession};
pub use protocol::{AccountRequest, ProtocolEvent, RpcReply, RpcToken};
pub const MAX_PROTOCOL_LINE_BYTES: usize = 1_048_576;
pub const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
