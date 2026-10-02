use std::path::{Path, PathBuf};
use token_pulse_integration::notify_config::{
    ConfigError, MAX_CONFIG_BYTES, ManagedNotifyCommand, NotifyRestoreRecord, prepare_enable,
    restore,
};

fn executable() -> PathBuf {
    std::env::temp_dir()
        .join("Token Pulse 中文")
        .join("token-pulse.exe")
}
fn command() -> ManagedNotifyCommand {
    ManagedNotifyCommand::new(&executable(), "0123456789abcdef0123456789abcdef").unwrap()
}
fn edited(source: &str) -> (Vec<u8>, NotifyRestoreRecord) {
    let plan = prepare_enable(source.as_bytes(), &command()).unwrap();
    (
        plan.apply_to(source.as_bytes()).unwrap(),
        plan.restore_record().clone(),
    )
}

#[test]
fn changes_only_root_array_bytes_and_restores_exact_multiline_syntax() {
    let before = "# 保留\r\nmodel = 'unchanged'\r\n\"notify\"  = [\r\n  'old.exe', # inside\r\n  '''literal arg''',\r\n] # outside\r\n[profiles.work]\r\nnotify = ['nested']\r\n";
    let raw = "[\r\n  'old.exe', # inside\r\n  '''literal arg''',\r\n]";
    let plan = prepare_enable(before.as_bytes(), &command()).unwrap();
    let expected = before.replacen(raw, plan.next_value(), 1);
    let installed = plan.apply_to(before.as_bytes()).unwrap();
    assert_eq!(installed, expected.as_bytes());
    assert_eq!(plan.restore_record().original_value(), Some(raw));
    assert_eq!(
        plan.restore_record().original_arguments().unwrap(),
        Some(vec!["old.exe".into(), "literal arg".into()])
    );
    assert_eq!(
        restore(&installed, plan.restore_record()).unwrap(),
        before.as_bytes()
    );
}

#[test]
fn inserts_before_tables_and_removes_only_owned_assignment_with_bom_and_eof() {
    for source in [
        "",
        "\u{feff}",
        "# comment",
        "[profiles.work]\nnotify = ['nested']\n",
        "\u{feff}# 注释\r\n[profile]\r\nmodel='a'\r\n",
        "x=1",
    ] {
        let plan = prepare_enable(source.as_bytes(), &command()).unwrap();
        assert!(plan.restore_record().original_value().is_none());
        assert!(
            plan.restore_record()
                .original_arguments()
                .unwrap()
                .is_none()
        );
        let result = plan.apply_to(source.as_bytes()).unwrap();
        let text = std::str::from_utf8(&result)
            .unwrap()
            .trim_start_matches('\u{feff}');
        let doc = toml_edit::Document::parse(text).unwrap();
        let args: Vec<_> = doc
            .get("notify")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(args, command().arguments());
        assert_eq!(
            restore(&result, plan.restore_record()).unwrap(),
            source.as_bytes()
        );
    }
}

#[test]
fn absent_and_explicitly_disabled_notify_are_distinct() {
    let (result, record) = edited("notify=[] # disabled\n");
    assert_eq!(record.original_value(), Some("[]"));
    assert_eq!(record.original_arguments().unwrap(), Some(vec![]));
    assert_eq!(
        restore(&result, &record).unwrap(),
        b"notify=[] # disabled\n"
    );
}

#[test]
fn restore_retains_unrelated_new_values_key_format_and_comments() {
    let (result, record) = edited("notify = ['old']\nmodel='a'\n");
    let latest = String::from_utf8(result)
        .unwrap()
        .replace("notify =", "'notify'\t=")
        .replace(
            "model='a'",
            "model='b' # user edit\n[profiles.work]\nnotify=['profile']",
        );
    let value = latest
        .split_once('=')
        .unwrap()
        .1
        .split_once('\n')
        .unwrap()
        .0
        .trim();
    let expected = latest.replacen(value, "['old']", 1);
    assert_eq!(
        restore(latest.as_bytes(), &record).unwrap(),
        expected.as_bytes()
    );

    let (result, record) = edited("model='a'\n");
    let latest = String::from_utf8(result)
        .unwrap()
        .replacen('\n', " # keep this comment\n", 1);
    assert_eq!(
        restore(latest.as_bytes(), &record).unwrap(),
        b" # keep this comment\nmodel='a'\n"
    );
}

#[test]
fn any_changed_config_bytes_reject_stale_prepared_apply() {
    let source = b"notify=['old']\nx=1\n";
    let plan = prepare_enable(source, &command()).unwrap();
    for changed in [
        b"notify=['old']\nx=2\n".as_slice(),
        b"notify=['old']\nx=1\n# comment\n",
        b"",
    ] {
        assert_eq!(plan.apply_to(changed).unwrap_err(), ConfigError::StalePlan);
    }
    assert!(plan.apply_to(source).is_ok());
}

