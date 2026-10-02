//! Explicit opt-in acceptance of an existing local Codex login. Never a CI/default test.
use std::{
    path::PathBuf,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use token_pulse_core::protocol::QuotaState;
use token_pulse_quota::{
    NativeService,
    service::{AccountQuotaService, DisplayEntry},
};

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
    let observe =
        action.as_deref() == Some(std::ffi::OsStr::new("--observe-local-existing-account"));
    if !observe && action.as_deref() != Some(std::ffi::OsStr::new("--read-existing-account")) {
        return Err(
            "explicit --read-existing-account with paths, or --observe-local-existing-account required".into(),
        );
    }
    let target = if observe {
        token_pulse_quota::detect_local_service(None)?
    } else {
        let executable = PathBuf::from(args.next().ok_or("native executable required")?);
        let home = PathBuf::from(args.next().ok_or("existing Codex Home required")?);
        NativeService::inspect(&executable, Some(&home))?
    };
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let native = NativeService::from_target(&target)?;
    let service = AccountQuotaService::start("local-account-check", Arc::new(|_| {}))?;
    if observe {
        service.set_visible(DisplayEntry::Main, true);
    }
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
    if observe {
        if !matches!(snapshot.state, QuotaState::Ready) {
            println!(
                "{}",
                serde_json::json!({"probe":"local-existing-account-observation","stage":"initial_unavailable","state":snapshot.state,"error_code":snapshot.error_code})
            );
            service.disconnect(&snapshot.connection_epoch)?;
            service.shutdown();
            return Err("existing account unavailable; no login attempted".into());
        }
        let result = observe_reads(&service, &snapshot);
        let current = service.snapshot()?;
        service.disconnect(&current.connection_epoch)?;
        service.shutdown();
        return result;
    }
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

/// Observe the production timer twice without forced refreshes, clock changes or model rounds.
/// Changed attempt and fetched times distinguish quota reads from display-only invalidations.
fn observe_reads(
    service: &AccountQuotaService,
    initial: &token_pulse_core::protocol::QuotaSnapshot,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut previous_attempt = initial
        .last_attempt_at_ms
        .ok_or("initial attempt missing")?;
    let mut previous_fetch = initial.fetched_at_ms.ok_or("initial quota proof missing")?;
    println!(
        "{}",
        serde_json::json!({"probe":"local-existing-account-observation","stage":"initial_ready","actual_windows":initial.windows.len(),"percentages_available":!initial.windows.is_empty() && initial.windows.iter().all(|w|w.remaining_percent.is_some())})
    );
    for round in 1..=2 {
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            let snapshot = service.snapshot()?;
            if snapshot.connection_epoch != initial.connection_epoch {
                return Err(
                    "account connection changed during observation; previous proof discarded"
                        .into(),
                );
            }
            if matches!(
                snapshot.state,
                QuotaState::AuthorizationRequired
                    | QuotaState::Unsupported
                    | QuotaState::Disconnected
            ) {
                return Err("account capability lost during observation".into());
            }
            if matches!(snapshot.state, QuotaState::Ready)
                && snapshot
                    .last_attempt_at_ms
                    .is_some_and(|at| at > previous_attempt)
                && snapshot.fetched_at_ms.is_some_and(|at| at > previous_fetch)
            {
                // A read must have begun after the prior successful proof; an old in-flight read
                // or notification alone cannot satisfy this acceptance condition.
                let attempt = snapshot.last_attempt_at_ms.ok_or("read attempt missing")?;
                if attempt >= previous_fetch {
                    previous_attempt = attempt;
                    previous_fetch = snapshot.fetched_at_ms.ok_or("read proof missing")?;
                    println!(
                        "{}",
                        serde_json::json!({"probe":"local-existing-account-observation","stage":"subsequent_read_ready","round":round,"actual_windows":snapshot.windows.len(),"percentages_available":!snapshot.windows.is_empty() && snapshot.windows.iter().all(|w|w.remaining_percent.is_some()),"forced_refresh":false,"same_connection":true})
                    );
                    break;
                }
            }
            if Instant::now() >= deadline {
                return Err(
                    "ordinary read was not observed within the bounded acceptance window".into(),
                );
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
    Ok(())
}
