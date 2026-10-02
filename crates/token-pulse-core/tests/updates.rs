use token_pulse_core::{
    error::ErrorCode,
    numeric::{DecimalInt, EpochMs},
    updates::*,
};

fn revision(value: &str) -> DecimalInt {
    DecimalInt::parse(value).unwrap()
}
fn offer(version: &str) -> UpdateRelease {
    UpdateRelease {
        version: version.into(),
        notes: Some("公开版本说明\n修正显示".into()),
        published_at_ms: None,
    }
}
fn available() -> UpdateWorkflow {
    let mut workflow = UpdateWorkflow::new("0.1.0".into(), None).unwrap();
    let check = workflow.begin_check().unwrap();
    workflow
        .checked(check, Some(offer("0.2.0")), EpochMs::new(100).unwrap())
        .unwrap();
    workflow
}

#[test]
fn absent_publication_never_reports_current_or_invents_download_measurements() {
    let mut missing =
        UpdateWorkflow::new("0.1.0".into(), Some(UpdateIssue::PublicationNotConfigured)).unwrap();
    assert_eq!(missing.begin_check(), Err(ErrorCode::UpdateUnavailable));
    let actual = serde_json::to_value(missing.snapshot()).unwrap();
    assert_eq!(
        actual,
        serde_json::json!({
            "update_revision":"0", "phase":"unavailable", "current_version":"0.1.0",
            "release":null, "last_checked_at_ms":null, "downloaded_bytes":null,
            "total_bytes":null, "issue":"publication_not_configured"
        })
    );
    assert!(UpdateWorkflow::new("0.1.0".into(), Some(UpdateIssue::Network)).is_err());
    let mut supported = UpdateWorkflow::new("0.1.0".into(), None).unwrap();
    let operation = supported.begin_check().unwrap();
    supported
        .checked(operation, None, EpochMs::new(234).unwrap())
        .unwrap();
    assert_eq!(supported.snapshot().phase, UpdatePhase::Current);
    assert_eq!(
        supported.snapshot().last_checked_at_ms.unwrap().value(),
        234
    );
    assert_eq!(supported.snapshot().downloaded_bytes, None);
}

#[test]
fn eof_is_verification_in_progress_and_cannot_authorize_installation() {
    let mut workflow = available();
    let download = workflow.begin_download(&revision("2")).unwrap();
    assert_eq!(workflow.snapshot().downloaded_bytes, Some(revision("0")));
    assert_eq!(
        workflow.begin_install(&revision("3")),
        Err(ErrorCode::UpdateBusy)
    );
    workflow.progress(download, 19, None).unwrap();
    assert_eq!(workflow.snapshot().total_bytes, None);
    assert_eq!(
        workflow.verified(download, 19),
        Err(ErrorCode::CandidateObsolete)
    );
    workflow.begin_verification(download).unwrap();
    assert_eq!(workflow.snapshot().phase, UpdatePhase::Verifying);
    assert_eq!(
        workflow.begin_install(&revision("5")),
        Err(ErrorCode::UpdateBusy)
    );
    workflow
        .failed(download, UpdateIssue::SignatureInvalid)
        .unwrap();
    assert_eq!(workflow.snapshot().phase, UpdatePhase::Error);
    assert_eq!(
        workflow.snapshot().issue,
        Some(UpdateIssue::SignatureInvalid)
    );
    assert_eq!(
        workflow.begin_install(&revision("6")),
        Err(ErrorCode::InvalidQuery)
    );
    assert_eq!(
        workflow.verified(download, 19),
        Err(ErrorCode::CandidateObsolete)
    );
    let next = workflow.begin_check().unwrap();
    assert_eq!(workflow.snapshot().release, None);
    assert_eq!(workflow.snapshot().downloaded_bytes, None);
    workflow
        .checked(next, None, EpochMs::new(300).unwrap())
        .unwrap();
    assert_eq!(workflow.snapshot().phase, UpdatePhase::Current);
}

