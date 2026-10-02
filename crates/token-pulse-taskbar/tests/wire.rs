use std::io::{Cursor, Read};
use token_pulse_core::{
    numeric::{DecimalInt, DecimalMoney, EpochMs},
    protocol::{CoverageState, QuotaState},
};
use token_pulse_taskbar::*;

fn number(v: &str) -> DecimalInt {
    DecimalInt::parse(v).unwrap()
}
fn frame(sequence: &str, body: HostMessage) -> Envelope<HostMessage> {
    Envelope {
        protocol_version: 1,
        host_instance_id: "synthetic-host".into(),
        nonce: "a".repeat(64),
        sequence: number(sequence),
        body,
    }
}
fn session() -> HostSession {
    HostSession::new("synthetic-host".into(), "a".repeat(64)).unwrap()
}
fn view(privacy: bool, revision: &str) -> TaskbarView {
    TaskbarView {
        settings_revision: number(revision),
        usage_revision: number("9007199254740993"),
        price_revision: number("3"),
        generated_at_ms: EpochMs::new(1000).unwrap(),
        privacy,
        scope_label: (!privacy).then(|| "SYNTHETIC SESSION".into()),
        timezone: "UTC".into(),
        total_tokens: Some(number("9007199254740993")),
        input_tokens: None,
        cached_tokens: Some(number("0")),
        output_tokens: None,
        usage_status: CoverageState::Partial,
        costs: if privacy {
            vec![]
        } else {
            vec![HostCost {
                currency: "USD".into(),
                estimated_cost: Some(DecimalMoney::parse("0.000000000000001").unwrap()),
            }]
        },
        priced_tokens: number("0"),
        unpriced_tokens: number("9007199254740993"),
        quota: None,
    }
}

#[test]
fn framing_roundtrips_coalesced_messages_and_exact_numbers() {
    let mut bytes = vec![];
    write_frame(&mut bytes, &frame("1", HostMessage::Hello {})).unwrap();
    write_frame(
        &mut bytes,
        &frame(
            "9007199254740993",
            HostMessage::Snapshot {
                view: Box::new(view(false, "2")),
            },
        ),
    )
    .unwrap();
    let mut reader = Cursor::new(bytes);
    assert!(matches!(
        read_frame::<_, HostMessage>(&mut reader)
            .unwrap()
            .unwrap()
            .body,
        HostMessage::Hello {}
    ));
    let message = read_frame::<_, HostMessage>(&mut reader).unwrap().unwrap();
    assert_eq!(message.sequence.as_str(), "9007199254740993");
    let HostMessage::Snapshot { view } = message.body else {
        panic!("snapshot expected")
    };
    assert_eq!(view.total_tokens.unwrap().as_str(), "9007199254740993");
    assert_eq!(
        view.costs[0].estimated_cost.as_ref().unwrap().as_str(),
        "0.000000000000001"
    );
    assert_eq!(view.cached_tokens.unwrap().as_str(), "0");
    assert!(view.input_tokens.is_none());
    assert!(read_frame::<_, HostMessage>(&mut reader).unwrap().is_none());
}
#[test]
fn fragmented_reads_and_truncated_or_oversized_frames_are_distinct() {
    struct Fragment(Cursor<Vec<u8>>);
    impl Read for Fragment {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            self.0.read(&mut bytes[..1])
        }
    }
    let mut bytes = vec![];
    write_frame(&mut bytes, &frame("1", HostMessage::Hello {})).unwrap();
    assert!(
        read_frame::<_, HostMessage>(&mut Fragment(Cursor::new(bytes.clone())))
            .unwrap()
            .is_some()
    );
    for missing in [1, bytes.len() - 1] {
        assert_eq!(
            read_frame::<_, HostMessage>(&mut Cursor::new(&bytes[..missing])).err(),
            Some(WireError::Io(std::io::ErrorKind::UnexpectedEof))
        );
    }
    let mut too_large = Cursor::new(((MAX_FRAME_BYTES + 1) as u32).to_le_bytes());
    assert_eq!(
        read_frame::<_, HostMessage>(&mut too_large).err(),
        Some(WireError::TooLarge)
    );
    assert_eq!(too_large.position(), 4); // No declared payload allocation or read.
    assert_eq!(
        read_frame::<_, HostMessage>(&mut Cursor::new([0; 4])).err(),
        Some(WireError::InvalidFrame)
    );
}

