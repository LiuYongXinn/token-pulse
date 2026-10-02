use std::ffi::OsString;
use token_pulse_integration::notify_invocation::{InvocationError, parse_invocation};
const ID: &str = "0123456789abcdef0123456789abcdef";
fn parse(
    arguments: &[&str],
) -> Result<Option<token_pulse_integration::notify_invocation::NotifyInvocation>, InvocationError> {
    parse_invocation(arguments.iter().map(OsString::from))
}
#[test]
fn requires_exact_notify_arguments_and_never_treats_misplaced_flag_as_gui_mode() {
    let payload = r#"{"type":"agent-turn-complete","thread-id":"t1"}"#;
    for args in [
        vec!["--tokenpulse-notify"],
        vec!["--tokenpulse-notify", "--integration", ID],
        vec!["--tokenpulse-notify", "--wrong", ID, payload],
        vec!["--tokenpulse-notify", "--integration", "../bad", payload],
        vec!["--tokenpulse-notify", "--integration", ID, payload, "extra"],
        vec!["--native-smoke", "--tokenpulse-notify"],
    ] {
        assert_eq!(
            parse(&args).err().unwrap(),
            InvocationError::InvalidArguments
        );
    }
    assert!(parse(&[]).unwrap().is_none());
    assert!(
        parse(&["--native-smoke", "--native-mini-smoke"])
            .unwrap()
            .is_none()
    );
}
#[test]
fn returns_only_validated_identity_and_discards_raw_notification_fields() {
    let invocation=parse(&["--tokenpulse-notify","--integration",ID,
        r#"{"type":"agent-turn-complete","thread-id":"t1","turn-id":null,"cwd":"private","input-messages":["secret"],"auth":"never retain"}"#]).unwrap().unwrap();
    assert_eq!(invocation.registration_id(), ID);
    let hint = invocation.hint().unwrap();
    assert_eq!(hint.thread_id(), "t1");
    assert!(hint.turn_id().is_none());
    assert_eq!(
        serde_json::to_value(hint).unwrap(),
        serde_json::json!({"thread_id":"t1","turn_id":null})
    );
    assert!(
        parse(&[
            "--tokenpulse-notify",
            "--integration",
            ID,
            r#"{"type":"unsupported"}"#
        ])
        .unwrap()
        .unwrap()
        .hint()
        .is_none()
    );
}
#[test]
fn payload_failures_are_bounded_and_do_not_echo_contents() {
    for payload in [
        "private body".to_owned(),
        "!".repeat(64 * 1024 + 1),
        r#"{"type":"agent-turn-complete","thread-id":"../bad"}"#.into(),
    ] {
        let error = parse(&["--tokenpulse-notify", "--integration", ID, &payload])
            .err()
            .unwrap();
        assert_eq!(error.to_string(), "notify_invocation_invalid_payload");
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        let invalid = OsString::from_wide(&[0xd800]);
        assert_eq!(
            parse_invocation([
                OsString::from("--tokenpulse-notify"),
                OsString::from("--integration"),
                OsString::from(ID),
                invalid
            ])
            .err()
            .unwrap(),
            InvocationError::InvalidPayload
        );
    }
}

#[cfg(windows)]
#[test]
fn readonly_config_checks_owned_command_and_rejects_hardlink_and_oversized_input() {
    use token_pulse_integration::{
        notify_channel::NotifyCapability,
        notify_config::{
            ConfigError, MAX_CONFIG_BYTES, ManagedNotifyCommand, owns_current_notify,
            prepare_enable, windows::read_config,
        },
    };
    let temp = tempfile::tempdir().unwrap();
    let cap = NotifyCapability::new();
    let command =
        ManagedNotifyCommand::new(&std::env::current_exe().unwrap(), cap.registration_id())
            .unwrap();
    let before = b"notify=['old']\n[profile]\nnotify=['nested']\n";
    let plan = prepare_enable(before, &command).unwrap();
    let installed = plan.apply_to(before).unwrap();
    let path = temp.path().join("config.toml");
    std::fs::write(&path, &installed).unwrap();
    let read = read_config(temp.path()).unwrap();
    assert!(owns_current_notify(&read, plan.restore_record()).unwrap());
    assert!(!owns_current_notify(before, plan.restore_record()).unwrap());
    assert!(!owns_current_notify(b"[profile]\nnotify=[]", plan.restore_record()).unwrap());
    assert_eq!(std::fs::read(&path).unwrap(), installed);
    std::fs::hard_link(&path, temp.path().join("outside-alias")).unwrap();
    assert_eq!(
        read_config(temp.path()).unwrap_err(),
        ConfigError::InvalidToml
    );
    std::fs::remove_file(temp.path().join("outside-alias")).unwrap();
    let large = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    large.set_len(MAX_CONFIG_BYTES as u64 + 1).unwrap();
    drop(large);
    assert_eq!(read_config(temp.path()).unwrap_err(), ConfigError::TooLarge);
}