#[test]
fn bytes_and_total_are_exact_and_invalid_progress_changes_nothing() {
    let mut workflow = available();
    let download = workflow.begin_download(&revision("2")).unwrap();
    workflow
        .progress(download, 9_007_199_254_740_993, Some(9_007_199_254_741_000))
        .unwrap();
    let before = workflow.snapshot();
    assert_eq!(before.downloaded_bytes, Some(revision("9007199254740993")));
    assert_eq!(
        workflow.progress(download, 8, None),
        Err(ErrorCode::InvalidUsage)
    );
    assert_eq!(workflow.snapshot(), before);
    assert_eq!(
        workflow.progress(download, 1, Some(9_007_199_254_741_001)),
        Err(ErrorCode::InvalidUsage)
    );
    assert_eq!(workflow.snapshot(), before);
    workflow.progress(download, 7, None).unwrap();
    workflow.begin_verification(download).unwrap();
    let before = workflow.snapshot();
    assert_eq!(
        workflow.verified(download, 9_007_199_254_740_999),
        Err(ErrorCode::InvalidUsage)
    );
    assert_eq!(workflow.snapshot(), before);
    workflow.verified(download, 9_007_199_254_741_000).unwrap();
    let ready = workflow.snapshot();
    assert_eq!(ready.phase, UpdatePhase::ReadyToInstall);
    assert_eq!(ready.update_revision, revision("7"));
    assert_eq!(ready.total_bytes, Some(revision("9007199254741000")));
    let install = workflow.begin_install(&revision("7")).unwrap();
    assert_eq!(workflow.snapshot().phase, UpdatePhase::Installing);
    assert_eq!(workflow.begin_check(), Err(ErrorCode::UpdateBusy));
    workflow
        .failed(install, UpdateIssue::InstallFailed)
        .unwrap();
    assert_eq!(workflow.snapshot().issue, Some(UpdateIssue::InstallFailed));
}

#[test]
fn old_offers_and_late_results_cannot_replace_new_state() {
    let mut workflow = available();
    assert_eq!(
        workflow.begin_download(&revision("1")),
        Err(ErrorCode::RevisionConflict)
    );
    let old = workflow.begin_download(&revision("2")).unwrap();
    workflow.progress(old, 2, Some(2)).unwrap();
    workflow.begin_verification(old).unwrap();
    workflow.verified(old, 2).unwrap();
    let old_ready = workflow.snapshot().update_revision;
    let new = workflow.begin_check().unwrap();
    assert_eq!(workflow.begin_check(), Err(ErrorCode::UpdateBusy));
    let before = workflow.snapshot();
    assert_eq!(
        workflow.failed(old, UpdateIssue::DownloadFailed),
        Err(ErrorCode::CandidateObsolete)
    );
    assert_eq!(workflow.snapshot(), before);
    workflow
        .checked(new, Some(offer("0.3.0")), EpochMs::new(999).unwrap())
        .unwrap();
    assert_eq!(
        workflow.begin_install(&old_ready),
        Err(ErrorCode::RevisionConflict)
    );
    assert_eq!(workflow.snapshot().release.unwrap().version, "0.3.0");
}

#[test]
fn metadata_is_bounded_and_renderer_inputs_have_no_artifact_or_verification_fields() {
    let mut workflow = UpdateWorkflow::new("0.1.0".into(), None).unwrap();
    let check = workflow.begin_check().unwrap();
    let before = workflow.snapshot();
    for invalid in [
        offer("../installer.exe"),
        UpdateRelease {
            notes: Some("\0raw".into()),
            ..offer("0.2.0")
        },
        UpdateRelease {
            notes: Some("a".repeat(32769)),
            ..offer("0.2.0")
        },
    ] {
        assert_eq!(
            workflow.checked(check, Some(invalid), EpochMs::new(100).unwrap()),
            Err(ErrorCode::InvalidQuery)
        );
        assert_eq!(workflow.snapshot(), before);
    }
    for injected in [
        r#"{"expected_update_revision":"2","url":"http://local/arbitrary.exe"}"#,
        r#"{"expected_update_revision":"2","verified":true}"#,
        r#"{"expected_update_revision":"2","public_key":"other"}"#,
        r#"{"expected_update_revision":2}"#,
    ] {
        assert!(serde_json::from_str::<UpdateActionRequest>(injected).is_err());
    }
    let request: UpdateActionRequest =
        serde_json::from_str(r#"{"expected_update_revision":"9007199254740993"}"#).unwrap();
    assert_eq!(
        request.expected_update_revision,
        revision("9007199254740993")
    );
}

#[test]
fn known_length_truncation_and_empty_file_cannot_be_marked_ready() {
    for (chunk, total) in [(9, Some(10)), (0, Some(0)), (0, None)] {
        let mut workflow = available();
        let operation = workflow.begin_download(&revision("2")).unwrap();
        workflow.progress(operation, chunk, total).unwrap();
        workflow.begin_verification(operation).unwrap();
        let before = workflow.snapshot();
        assert_eq!(
            workflow.verified(operation, chunk),
            Err(ErrorCode::InvalidUsage)
        );
        assert_eq!(workflow.snapshot(), before);
    }
}
