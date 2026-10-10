#![cfg(feature = "test-fixture")]
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tempfile::TempDir;
use token_pulse_core::{
    error::ErrorCode,
    protocol::{QuotaSnapshot, QuotaState},
    quota::QuotaRefreshDecision,
};
use token_pulse_quota::{
    NativeService,
    service::{AccountQuotaService, DisplayEntry},
};
fn spec(mode: &str) -> (TempDir, NativeService) {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join("fixture-mode"), mode).unwrap();
    let spec = NativeService::new(
        Path::new(env!("CARGO_BIN_EXE_quota-fixture")),
        Some(home.path()),
    )
    .unwrap();
    (home, spec)
}
fn start() -> (AccountQuotaService, Arc<AtomicU64>) {
    let changes = Arc::new(AtomicU64::new(0));
    let counter = changes.clone();
    (
        AccountQuotaService::start(
            "service-fixture",
            Arc::new(move |_| {
                counter.fetch_add(1, Ordering::Relaxed);
            }),
        )
        .unwrap(),
        changes,
    )
}
fn wait(
    service: &AccountQuotaService,
    predicate: impl Fn(&QuotaSnapshot) -> bool,
) -> QuotaSnapshot {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let snapshot = service.snapshot().unwrap();
        if predicate(&snapshot) {
            return snapshot;
        }
        assert!(Instant::now() < deadline, "snapshot: {snapshot:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn default_is_disconnected_and_shutdown_does_not_launch_a_service() {
    let (service, changes) = start();
    let initial = service.snapshot().unwrap();
    assert!(matches!(initial.state, QuotaState::Disconnected));
    assert!(initial.windows.is_empty());
    assert_eq!(service.refresh().err(), Some(ErrorCode::QuotaDisconnected));
    std::thread::sleep(Duration::from_millis(120));
    assert_eq!(changes.load(Ordering::Relaxed), 0);
    assert_eq!(
        service.snapshot().unwrap().quota_revision,
        initial.quota_revision
    );
    service.shutdown();
    service.shutdown();
    assert!(matches!(
        service.snapshot().unwrap().state,
        QuotaState::Disconnected
    ));
    assert_eq!(
        service.disconnect(&initial.connection_epoch).err(),
        Some(ErrorCode::QuotaDisconnected)
    );
}
#[test]
fn invalid_saved_target_reports_real_error_with_new_epoch_and_no_automatic_retry() {
    let (service, _) = start();
    let initial = service.snapshot().unwrap();
    let failed = service
        .report_unavailable(&initial.connection_epoch)
        .unwrap();
    assert!(matches!(failed.state, QuotaState::Error));
    assert_ne!(failed.connection_epoch, initial.connection_epoch);
    assert_eq!(
        failed.error_code.as_deref(),
        Some("QUOTA_SERVICE_UNAVAILABLE")
    );
    assert!(failed.windows.is_empty());
    assert!(failed.fetched_at_ms.is_none());
    assert_eq!(
        service.report_unavailable(&initial.connection_epoch).err(),
        Some(ErrorCode::RevisionConflict)
    );
    std::thread::sleep(Duration::from_millis(120));
    assert_eq!(
        service.snapshot().unwrap().connection_epoch,
        failed.connection_epoch
    );
    service.shutdown();
}
#[test]
fn owner_drives_handshake_account_query_and_independent_bucket_selection() {
    let (_home, native) = spec("service");
    let (service, changes) = start();
    let original = service.snapshot().unwrap();
    let connecting = service.connect(native, &original.connection_epoch).unwrap();
    assert!(matches!(connecting.state, QuotaState::Connecting));
    let ready = wait(&service, |s| matches!(s.state, QuotaState::Ready));
    assert_eq!(ready.selected_limit_id.as_deref(), Some("codex"));
    assert_eq!(ready.windows[0].remaining_percent, Some(75.0));
    assert_eq!(ready.available_limits.len(), 2);
    let selected = service
        .select_limit(
            "other",
            &ready.connection_epoch,
            ready.quota_revision.clone(),
        )
        .unwrap();
    assert_eq!(selected.windows[0].remaining_percent, Some(20.0));
    assert_eq!(selected.windows[0].duration_mins, Some(10080));
    assert_eq!(
        service
            .select_limit("codex", &ready.connection_epoch, ready.quota_revision)
            .err(),
        Some(ErrorCode::RevisionConflict)
    );
    assert_eq!(
        service.disconnect(&original.connection_epoch).err(),
        Some(ErrorCode::RevisionConflict)
    );
    let limited = service.refresh().unwrap();
    assert!(matches!(
        limited.decision,
        QuotaRefreshDecision::RateLimited { .. }
    ));
    assert_eq!(limited.snapshot.fetched_at_ms, selected.fetched_at_ms);
    let disconnected = service.disconnect(&selected.connection_epoch).unwrap();
    assert!(matches!(disconnected.state, QuotaState::Disconnected));
    assert!(disconnected.windows.is_empty());
    assert_eq!(disconnected.fetched_at_ms, None);
    assert!(changes.load(Ordering::Relaxed) >= 5);
}
#[test]
fn authorization_and_actual_quota_capability_errors_never_fake_quota() {
    for (mode, state, code) in [
        (
            "auth-required",
            QuotaState::AuthorizationRequired,
            ErrorCode::QuotaAuthRequired,
        ),
        (
            "unsupported-account",
            QuotaState::Unsupported,
            ErrorCode::QuotaUnsupported,
        ),
    ] {
        let (_home, native) = spec(mode);
        let (service, _) = start();
        let initial = service.snapshot().unwrap();
        service.connect(native, &initial.connection_epoch).unwrap();
        let snapshot = wait(&service, |s| {
            matches!(
                s.state,
                QuotaState::AuthorizationRequired | QuotaState::Unsupported
            )
        });
        assert_eq!(
            std::mem::discriminant(&snapshot.state),
            std::mem::discriminant(&state)
        );
        assert!(snapshot.windows.is_empty());
        assert_eq!(snapshot.fetched_at_ms, None);
        assert_eq!(snapshot.error_code, Some(code.to_string()));
        assert_eq!(service.refresh().err(), Some(code));
    }
}
#[test]
fn future_account_type_uses_actual_rate_limits_capability() {
    let (_home, native) = spec("future-account");
    let (service, _) = start();
    let initial = service.snapshot().unwrap();
    service.connect(native, &initial.connection_epoch).unwrap();
    let snapshot = wait(&service, |s| matches!(s.state, QuotaState::Ready));
    assert!(!snapshot.windows.is_empty());
    assert!(snapshot.fetched_at_ms.is_some());
}
#[test]
fn account_changed_drops_old_values_and_unproven_new_notifications() {
    let (_home, native) = spec("switch-account");
    let (service, _) = start();
    let initial = service.snapshot().unwrap();
    let connecting = service.connect(native, &initial.connection_epoch).unwrap();
    let next = wait(&service, |s| {
        matches!(s.state, QuotaState::AuthorizationRequired)
    });
    assert_ne!(next.connection_epoch, connecting.connection_epoch);
    assert!(next.windows.is_empty());
    assert!(next.available_limits.is_empty());
    assert_eq!(next.fetched_at_ms, None);
    assert_eq!(
        service.disconnect(&connecting.connection_epoch).err(),
        Some(ErrorCode::RevisionConflict)
    );
}
#[test]
fn unexpected_process_exit_keeps_known_values_explicitly_stale_until_reconnect() {
    let (_home, native) = spec("exit-after-read");
    let (service, _) = start();
    let initial = service.snapshot().unwrap();
    service.connect(native, &initial.connection_epoch).unwrap();
    let ready = wait(&service, |s| matches!(s.state, QuotaState::Ready));
    let stale = wait(&service, |s| matches!(s.state, QuotaState::Stale));
    assert_eq!(stale.connection_epoch, ready.connection_epoch);
    assert_eq!(stale.fetched_at_ms, ready.fetched_at_ms);
    assert_eq!(stale.windows[0].remaining_percent, Some(50.0));
    assert_eq!(
        stale.error_code.as_deref(),
        Some("QUOTA_SERVICE_UNAVAILABLE")
    );
    service.disconnect(&stale.connection_epoch).unwrap();
    let epoch = service.snapshot().unwrap().connection_epoch;
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(service.snapshot().unwrap().connection_epoch, epoch);
}
#[test]
fn resume_defers_refresh_during_suspend_and_preserves_real_values() {
    let (_home, native) = spec("service");
    let (service, _) = start();
    service.set_visible(DisplayEntry::Main, true);
    let initial = service.snapshot().unwrap();
    service.connect(native, &initial.connection_epoch).unwrap();
    let ready = wait(&service, |s| matches!(s.state, QuotaState::Ready));
    service.suspend();
    assert_eq!(
        service.refresh().err(),
        Some(ErrorCode::QuotaServiceUnavailable)
    );
    assert_eq!(
        service.snapshot().unwrap().windows[0].remaining_percent,
        Some(75.0)
    );
    service.set_visible(DisplayEntry::Mini, true);
    service.set_visible(DisplayEntry::Main, false);
    service.resume();
    let deadline = Instant::now() + Duration::from_secs(7);
    loop {
        let snapshot = service.snapshot().unwrap();
        if snapshot.fetched_at_ms != ready.fetched_at_ms {
            assert_eq!(snapshot.windows[0].remaining_percent, Some(75.0));
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn failed_handshake_retries_after_backoff_with_new_epoch_and_fresh_identity_proof() {
    let (home, native) = spec("bad-init");
    let (service, _) = start();
    let initial = service.snapshot().unwrap();
    service.connect(native, &initial.connection_epoch).unwrap();
    let failed = wait(&service, |s| matches!(s.state, QuotaState::Error));
    assert_eq!(failed.error_code.as_deref(), Some("QUOTA_PROTOCOL_ERROR"));
    assert!(failed.windows.is_empty());
    std::fs::write(home.path().join("fixture-mode"), "service").unwrap();
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(
        service.snapshot().unwrap().connection_epoch,
        failed.connection_epoch
    );
    let deadline = Instant::now() + Duration::from_secs(7);
    loop {
        let snapshot = service.snapshot().unwrap();
        if matches!(snapshot.state, QuotaState::Ready) {
            assert_ne!(snapshot.connection_epoch, failed.connection_epoch);
            assert_eq!(snapshot.windows[0].remaining_percent, Some(75.0));
            assert_eq!(snapshot.error_code, None);
            break;
        }
        assert!(Instant::now() < deadline, "{snapshot:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}
