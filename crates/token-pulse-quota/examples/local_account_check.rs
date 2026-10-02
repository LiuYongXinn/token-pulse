//! Explicit opt-in acceptance of an existing local Codex login. Never a CI/default test.
use std::{
    path::PathBuf,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use token_pulse_core::protocol::QuotaState;
use token_pulse_quota::{NativeService, service::AccountQuotaService};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let action = args.next();
    if action.as_deref() == Some(std::ffi::OsStr::new("--inspect-local")) {
        if args.next().is_some() {
            return Err("unexpected inspection argument".into());
        }
        let target = token_pulse_quota::detect_local_service(None)?;
        println!(
            "{}",
            serde_json::json!({"probe":"local-service-detection","native_program_found":true,"existing_home_found":target.home_path.is_some()})
        );
        return Ok(());
    }
    if action.as_deref() != Some(std::ffi::OsStr::new("--read-existing-account")) {
        return Err(
            "explicit --read-existing-account and native executable / Home paths required".into(),
        );
    }
    let executable = PathBuf::from(args.next().ok_or("native executable required")?);
    let home = PathBuf::from(args.next().ok_or("existing Codex Home required")?);
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let target = NativeService::inspect(&executable, Some(&home))?;
    let native = NativeService::from_target(&target)?;
    let service = AccountQuotaService::start("local-account-check", Arc::new(|_| {}))?;
    let epoch = service.snapshot()?.connection_epoch;
    service.connect(native, &epoch)?;
    let deadline = Instant::now() + Duration::from_secs(20);
    let snapshot = loop {
        let snapshot = service.snapshot()?;
        if matches!(
            snapshot.state,
            QuotaState::Ready | QuotaState::AuthorizationRequired | QuotaState::Unsupported
        ) || Instant::now() >= deadline
        {
            break snapshot;
        }
        thread::sleep(Duration::from_millis(50));
    };
    // Only sanitized quota fields. No account identity, paths, auth or raw server messages.
    println!(
        "{}",
        serde_json::json!({
            "probe":"local-existing-account", "state": snapshot.state,
            "fetched_at_ms":snapshot.fetched_at_ms, "error_code":snapshot.error_code,
            "windows":snapshot.windows.iter().map(|window| serde_json::json!({
                "duration_mins":window.duration_mins, "remaining_percent":window.remaining_percent,
                "resets_at_ms":window.resets_at_ms
            })).collect::<Vec<_>>()
        })
    );
    service.disconnect(&snapshot.connection_epoch)?;
    service.shutdown();
    if matches!(snapshot.state, QuotaState::Ready) {
        Ok(())
    } else {
        Err("existing local account quota is not available; no login attempted".into())
    }
}
