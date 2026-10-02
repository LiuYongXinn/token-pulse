#![cfg(feature = "test-fixture")]
use std::{path::Path, time::Duration};
use tempfile::TempDir;
use token_pulse_core::{
    error::ErrorCode,
    quota::{AccountAvailability, QuotaUpdate},
};
use token_pulse_quota::{AccountRequest, NativeService, ProtocolEvent, RpcReply, StdioSession};

fn launch(mode: &str) -> (TempDir, StdioSession) {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join("fixture-mode"), mode).unwrap();
    let spec = NativeService::new(
        Path::new(env!("CARGO_BIN_EXE_quota-fixture")),
        Some(home.path()),
    )
    .unwrap();
    let session = StdioSession::launch(&spec, "synthetic-transport-1").unwrap();
    (home, session)
}
fn ready(session: &mut StdioSession) {
    assert!(!session.is_ready());
    assert_eq!(
        session.send(AccountRequest::ReadLimits).unwrap_err(),
        ErrorCode::QuotaDisconnected
    );
    match session.next_event(Duration::from_secs(2)).unwrap().unwrap() {
        ProtocolEvent::Reply {
            token,
            result: Ok(RpcReply::Initialized),
        } => {
            assert_eq!(token.connection_epoch, "synthetic-transport-1");
            assert_eq!(token.request_id, "synthetic-transport-1:1");
        }
        event => panic!("unexpected {event:?}"),
    }
    assert!(session.is_ready());
}

#[test]
fn real_pipes_handshake_notifications_and_typed_replies() {
    let (_home, mut session) = launch("normal");
    ready(&mut session);
    let account = session.send(AccountRequest::ReadAccount).unwrap();
    match session.next_event(Duration::from_secs(2)).unwrap().unwrap() {
        ProtocolEvent::Reply {
            token,
            result: Ok(RpcReply::Account(AccountAvailability::QuotaEligible)),
        } => assert_eq!(token, account),
        event => panic!("unexpected {event:?}"),
    }
    let request = session.send(AccountRequest::ReadLimits).unwrap();
    assert_eq!(
        session.send(AccountRequest::ReadLimits).unwrap_err(),
        ErrorCode::QuotaServiceUnavailable
    );
    match session.next_event(Duration::from_secs(2)).unwrap().unwrap() {
        ProtocolEvent::LimitsUpdated(QuotaUpdate::Bucket(bucket)) => {
            assert_eq!(bucket.windows[0].remaining_percent, Some(60.0))
        }
        event => panic!("unexpected {event:?}"),
    }
    match session.next_event(Duration::from_secs(2)).unwrap().unwrap() {
        ProtocolEvent::Reply {
            token,
            result: Ok(RpcReply::Limits(book)),
        } => {
            assert_eq!(token, request);
            assert_eq!(book["codex"].windows[0].remaining_percent, Some(50.0));
        }
        event => panic!("unexpected {event:?}"),
    }
    session.close();
    assert!(session.process_id().is_none());
    assert_eq!(
        session.send(AccountRequest::ReadAccount).unwrap_err(),
        ErrorCode::QuotaDisconnected
    );
    session.close(); // Idempotent cleanup.
}

#[test]
fn malformed_or_unsupported_initialization_never_becomes_ready() {
    for (mode, expected) in [
        ("bad-init", ErrorCode::QuotaProtocolError),
        ("unsupported", ErrorCode::QuotaUnsupported),
    ] {
        let (_home, mut session) = launch(mode);
        match session.next_event(Duration::from_secs(2)).unwrap().unwrap() {
            ProtocolEvent::Reply {
                result: Err(code), ..
            } => assert_eq!(code, expected),
            event => panic!("unexpected {event:?}"),
        }
        assert!(!session.is_ready());
        assert!(session.process_id().is_none());
    }
}

#[test]
fn malicious_framing_and_json_are_controlled_errors_and_close_the_process() {
    for mode in ["malformed", "oversized", "truncated"] {
        let (_home, mut session) = launch(mode);
        ready(&mut session);
        session.send(AccountRequest::ReadLimits).unwrap();
        assert_eq!(
            session.next_event(Duration::from_secs(2)).unwrap_err(),
            ErrorCode::QuotaProtocolError,
            "{mode}"
        );
        assert!(!session.is_ready());
        assert!(session.process_id().is_none());
    }
    let (_home, mut session) = launch("invalid-shape");
    ready(&mut session);
    session.send(AccountRequest::ReadLimits).unwrap();
    match session.next_event(Duration::from_secs(2)).unwrap().unwrap() {
        ProtocolEvent::Reply {
            result: Err(code), ..
        } => assert_eq!(code, ErrorCode::QuotaProtocolError),
        event => panic!("unexpected {event:?}"),
    }
}

