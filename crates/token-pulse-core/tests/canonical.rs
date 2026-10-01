use token_pulse_core::{canonical::*, domain::UsageVector, sequence::*};
fn record(time: i64, last: i64, total: i64) -> UsageSignature {
    let vector = |n| UsageVector {
        input_total: Some(n),
        cached_input: Some(0),
        output_total: Some(0),
        reasoning_output: Some(0),
        reported_total: Some(n),
    };
    UsageSignature {
        event_time_ms: Some(time),
        request_identity: None,
        stream_hint: None,
        last: Some(vector(last)),
        cumulative: Some(vector(total)),
        explicit_episode_start: false,
    }
}
fn physical(key: &str, owner: &str, records: Vec<UsageSignature>) -> PhysicalSequence {
    PhysicalSequence {
        sequence_key: key.into(),
        owner_session_key: owner.into(),
        identity: SequenceIdentity {
            provider_namespace: "codex".into(),
            provider_session_id: "provider-session".into(),
            created_at_ms: Some(1),
            parent_provider_id: None,
        },
        records,
        starts_at_session_head: true,
        scanned_to_upper_bound: true,
        previously_canonical: false,
        has_trusted_history: false,
    }
}
#[test]
fn primary_metadata_and_unique_longer_continuation_have_independently_expected_origins() {
    let a = record(1, 110, 110);
    let b = record(2, 25, 135);
    let c = record(3, 13, 148);
    let mut primary = physical("z-primary", "stable-session", vec![a.clone(), b.clone()]);
    primary.previously_canonical = true;
    let mirror = physical("a-mirror", "incoming-alias", vec![a, b, c]);
    let result = plan(&[mirror, primary]).unwrap();
    let group = &result.groups[0];
    assert_eq!(group.session_key, "stable-session");
    assert_eq!(group.record_count, 3);
    assert_eq!(group.alias_session_keys, ["incoming-alias"]);
    assert_eq!(group.member_sequence_keys, ["a-mirror", "z-primary"]);
    assert_eq!(
        (0..4)
            .map(|n| group.origin_sequence_for(n))
            .collect::<Vec<_>>(),
        [Some("z-primary"), Some("z-primary"), Some("a-mirror"), None]
    );
    assert!(result.isolated.is_empty());
}
#[test]
fn two_different_extensions_cannot_be_selected_by_arrival_time_or_length() {
    let a = record(1, 110, 110);
    let mut primary = physical("primary", "stable", vec![a.clone()]);
    primary.previously_canonical = true;
    let one = physical("one", "one", vec![a.clone(), record(2, 25, 135)]);
    let two = physical("two", "two", vec![a, record(2, 30, 140), record(3, 5, 145)]);
    let result = plan(&[primary, one, two]).unwrap();
    assert_eq!(result.groups[0].record_count, 1);
    assert_eq!(result.groups[0].member_sequence_keys, ["primary"]);
    assert_eq!(result.isolated.len(), 2);
    assert!(
        result
            .isolated
            .iter()
            .all(|s| s.reason == SequenceDecision::IdentityConflict)
    );
}
#[test]
fn incomplete_predecessor_unknown_time_and_creation_conflict_are_explicitly_isolated() {
    let a = record(1, 110, 110);
    let mut primary = physical("primary", "stable", vec![a.clone()]);
    primary.previously_canonical = true;
    primary.scanned_to_upper_bound = false;
    let extension = physical(
        "extension",
        "extension",
        vec![a.clone(), record(2, 25, 135)],
    );
    let mut no_time = physical("no-time", "no-time", vec![a.clone()]);
    no_time.records[0].event_time_ms = None;
    let mut conflict = physical("conflict", "conflict", vec![a]);
    conflict.identity.created_at_ms = Some(2);
    let result = plan(&[primary, extension, no_time, conflict]).unwrap();
    assert_eq!(result.groups[0].record_count, 1);
    assert!(result.groups[0].alias_session_keys.is_empty());
    assert_eq!(
        result.isolated.iter().map(|s| s.reason).collect::<Vec<_>>(),
        [
            SequenceDecision::IdentityConflict,
            SequenceDecision::PendingIncomplete,
            SequenceDecision::IdentityConflict
        ]
    );
}
#[test]
fn anonymous_sessions_and_namespaces_do_not_form_a_shared_bucket_and_duplicate_keys_fail() {
    let mut a = physical("a", "a", vec![record(1, 5, 5)]);
    a.identity.provider_session_id.clear();
    let mut b = physical("b", "b", vec![record(1, 5, 5)]);
    b.identity.provider_session_id.clear();
    let mut c = physical("c", "c", vec![record(1, 5, 5)]);
    c.identity.provider_namespace = "another-provider".into();
    let mut d = physical("d", "d", vec![record(1, 5, 5)]);
    d.identity.provider_session_id = "a".into();
    let result = plan(&[a, b, c, d]).unwrap();
    assert_eq!(result.groups.len(), 4);
    assert!(
        result
            .groups
            .iter()
            .all(|g| g.alias_session_keys.is_empty())
    );
    assert!(plan(&[physical("same", "a", vec![]), physical("same", "b", vec![])]).is_err());
}
