use std::time::{Duration, Instant};
use token_pulse_core::{error::ErrorCode, numeric::DecimalInt, quota::*};
fn target() -> AccountServiceTarget {
    #[cfg(windows)]
    let program = "C:\\Synthetic\\codex.exe";
    #[cfg(not(windows))]
    let program = "/synthetic/codex";
    AccountServiceTarget {
        executable_path: program.into(),
        home_path: None,
        executable_sha256: "ab".repeat(32),
    }
}
#[test]
fn stored_targets_and_renderer_contracts_reject_credential_fields_and_uncontrolled_paths() {
    target().validate().unwrap();
    for bad in ["codex.exe", "\n", ""] {
        let mut value = target();
        value.executable_path = bad.into();
        assert_eq!(value.validate(), Err(ErrorCode::InvalidQuery));
    }
    #[cfg(windows)]
    for bad in [
        "C:\\codex.cmd",
        "\\\\server\\share\\codex.exe",
        "\\\\.\\C:\\codex.exe",
        "C:\\fake\\..\\codex.exe",
    ] {
        let mut value = target();
        value.executable_path = bad.into();
        assert_eq!(value.validate(), Err(ErrorCode::InvalidQuery));
    }
    let mut value = target();
    value.home_path = Some("relative".into());
    assert!(value.validate().is_err());
    for bad in ["a".repeat(63), "A".repeat(64), "x".repeat(64)] {
        let mut value = target();
        value.executable_sha256 = bad;
        assert!(value.validate().is_err());
    }
    assert!(
        AccountServicePreferences {
            target: None,
            auto_connect: true
        }
        .validate()
        .is_err()
    );
    let mut value = serde_json::to_value(target()).unwrap();
    value["auth"] = "SECRET".into();
    assert!(serde_json::from_value::<AccountServiceTarget>(value).is_err());
    let raw = serde_json::json!({"kind":"connect","expected_settings_revision":"1","expected_connection_epoch":"quota-fixture-1","acknowledged_executable_sha256":"ab".repeat(32),"executable_path":"ARBITRARY"});
    assert!(serde_json::from_value::<AccountConnectionRequest>(raw).is_err());
    let request = AccountConnectionRequest::Connect {
        expected_settings_revision: DecimalInt::parse("9223372036854775808").unwrap(),
        expected_connection_epoch: "quota-fixture-1".into(),
        acknowledged_executable_sha256: "ab".repeat(32),
    };
    assert_eq!(request.validate(), Err(ErrorCode::InvalidQuery));
}
#[test]
fn executable_selection_capabilities_expire_and_never_cross_domains_or_windows() {
    let mut selections = AccountServiceSelections::default();
    let now = Instant::now();
    let candidate = AccountServiceCandidate {
        target: target(),
        settings_revision: DecimalInt::parse("9007199254740993").unwrap(),
    };
    selections
        .insert("account-handle".into(), candidate.clone(), now)
        .unwrap();
    assert_eq!(
        selections.get("account-handle", "mini", now).err(),
        Some(ErrorCode::PermissionDenied)
    );
    assert_eq!(
        selections.get("source-handle", "main", now).err(),
        Some(ErrorCode::StaleConfirmation)
    );
    assert_eq!(
        selections
            .get("account-handle", "main", now)
            .unwrap()
            .settings_revision
            .as_str(),
        "9007199254740993"
    );
    assert!(
        selections
            .insert("account-handle".into(), candidate.clone(), now)
            .is_err()
    );
    assert!(
        selections
            .get("account-handle", "main", now + Duration::from_secs(299))
            .is_ok()
    );
    assert_eq!(
        selections
            .get("account-handle", "main", now + Duration::from_secs(300))
            .err(),
        Some(ErrorCode::StaleConfirmation)
    );
    for index in 0..16 {
        selections
            .insert(
                format!("new-{index}"),
                candidate.clone(),
                now + Duration::from_secs(300),
            )
            .unwrap();
    }
    assert!(
        selections
            .insert("overflow".into(), candidate, now + Duration::from_secs(300))
            .is_err()
    );
    selections.remove("new-0");
    assert_eq!(
        selections
            .get("new-0", "main", now + Duration::from_secs(300))
            .err(),
        Some(ErrorCode::StaleConfirmation)
    );
}
#[test]
fn configuration_previews_obey_latest_privacy_without_hiding_unknowns_or_controls() {
    use token_pulse_core::privacy::{DisplayPolicyStamp, PrivacyState, PrivateResponse};
    let preferences = AccountServicePreferences {
        target: Some(target()),
        auto_connect: false,
    };
    let snapshot = AccountServiceConfigSnapshot::from_preferences(
        &preferences,
        DecimalInt::parse("1").unwrap(),
    );
    let policy = PrivacyState::new(DisplayPolicyStamp {
        settings_revision: DecimalInt::parse("1").unwrap(),
        privacy: false,
    });
    let response = PrivateResponse::new("config-fixture".into(), snapshot.clone(), policy.clone());
    policy
        .publish(DisplayPolicyStamp {
            settings_revision: DecimalInt::parse("2").unwrap(),
            privacy: true,
        })
        .unwrap();
    let hidden = serde_json::to_value(response).unwrap();
    assert!(!hidden.to_string().contains("Synthetic"));
    assert_eq!(hidden["data"]["home_display_path"], serde_json::Value::Null);
    assert_eq!(hidden["data"]["executable_sha256"], "ab".repeat(32));
    assert_eq!(hidden["data"]["configured"], true);
    assert_eq!(
        snapshot.executable_display_path,
        Some(target().executable_path)
    );
}