#[test]
fn oversized_serialization_writes_no_partial_frame() {
    let mut invalid = frame("1", HostMessage::Hello {});
    invalid.nonce = "x".repeat(MAX_FRAME_BYTES + 1);
    let mut output = vec![];
    assert_eq!(write_frame(&mut output, &invalid), Err(WireError::TooLarge));
    assert!(output.is_empty());
}
#[test]
fn schema_is_narrow_unknown_fields_and_arbitrary_actions_are_rejected() {
    let valid = serde_json::to_value(frame("1", HostMessage::Hello {})).unwrap();
    for body in [
        serde_json::json!({"kind":"hello","command":"whoami"}),
        serde_json::json!({"kind":"execute","path":"untrusted"}),
    ] {
        let mut value = valid.clone();
        value["body"] = body;
        assert!(serde_json::from_value::<Envelope<HostMessage>>(value).is_err());
    }
    let mut value = valid;
    value["token"] = serde_json::json!("untrusted");
    assert!(serde_json::from_value::<Envelope<HostMessage>>(value).is_err());
    assert!(
        serde_json::from_value::<HostReply>(
            serde_json::json!({"kind":"action","action":{"kind":"open_stats","path":"untrusted"}})
        )
        .is_err()
    );
}
#[test]
fn wrong_version_nonce_instance_and_missing_handshake_close_the_session() {
    for alteration in 0..4 {
        let mut host = session();
        let mut message = frame("1", HostMessage::Hello {});
        match alteration {
            0 => message.protocol_version = 2,
            1 => message.nonce = "b".repeat(64),
            2 => message.host_instance_id = "other-host".into(),
            _ => message.body = HostMessage::Heartbeat {},
        }
        assert!(host.apply(message).is_err());
        assert_eq!(
            host.apply(frame("2", HostMessage::Hello {})).err(),
            Some(WireError::Closed)
        );
        assert!(host.view().is_none());
    }
}
#[test]
fn privacy_barrier_clears_before_ack_and_rejects_old_or_sensitive_frames() {
    let mut host = session();
    host.apply(frame("1", HostMessage::Hello {})).unwrap();
    host.apply(frame(
        "2",
        HostMessage::Privacy {
            settings_revision: number("1"),
            enabled: false,
        },
    ))
    .unwrap();
    host.apply(frame(
        "3",
        HostMessage::Snapshot {
            view: Box::new(view(false, "1")),
        },
    ))
    .unwrap();
    assert!(host.view().is_some());
    assert!(matches!(
        host.apply(frame(
            "4",
            HostMessage::Privacy {
                settings_revision: number("2"),
                enabled: true
            }
        ))
        .unwrap(),
        HostReply::PrivacyApplied { enabled: true }
    ));
    assert!(host.view().is_none());
    host.apply(frame(
        "5",
        HostMessage::Snapshot {
            view: Box::new(view(true, "2")),
        },
    ))
    .unwrap();
    assert!(host.view().unwrap().costs.is_empty());
    assert_eq!(
        host.apply(frame(
            "6",
            HostMessage::Snapshot {
                view: Box::new(view(false, "1"))
            }
        ))
        .err(),
        Some(WireError::PrivacyViolation)
    );
    assert!(host.view().is_none());
    let mut sensitive = view(true, "3");
    sensitive.scope_label = Some("SYNTHETIC PRIVATE NAME".into());
    assert_eq!(sensitive.validate(), Err(WireError::PrivacyViolation));
}
#[test]
fn revisions_sequences_shutdown_and_disconnection_cannot_reuse_old_display() {
    let mut host = session();
    host.apply(frame("1", HostMessage::Hello {})).unwrap();
    host.apply(frame(
        "2",
        HostMessage::Snapshot {
            view: Box::new(view(true, "9007199254740993")),
        },
    ))
    .unwrap();
    assert_eq!(
        host.apply(frame(
            "3",
            HostMessage::Privacy {
                settings_revision: number("9007199254740992"),
                enabled: false
            }
        ))
        .err(),
        Some(WireError::OutOfOrder)
    );
    assert!(host.view().is_none());
    let mut host = session();
    host.apply(frame("1", HostMessage::Hello {})).unwrap();
    assert_eq!(
        host.apply(frame("1", HostMessage::Heartbeat {})).err(),
        Some(WireError::OutOfOrder)
    );
    let mut host = session();
    host.apply(frame("1", HostMessage::Hello {})).unwrap();
    assert!(matches!(
        host.apply(frame("2", HostMessage::Shutdown {})).unwrap(),
        HostReply::Stopped {}
    ));
    assert_eq!(
        host.apply(frame("3", HostMessage::Hello {})).err(),
        Some(WireError::Closed)
    );
}
#[test]
fn quota_metadata_cannot_smuggle_invalid_percentages_or_duplicate_windows() {
    let mut display = view(false, "1");
    let window = token_pulse_core::protocol::QuotaWindow {
        window_id: "weekly".into(),
        duration_mins: Some(10080),
        used_percent: Some(100.0),
        remaining_percent: Some(0.0),
        resets_at_ms: None,
    };
    display.quota = Some(HostQuota {
        connection_epoch: "synthetic-account".into(),
        revision: number("1"),
        state: QuotaState::Ready,
        limit_label: None,
        fetched_at_ms: None,
        windows: vec![window.clone()],
    });
    display.validate().unwrap();
    // The provider preserves finite usedPercent and clamps remaining, including over-limit use.
    display.quota.as_mut().unwrap().windows[0].used_percent = Some(120.0);
    display.validate().unwrap();
    display.quota.as_mut().unwrap().windows[0].remaining_percent = Some(f64::NAN);
    assert_eq!(display.validate(), Err(WireError::InvalidFrame));
    display.quota.as_mut().unwrap().windows = vec![window.clone(), window];
    assert_eq!(display.validate(), Err(WireError::InvalidFrame));
}

