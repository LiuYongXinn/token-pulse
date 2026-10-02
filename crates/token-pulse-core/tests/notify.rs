use token_pulse_core::notify::{
    MAX_NOTIFY_PAYLOAD_BYTES, NotifyParseError, parse_codex_notification,
};

#[test]
fn documented_payload_keeps_only_validated_ids_and_drops_private_nested_fields() {
    let hint = parse_codex_notification(br#"{
      "type":"agent-turn-complete","thread-id":"0193a1b2-0000-4000-8000-123456789abc","turn-id":"turn_01",
      "cwd":"E:\\PRIVATE_PROJECT","input-messages":["PRIVATE_PROMPT",{"text":"NESTED_PRIVATE_BODY"}],
      "last-assistant-message":"PRIVATE_RESPONSE","auth":{"token":"PRIVATE_SECRET"},
      "total_tokens":9999999,"estimated_cost":"123.45","future":[null,true,{},[1,2]]
    }"#).unwrap().unwrap();
    assert_eq!(hint.thread_id(), "0193a1b2-0000-4000-8000-123456789abc");
    assert_eq!(hint.turn_id(), Some("turn_01"));
    let serialized = serde_json::to_string(&hint).unwrap();
    assert_eq!(
        serialized,
        r#"{"thread_id":"0193a1b2-0000-4000-8000-123456789abc","turn_id":"turn_01"}"#
    );
    assert!(!format!("{hint:?}").contains("PRIVATE"));
}

#[test]
fn missing_turn_is_null_and_unsupported_events_never_produce_hints() {
    for payload in [
        br#"{"type":"agent-turn-complete","thread-id":"thread-1"}"#.as_slice(),
        br#"{"type":"agent-turn-complete","thread-id":"thread-1","turn-id":null}"#.as_slice(),
    ] {
        let hint = parse_codex_notification(payload).unwrap().unwrap();
        assert_eq!(hint.turn_id(), None);
        assert_eq!(
            serde_json::to_string(&hint).unwrap(),
            r#"{"thread_id":"thread-1","turn_id":null}"#
        );
    }
    for payload in [
        br#"{"type":"approval-requested","input-messages":["PRIVATE"]}"#.as_slice(),
        br#"{"type":"future-event","thread-id":"../../auth.json"}"#.as_slice(),
        br#"{"type":"","thread-id":"thread-1"}"#.as_slice(),
    ] {
        assert_eq!(parse_codex_notification(payload).unwrap(), None);
    }
}

#[test]
fn supported_notifications_require_bounded_safe_opaque_identifiers() {
    for id in [
        "",
        "../auth.json",
        "a/b",
        "a\\b",
        "a\nb",
        "a\0b",
        "a;echo",
        "a b",
        "线程",
        ".",
        "..",
        "-leading",
    ] {
        for field in ["thread-id", "turn-id"] {
            let mut payload =
                serde_json::json!({"type":"agent-turn-complete","thread-id":"thread-1"});
            payload[field] = id.into();
            assert_eq!(
                parse_codex_notification(serde_json::to_string(&payload).unwrap().as_bytes()),
                Err(NotifyParseError::InvalidIdentity)
            );
        }
    }
    let id = "a".repeat(128);
    let mut payload = serde_json::json!({"type":"agent-turn-complete","thread-id":id});
    assert!(
        parse_codex_notification(serde_json::to_string(&payload).unwrap().as_bytes())
            .unwrap()
            .is_some()
    );
    payload["thread-id"] = "a".repeat(129).into();
    assert_eq!(
        parse_codex_notification(serde_json::to_string(&payload).unwrap().as_bytes()),
        Err(NotifyParseError::InvalidIdentity)
    );
    for payload in [
        br#"{"type":"agent-turn-complete"}"#.as_slice(),
        br#"{"type":"agent-turn-complete","thread-id":null}"#.as_slice(),
    ] {
        assert_eq!(
            parse_codex_notification(payload),
            Err(NotifyParseError::InvalidIdentity)
        );
    }
}

#[test]
fn malformed_or_duplicate_allowlist_fields_return_payload_free_errors() {
    for payload in [
        br#"[]"#.as_slice(),
        br#"{}"#.as_slice(),
        br#"{"type":null}"#.as_slice(),
        br#"{"type":"agent-turn-complete","thread-id":123}"#.as_slice(),
        br#"{"type":"agent-turn-complete","type":"future-event","thread-id":"thread-1"}"#
            .as_slice(),
        br#"{"type":"agent-turn-complete","thread-id":"first","thread-id":"second"}"#.as_slice(),
        br#"{"type":"agent-turn-complete","thread-id":"thread-1","turn-id":"a","turn-id":"b"}"#
            .as_slice(),
        br#"{"type":"agent-turn-complete","thread-id":"thread-1","ignored":"PRIVATE"} trailing"#
            .as_slice(),
        b"\xffPRIVATE".as_slice(),
    ] {
        let error = parse_codex_notification(payload).unwrap_err();
        assert_eq!(error, NotifyParseError::MalformedPayload);
        assert_eq!(error.to_string(), "NOTIFY_PAYLOAD_INVALID");
        assert!(!format!("{error:?}").contains("PRIVATE"));
    }
}

#[test]
fn byte_cap_is_checked_before_parsing_and_unknown_large_content_is_not_retained() {
    let head = r#"{"type":"agent-turn-complete","thread-id":"thread-1","input-messages":[""#;
    let tail = r#""]}"#;
    let payload = format!(
        "{head}{}{tail}",
        "x".repeat(MAX_NOTIFY_PAYLOAD_BYTES - head.len() - tail.len())
    );
    assert_eq!(payload.len(), MAX_NOTIFY_PAYLOAD_BYTES);
    let hint = parse_codex_notification(payload.as_bytes())
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::to_string(&hint).unwrap(),
        r#"{"thread_id":"thread-1","turn_id":null}"#
    );
    let too_large = format!("{payload} ");
    assert_eq!(
        parse_codex_notification(too_large.as_bytes()),
        Err(NotifyParseError::PayloadTooLarge)
    );
    let invalid_large = vec![0xff; MAX_NOTIFY_PAYLOAD_BYTES + 1];
    assert_eq!(
        parse_codex_notification(&invalid_large),
        Err(NotifyParseError::PayloadTooLarge)
    );
}
