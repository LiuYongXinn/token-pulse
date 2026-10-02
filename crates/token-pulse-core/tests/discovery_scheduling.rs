use std::{
    fs,
    time::{Duration, Instant},
};
use token_pulse_core::{scheduling::*, sources::*};

#[test]
fn discovery_is_resumable_and_does_not_read_codex_auth_or_other_jsonl_trees() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("sessions/2026/10")).unwrap();
    fs::create_dir_all(root.join("archived_sessions")).unwrap();
    fs::create_dir_all(root.join("unrelated")).unwrap();
    fs::write(root.join("auth.json"), b"SYNTHETIC_AUTH_DO_NOT_OPEN").unwrap();
    fs::write(root.join("unrelated/secret.jsonl"), b"excluded").unwrap();
    fs::write(root.join("sessions/config.json"), b"excluded").unwrap();
    for i in 0..100 {
        fs::write(root.join(format!("sessions/2026/10/{i}.jsonl")), b"{}\n").unwrap();
    }
    fs::write(root.join("archived_sessions/old.JSONL"), b"{}\n").unwrap();
    let mut scan = SourceScanner::new(root, SourceOrigin::Custom).unwrap();
    let first: Vec<_> = scan.by_ref().take(10).collect();
    assert_eq!(first.len(), 10);
    let remainder: Vec<_> = scan.collect();
    assert_eq!(remainder.len(), 91);
    for file in first.into_iter().chain(remainder) {
        let file = file.unwrap();
        assert_eq!(file.size, 3);
        assert!(
            file.path.starts_with(root.join("sessions"))
                || file.path.starts_with(root.join("archived_sessions"))
        );
    }
    assert_eq!(
        fs::read(root.join("auth.json")).unwrap(),
        b"SYNTHETIC_AUTH_DO_NOT_OPEN"
    );
}
#[test]
fn missing_directory_does_not_stop_the_readable_tree() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("sessions")).unwrap();
    fs::write(dir.path().join("sessions/a.jsonl"), b"{}\n").unwrap();
    let all: Vec<_> = SourceScanner::new(dir.path(), SourceOrigin::Custom)
        .unwrap()
        .collect();
    assert_eq!(all.iter().filter(|f| f.is_ok()).count(), 1);
    assert_eq!(
        all.iter()
            .filter(|f| f
                .as_ref()
                .is_err_and(|i| i.readability == SourceReadability::AwaitingDirectory))
            .count(),
        1
    );
}
#[test]
fn candidate_order_deduplicates_explicit_environment_and_default_paths() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join(".codex");
    fs::create_dir(&home).unwrap();
    let candidates = local_candidates(
        std::slice::from_ref(&home),
        Some(home.clone()),
        Some(dir.path().into()),
    );
    assert_eq!(
        candidates,
        vec![SourceCandidate {
            root: home,
            origin: SourceOrigin::Custom
        }]
    );
    assert!(local_candidates(&[], Some("relative".into()), None).is_empty());
}
#[cfg(windows)]
#[test]
fn native_junction_is_not_followed_and_is_reported_as_a_scan_gap() {
    use std::os::windows::process::CommandExt;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("home");
    let target = dir.path().join("outside");
    fs::create_dir_all(root.join("sessions")).unwrap();
    fs::create_dir(root.join("archived_sessions")).unwrap();
    fs::create_dir(&target).unwrap();
    fs::write(target.join("excluded.jsonl"), b"SYNTHETIC_EXCLUDED\n").unwrap();
    let link = root.join("sessions").join("linked");
    let output=std::process::Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-Command","New-Item -ItemType Junction -Path $env:TOKENPULSE_TEST_LINK -Target $env:TOKENPULSE_TEST_TARGET -ErrorAction Stop | Out-Null"])
        .env("TOKENPULSE_TEST_LINK",&link).env("TOKENPULSE_TEST_TARGET",&target).creation_flags(0x08000000).output().unwrap();
    assert!(
        output.status.success(),
        "junction fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let entries: Vec<_> = SourceScanner::new(&root, SourceOrigin::Custom)
        .unwrap()
        .collect();
    // Remove the junction itself before assertions / temporary-tree cleanup. Never delete its target.
    fs::remove_dir(&link).unwrap();
    assert!(entries.iter().all(|entry| entry.is_err()));
    assert!(entries.iter().any(|entry| entry.as_ref().is_err_and(
        |issue| issue.path == link && issue.readability == SourceReadability::Unreadable
    )));
    assert_eq!(
        fs::read(target.join("excluded.jsonl")).unwrap(),
        b"SYNTHETIC_EXCLUDED\n"
    );
}
#[cfg(windows)]
#[test]
fn wsl_requires_explicit_origin_and_local_sources_reject_network_and_device_roots() {
    use std::path::Path;
    let wsl = Path::new(r"\\wsl.localhost\Ubuntu\home\synthetic\.codex");
    assert!(validate_root(wsl, SourceOrigin::Wsl).is_ok());
    assert!(validate_root(wsl, SourceOrigin::Custom).is_err());
    for path in [
        r"\\server\share\home",
        r"\\?\UNC\server\share",
        r"\\?\unc\server\share",
        r"\\.\pipe\name",
        r"\\?\GLOBALROOT\Device\HarddiskVolume1",
        r"E:\home\..\other",
    ] {
        assert!(
            validate_root(Path::new(path), SourceOrigin::Custom).is_err(),
            "{path}"
        );
    }
    assert!(validate_root(Path::new(r"\\wsl.localhost\..\home"), SourceOrigin::Wsl).is_err());
    assert!(validate_root(Path::new(r"E:\synthetic\.codex"), SourceOrigin::Custom).is_ok());
    assert!(local_candidates(&[], Some(wsl.into()), None).is_empty());
}
#[test]
fn live_work_coalesces_and_preempts_import_without_losing_event_overflow() {
    let now = Instant::now();
    let mut queue = WorkQueue::default();
    queue.enqueue(
        "history".into(),
        WorkPriority::Historical,
        now,
        Duration::ZERO,
    );
    queue.enqueue(
        "active".into(),
        WorkPriority::Live,
        now,
        Duration::from_millis(300),
    );
    queue.enqueue(
        "active".into(),
        WorkPriority::Live,
        now + Duration::from_millis(100),
        Duration::from_millis(300),
    );
    assert_eq!(queue.len(), 2);
    assert_eq!(
        queue.pop_ready(now + Duration::from_millis(400)).as_deref(),
        Some("active")
    );
    assert_eq!(
        queue.pop_ready(now + Duration::from_millis(400)).as_deref(),
        Some("history")
    );
    for i in 0..MAX_PENDING_FILES {
        assert!(queue.enqueue(i.to_string(), WorkPriority::Historical, now, Duration::ZERO));
    }
    assert!(!queue.enqueue("overflow".into(), WorkPriority::Live, now, Duration::ZERO));
    assert_eq!(queue.len(), MAX_PENDING_FILES);
    assert!(queue.take_reconcile_required());
    assert!(!queue.take_reconcile_required());
}
#[test]
fn continual_watcher_events_cannot_postpone_active_file_forever() {
    let now = Instant::now();
    let mut queue = WorkQueue::default();
    for millis in (0..2500).step_by(100) {
        queue.enqueue(
            "active".into(),
            WorkPriority::Live,
            now + Duration::from_millis(millis),
            Duration::from_millis(300),
        );
    }
    assert_eq!(
        queue.pop_ready(now + Duration::from_secs(2)).as_deref(),
        Some("active")
    );
}
