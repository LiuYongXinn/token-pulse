use token_pulse_core::notify::parse_codex_notification;
use token_pulse_integration::notify_channel::{MAX_WAKE_FRAME_BYTES, NotifyCapability, WakeError};

fn hint() -> token_pulse_core::notify::NotifyWakeHint {
    parse_codex_notification(br#"{"type":"agent-turn-complete","thread-id":"thread-1","turn-id":null,"input-messages":["never retain body"],"cwd":"C:\\private","last-assistant-message":"private answer"}"#).unwrap().unwrap()
}
#[test]
fn frame_contains_only_validated_ids_version_and_capability() {
    let cap = NotifyCapability::new();
    let encoded = cap.encode_hint(&hint()).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 5);
    assert_eq!(value["version"], 1);
    assert_eq!(value["registration_id"], cap.registration_id());
    assert!(value["turn_id"].is_null());
    assert_eq!(value["thread_id"], "thread-1");
    let text = std::str::from_utf8(&encoded).unwrap();
    assert!(!text.contains("private") && !text.contains("body") && !text.contains("cwd"));
    assert_eq!(cap.decode_hint(&encoded).unwrap(), hint());
    assert_eq!(
        NotifyCapability::new().decode_hint(&encoded).unwrap_err(),
        WakeError::Unauthorized
    );
}
#[test]
fn rejects_wrong_capabilities_versions_additional_fields_and_invalid_ids() {
    let cap = NotifyCapability::new();
    let original: serde_json::Value =
        serde_json::from_slice(&cap.encode_hint(&hint()).unwrap()).unwrap();
    for (key, value, expected) in [
        ("version", serde_json::json!(2), WakeError::VersionMismatch),
        ("nonce", serde_json::json!("wrong"), WakeError::Unauthorized),
        (
            "registration_id",
            serde_json::json!("unknown"),
            WakeError::Unauthorized,
        ),
        (
            "thread_id",
            serde_json::json!("../../private"),
            WakeError::InvalidFrame,
        ),
        ("turn_id", serde_json::json!(false), WakeError::InvalidFrame),
        ("body", serde_json::json!("secret"), WakeError::InvalidFrame),
    ] {
        let mut changed = original.clone();
        changed[key] = value;
        assert_eq!(
            cap.decode_hint(&serde_json::to_vec(&changed).unwrap())
                .unwrap_err(),
            expected
        );
    }
    let mut missing = original;
    missing.as_object_mut().unwrap().remove("turn_id");
    assert_eq!(
        cap.decode_hint(&serde_json::to_vec(&missing).unwrap())
            .unwrap_err(),
        WakeError::InvalidFrame
    );
}
#[test]
fn frames_are_bounded_before_parsing_and_errors_do_not_echo_sensitive_text() {
    let cap = NotifyCapability::new();
    assert_eq!(
        cap.decode_hint(&vec![b'!'; MAX_WAKE_FRAME_BYTES + 1])
            .unwrap_err(),
        WakeError::TooLarge
    );
    for bytes in [
        b"secret raw text".as_slice(),
        b"{}",
        &[0xff],
        b"{\"version\":1,\"version\":1}",
    ] {
        let error = cap.decode_hint(bytes).unwrap_err();
        assert_eq!(error, WakeError::InvalidFrame);
        assert_eq!(error.to_string(), "notify_wake_invalid");
    }
}
#[test]
fn persisted_capability_requires_both_exact_random_identifiers_and_no_unknown_fields() {
    let cap = NotifyCapability::new();
    let encoded = serde_json::to_vec(&cap).unwrap();
    let restored: NotifyCapability = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(
        restored
            .decode_hint(&cap.encode_hint(&hint()).unwrap())
            .unwrap(),
        hint()
    );
    let original: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    for (key, value) in [
        ("nonce", serde_json::json!("../bad")),
        ("registration_id", serde_json::json!("A".repeat(32))),
        ("other", serde_json::json!(1)),
    ] {
        let mut changed = original.clone();
        changed[key] = value;
        assert!(serde_json::from_value::<NotifyCapability>(changed).is_err());
    }
    assert!(serde_json::from_str::<NotifyCapability>("{}").is_err());
}

#[cfg(all(windows, feature = "test-fixture"))]
#[test]
fn real_separate_process_delivers_minimal_hint_without_main_gui() {
    use std::{
        io::Write,
        process::{Command, Stdio},
        sync::Arc,
        time::Duration,
    };
    use token_pulse_integration::notify_channel::windows::NotifyListener;
    let cap = NotifyCapability::new();
    let (tx, rx) = std::sync::mpsc::channel();
    let listener = NotifyListener::start(
        cap.clone(),
        Arc::new(move |hint| {
            let _ = tx.send(hint);
        }),
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_notify-channel-fixture"))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&cap).unwrap())
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("fixture did not exit");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let delivered = rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(delivered.thread_id(), "fixture-thread");
    assert_eq!(delivered.turn_id(), Some("fixture-turn"));
    drop(listener);
}