#[test]
fn stderr_is_drained_without_protocol_leakage_and_flood_cleanup_does_not_hang() {
    let (_home, mut session) = launch("stderr");
    ready(&mut session);
    let drain_deadline = std::time::Instant::now() + Duration::from_secs(2);
    while session.discarded_stderr_bytes() < 2_000_000 {
        assert!(std::time::Instant::now() < drain_deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(session.discarded_stderr_bytes(), 2_000_000);
    drop(session);
    let (_home, mut flood) = launch("flood");
    ready(&mut flood);
    std::thread::sleep(Duration::from_millis(40)); // Give the bounded pipe / frame queues time to fill.
    flood.close();
    assert!(flood.process_id().is_none());
}

#[test]
fn timed_out_reply_cannot_satisfy_a_new_request() {
    let (_home, mut session) = launch("late");
    ready(&mut session);
    let old = session.send(AccountRequest::ReadLimits).unwrap();
    match session
        .next_event(Duration::from_secs(10))
        .unwrap()
        .unwrap()
    {
        ProtocolEvent::TimedOut { token } => assert_eq!(token, old),
        event => panic!("unexpected {event:?}"),
    }
    let new = session.send(AccountRequest::ReadLimits).unwrap();
    assert_ne!(new.request_id, old.request_id);
    match session.next_event(Duration::from_secs(2)).unwrap().unwrap() {
        ProtocolEvent::Reply {
            token,
            result: Ok(RpcReply::Limits(book)),
        } => {
            assert_eq!(token, new);
            assert_eq!(book["codex"].windows[0].remaining_percent, Some(80.0));
        }
        event => panic!("unexpected {event:?}"),
    }
}

#[test]
fn account_change_invalidates_pending_reads_before_new_identity_query() {
    let (_home, mut session) = launch("account-change");
    ready(&mut session);
    let old = session.send(AccountRequest::ReadAccount).unwrap();
    assert!(matches!(
        session.next_event(Duration::from_secs(2)).unwrap().unwrap(),
        ProtocolEvent::AccountChanged
    ));
    let current = session.send(AccountRequest::ReadAccount).unwrap();
    assert_ne!(old.request_id, current.request_id);
    match session.next_event(Duration::from_secs(2)).unwrap().unwrap() {
        ProtocolEvent::Reply {
            token,
            result: Ok(RpcReply::Account(AccountAvailability::QuotaEligible)),
        } => assert_eq!(token, current),
        event => panic!("unexpected {event:?}"),
    }
}

#[test]
fn native_selection_validation_rejects_relative_paths_and_shell_scripts() {
    assert!(matches!(
        NativeService::new(Path::new("codex.exe"), None),
        Err(ErrorCode::InvalidQuery)
    ));
    let home = tempfile::tempdir().unwrap();
    let script = home.path().join("codex.cmd");
    std::fs::write(&script, "should never run").unwrap();
    #[cfg(windows)]
    assert!(matches!(
        NativeService::new(&script, None),
        Err(ErrorCode::InvalidQuery)
    ));
    #[cfg(windows)]
    {
        let link = home.path().join("script-link.exe");
        // Available only when this test user can create Windows symbolic links.
        if std::os::windows::fs::symlink_file(&script, &link).is_ok() {
            assert!(matches!(
                NativeService::new(&link, None),
                Err(ErrorCode::InvalidQuery)
            ));
        }
    }
    assert!(matches!(
        NativeService::new(
            Path::new(env!("CARGO_BIN_EXE_quota-fixture")),
            Some(Path::new("relative"))
        ),
        Err(ErrorCode::InvalidQuery)
    ));
}

#[cfg(windows)]
#[test]
fn owned_windows_job_kills_descendants_and_releases_inherited_pipes() {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    let (home, mut session) = launch("descendant");
    ready(&mut session);
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let pid: u32 = loop {
        if let Ok(text) = std::fs::read_to_string(home.path().join("descendant.pid")) {
            if let Ok(pid) = text.parse() {
                break pid;
            }
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    };
    unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        assert!(!handle.is_null());
        assert_eq!(WaitForSingleObject(handle, 0), 258); // WAIT_TIMEOUT before disconnect.
        session.close();
        assert_eq!(WaitForSingleObject(handle, 2000), 0); // WAIT_OBJECT_0 after owned job closes.
        CloseHandle(handle);
    }
    assert!(session.process_id().is_none());
}