#[test]
fn removed_modified_or_reassigned_notify_cannot_be_overwritten_by_undo() {
    let (result, record) = edited("notify=['old']\n");
    let text = String::from_utf8(result).unwrap();
    for changed in [
        "".to_owned(),
        "notify=['user']".to_owned(),
        text.replace(
            "0123456789abcdef0123456789abcdef",
            "abcdef0123456789abcdef0123456789",
        ),
        text.replace("--integration", "--something-else"),
        text.replace("]", ", 'extra']"),
    ] {
        assert_eq!(
            restore(changed.as_bytes(), &record).unwrap_err(),
            ConfigError::OwnershipConflict
        );
    }
    assert_eq!(
        restore(b"notify = 5", &record).unwrap_err(),
        ConfigError::InvalidNotify
    );
}

#[test]
fn existing_managed_command_is_rejected_to_prevent_self_chaining() {
    let (result, _) = edited("notify=['old']");
    assert_eq!(
        prepare_enable(&result, &command()).err().unwrap(),
        ConfigError::AlreadyManaged
    );
    assert_eq!(
        prepare_enable(
            b"notify=['different.exe','--tokenpulse-notify']",
            &command()
        )
        .err()
        .unwrap(),
        ConfigError::AlreadyManaged
    );
}

#[test]
fn malformed_types_and_bounded_arguments_fail_without_configuration_excerpts() {
    for source in [
        "notify = 3",
        "notify = 'sensitive secret'",
        "notify=[true]",
        "[notify]\nx=1",
        "notify.key=1",
    ] {
        let error = prepare_enable(source.as_bytes(), &command()).err().unwrap();
        assert_eq!(error, ConfigError::InvalidNotify);
        assert_eq!(error.to_string(), "notify_config_invalid_command");
    }
    for source in ["notify=[", "notify=[]\nnotify=[]", "sensitive secret"] {
        assert_eq!(
            prepare_enable(source.as_bytes(), &command()).err().unwrap(),
            ConfigError::InvalidToml
        );
    }
    assert_eq!(
        prepare_enable(&[0xff], &command()).err().unwrap(),
        ConfigError::InvalidToml
    );
    for source in [
        format!("notify=[{}]", vec!["'a'"; 65].join(",")),
        format!("notify=['{}']", "a".repeat(16 * 1024 + 1)),
        format!(
            "notify=[{}]",
            vec![format!("'{}'", "a".repeat(16 * 1024)); 5].join(",")
        ),
        "notify=['a',\"\\u0000\"]".into(),
    ] {
        assert_eq!(
            prepare_enable(source.as_bytes(), &command()).err().unwrap(),
            ConfigError::InvalidNotify
        );
    }
    let mut exact_limit = b"notify=['old']\n#".to_vec();
    exact_limit.resize(MAX_CONFIG_BYTES, b'a');
    assert_eq!(
        prepare_enable(&exact_limit, &command()).err().unwrap(),
        ConfigError::TooLarge
    ); // edited result exceeds limit
    exact_limit.push(b'a');
    assert_eq!(
        prepare_enable(&exact_limit, &command()).err().unwrap(),
        ConfigError::TooLarge
    );
}

#[test]
fn generated_command_escapes_arguments_and_requires_absolute_exe_and_registration() {
    let path = std::env::temp_dir()
        .join("path with space ' unicode 中文")
        .join("token-pulse.exe");
    let command = ManagedNotifyCommand::new(&path, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
    let plan = prepare_enable(b"", &command).unwrap();
    let doc = toml_edit::Document::parse(format!("notify = {}", plan.next_value())).unwrap();
    let args: Vec<_> = doc
        .get("notify")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(args, command.arguments());
    for id in [
        "",
        "abc",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "../../../../../../../../../../..",
    ] {
        assert_eq!(
            ManagedNotifyCommand::new(&path, id).err().unwrap(),
            ConfigError::InvalidManagedCommand
        );
    }
    assert_eq!(
        ManagedNotifyCommand::new(
            Path::new("relative.exe"),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        )
        .err()
        .unwrap(),
        ConfigError::InvalidManagedCommand
    );
}

#[test]
fn persisted_record_excludes_unrelated_settings_and_rejects_injected_restore_syntax() {
    let (_, record) = edited("notify=['old']\nsecret='never serialize this'\n");
    let encoded = serde_json::to_string(&record).unwrap();
    assert!(!encoded.contains("secret"));
    let decoded: NotifyRestoreRecord = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded.original_value(), Some("['old']"));
    let wire: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    let mut missing = wire.clone();
    missing.as_object_mut().unwrap().remove("original_value");
    assert!(serde_json::from_value::<NotifyRestoreRecord>(missing).is_err());
    for invalid in [
        "[]\nx=42",
        "[] # suffix",
        "[]\n[profile]\nx=42",
        "3",
        "[true]",
        "['exe','--tokenpulse-notify']",
    ] {
        let mut corrupt = wire.clone();
        corrupt["original_value"] = serde_json::Value::String(invalid.to_owned());
        assert!(serde_json::from_value::<NotifyRestoreRecord>(corrupt).is_err());
    }
    for (key, value) in [
        ("version", serde_json::json!(2)),
        ("installed_arguments", serde_json::json!(["evil.exe"])),
        ("extra", serde_json::json!("secret")),
    ] {
        let mut corrupt = wire.clone();
        corrupt[key] = value;
        assert!(serde_json::from_value::<NotifyRestoreRecord>(corrupt).is_err());
    }
}
