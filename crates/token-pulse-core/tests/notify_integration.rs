use serde_json::json;
use token_pulse_core::{
    error::ErrorCode,
    notify_integration::*,
    numeric::DecimalInt,
    privacy::{DisplayPolicyStamp, PrivacyState, PrivateResponse},
};
fn stamp(revision: &str, privacy: bool) -> DisplayPolicyStamp {
    DisplayPolicyStamp {
        settings_revision: DecimalInt::parse(revision).unwrap(),
        privacy,
    }
}
#[test]
fn management_requests_reject_arbitrary_paths_commands_and_capabilities() {
    for extra in [
        json!({"path":"synthetic-private"}),
        json!({"command":["synthetic.exe"]}),
        json!({"nonce":"synthetic-secret"}),
    ] {
        let mut request =
            json!({"kind":"enable_source","source_id":"source","chain_original":null});
        request
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert!(serde_json::from_value::<NotifyPrepareAction>(request).is_err());
    }
    for id in [
        "",
        "../auth.json",
        "0123456789ABCDEF0123456789ABCDEF",
        &"a".repeat(33),
    ] {
        assert_eq!(validate_notify_id(id), Err(ErrorCode::InvalidQuery));
    }
    assert!(validate_notify_id("0123456789abcdef0123456789abcdef").is_ok());
    let request: NotifyPrepareAction =
        serde_json::from_value(json!({"kind":"choose_home","chain_original":null})).unwrap();
    request.validate().unwrap();
}
#[test]
fn delayed_privacy_redacts_only_notify_review_and_paths_without_faking_unknowns() {
    let policy = PrivacyState::new(stamp("1", false));
    let preview = NotifyConfigPreview {
        plan_id: "a".repeat(32),
        registration_id: "b".repeat(32),
        operation: NotifyConfigOperation::Enable,
        home_path: Some("SECRET home".into()),
        before_notify: Some("SECRET original arguments".into()),
        after_notify: Some("SECRET executable".into()),
        creates_config: false,
        can_chain_original: true,
        chain_original: true,
        settings_revision: DecimalInt::parse("9007199254740993").unwrap(),
        expires_in_seconds: 120,
        redacted: false,
    };
    let review = PrivateResponse::new("review".into(), Some(preview), policy.clone());
    let status = PrivateResponse::new(
        "status".into(),
        NotifyIntegrationsSnapshot {
            ready: false,
            listener_count: None,
            service_issue: Some(NotifyIssue::Unavailable),
            registrations: Some(vec![NotifyIntegrationRow {
                registration_id: "b".repeat(32),
                home_path: Some("SECRET home".into()),
                configured: None,
                current_executable: None,
                chain_original: None,
                issue: Some(NotifyIssue::InvalidConfig),
            }]),
            registry_issue: None,
            redacted: false,
        },
        policy.clone(),
    );
    policy.publish(stamp("2", true)).unwrap();
    let review = serde_json::to_value(review).unwrap();
    let status = serde_json::to_value(status).unwrap();
    assert!(!review.to_string().contains("SECRET") && !status.to_string().contains("SECRET"));
    assert_eq!(review["data"]["settings_revision"], "9007199254740993");
    assert_eq!(review["data"]["before_notify"], json!(null));
    assert_eq!(review["data"]["redacted"], true);
    assert_eq!(status["data"]["listener_count"], json!(null));
    assert_eq!(
        status["data"]["registrations"][0]["configured"],
        json!(null)
    );
    assert_eq!(
        status["data"]["registrations"][0]["issue"],
        "invalid_config"
    );
}
#[test]
fn visible_operations_use_latest_policy_and_do_not_execute_while_private() {
    let policy = PrivacyState::new(stamp("1", false));
    let revision = policy
        .with_visible_operation::<_, ErrorCode>(|stamp| Ok(stamp.settings_revision.clone()))
        .unwrap();
    assert_eq!(revision.as_str(), "1");
    policy.publish(stamp("2", true)).unwrap();
    let mut ran = false;
    let result = policy.with_visible_operation::<_, ErrorCode>(|_| {
        ran = true;
        Ok(())
    });
    assert_eq!(result, Err(ErrorCode::PermissionDenied));
    assert!(!ran);
    policy.publish(stamp("3", false)).unwrap();
    assert_eq!(
        policy
            .with_visible_operation::<_, ErrorCode>(|stamp| Ok(stamp
                .settings_revision
                .as_str()
                .to_owned()))
            .unwrap(),
        "3"
    );
}