#[test]
fn projection_keeps_unknown_zero_and_exact_consumption_while_redacting_private_fields() {
    use token_pulse_core::{mini::MiniUsageSnapshot, protocol::QuotaSnapshot};
    let unknown = serde_json::json!({"value":null,"covered_total_tokens":"0","complete":false});
    let mut usage: MiniUsageSnapshot = serde_json::from_value(serde_json::json!({
        "meta":{"snapshot_id":"synthetic","data_revision":"9007199254740993","price_revision":"3","generated_at_ms":1000,"parser_versions":[],"accounting_versions":[],"display_timezone":"UTC"},
        "settings_revision":"2","mini_scope":{"kind":"today_all_sources"},"scope_display_name":"SYNTHETIC PRIVATE SCOPE","range":{"start_ms":0,"end_ms":1001,"timezone":"UTC"},
        "usage":{"total_tokens":"0","input_total":unknown,"noncached_input":unknown,"cached_input":unknown,"output_total":unknown,"reasoning_output":unknown,"session_count":"0","usage_event_count":"0","reliable_turn_count":null,"reliable_turns_complete":false},
        "pricing":{"redacted":false,"basis":{"mode":"event_time"},"currencies":[{"currency":"USD","estimated_cost":"0.000000000000001","priced_total_tokens":"0"}],"priced_total_tokens":"0","unpriced_total_tokens":"0","reasons":[],"calculating":false},
        "coverage":{"state":"unknown","pending_observation_count":"0","unattributed_observation_count":"0","unattributed_total_tokens":null,"pending_file_count":"1","source_issues":[],"format_issues":[],"breakdown_complete":false}
    })).unwrap();
    let quota: QuotaSnapshot = serde_json::from_value(serde_json::json!({
        "connection_epoch":"synthetic-account","quota_revision":"9007199254740993","state":"ready","selected_limit_id":"codex","available_limits":[{"limit_id":"codex","display_name":"SYNTHETIC PRIVATE BUCKET"}],"fetched_at_ms":1000,"last_attempt_at_ms":1000,"error_code":null,
        "windows":[{"window_id":"primary","duration_mins":10080,"used_percent":100,"remaining_percent":0,"resets_at_ms":null}]
    })).unwrap();
    let projected = TaskbarView::from_snapshots(&usage, &quota, false);
    projected.validate().unwrap();
    assert!(projected.total_tokens.is_none());
    assert!(projected.input_tokens.is_none());
    assert_eq!(
        projected.quota.unwrap().windows[0].remaining_percent,
        Some(0.0)
    );
    usage.coverage.state = CoverageState::Complete;
    assert_eq!(
        TaskbarView::from_snapshots(&usage, &quota, false)
            .total_tokens
            .unwrap()
            .as_str(),
        "0"
    );
    usage.usage.usage_event_count = number("1");
    usage.usage.total_tokens = number("9007199254740993");
    let hidden = TaskbarView::from_snapshots(&usage, &quota, true);
    hidden.validate().unwrap();
    let json = serde_json::to_string(&hidden).unwrap();
    assert!(json.contains("9007199254740993"));
    assert!(!json.contains("SYNTHETIC PRIVATE"));
    assert!(hidden.costs.is_empty());
    assert!(hidden.quota.is_none());
}
