//! Test-only child, not a product executable or installer resource.
#[cfg(windows)]
fn main() {
    use std::io::Read;
    use token_pulse_core::notify::parse_codex_notification;
    use token_pulse_integration::notify_channel::{NotifyCapability, windows::send_hint};
    let mut bytes = Vec::new();
    if std::io::stdin().take(4097).read_to_end(&mut bytes).is_err() || bytes.len() > 4096 {
        std::process::exit(2);
    }
    let capability: NotifyCapability = match serde_json::from_slice(&bytes) {
        Ok(cap) => cap,
        Err(_) => std::process::exit(2),
    };
    let hint = parse_codex_notification(
        br#"{"type":"agent-turn-complete","thread-id":"fixture-thread","turn-id":"fixture-turn"}"#,
    )
    .unwrap()
    .unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    if runtime.block_on(send_hint(&capability, &hint)).is_err() {
        std::process::exit(3);
    }
}
#[cfg(not(windows))]
fn main() {}
