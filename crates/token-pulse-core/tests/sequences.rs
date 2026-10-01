use serde::Deserialize;
use std::collections::BTreeMap;
use token_pulse_core::accounting::*;
use token_pulse_core::{domain::*, sequence::*};

#[derive(Deserialize)]
struct Fixture {
    signatures: BTreeMap<String, Signature>,
    mirror_cases: Vec<MirrorCase>,
    lineage_cases: Vec<LineageCase>,
}
#[derive(Deserialize)]
struct Signature {
    time: i64,
    last: [i64; 5],
    total: [i64; 5],
}
#[derive(Deserialize)]
struct MirrorCase {
    name: String,
    reference: Vec<String>,
    incoming: Vec<String>,
    expected: String,
    prefix: Option<usize>,
    continuation: Option<usize>,
    #[serde(default)]
    reference_incomplete: bool,
    #[serde(default)]
    unknown_creation: bool,
}
#[derive(Deserialize)]
struct LineageCase {
    name: String,
    parent: Vec<String>,
    child: Vec<String>,
    expected: String,
    prefix: Option<usize>,
    continuation: Option<usize>,
    #[serde(default)]
    missing_parent: bool,
    #[serde(default)]
    multiple_parents: bool,
}
fn vector(v: [i64; 5]) -> UsageVector {
    UsageVector {
        input_total: Some(v[0]),
        cached_input: Some(v[1]),
        output_total: Some(v[2]),
        reasoning_output: Some(v[3]),
        reported_total: Some(v[4]),
    }
}
fn signatures(f: &Fixture, keys: &[String]) -> Vec<UsageSignature> {
    keys.iter()
        .map(|key| {
            let s = &f.signatures[key];
            UsageSignature {
                event_time_ms: Some(s.time),
                request_identity: None,
                stream_hint: None,
                last: Some(vector(s.last)),
                cumulative: Some(vector(s.total)),
                explicit_episode_start: false,
            }
        })
        .collect()
}
fn identity(id: &str) -> SequenceIdentity {
    SequenceIdentity {
        provider_namespace: "codex".into(),
        provider_session_id: id.into(),
        created_at_ms: Some(1),
        parent_provider_id: None,
    }
}
fn sequence<'a>(
    identity: &'a SequenceIdentity,
    records: &'a [UsageSignature],
) -> SessionSequence<'a> {
    SessionSequence {
        identity,
        records,
        starts_at_session_head: true,
        scanned_to_upper_bound: true,
    }
}
fn assert_decision(
    actual: SequenceDecision,
    expected: &str,
    prefix: Option<usize>,
    continuation: Option<usize>,
    name: &str,
) {
    let expected = match expected {
        "mirror" => SequenceDecision::Mirror {
            aligned_prefix: prefix.unwrap(),
            incoming_continuation: continuation.unwrap(),
        },
        "inherited" => SequenceDecision::Inherited {
            aligned_prefix: prefix.unwrap(),
            child_continuation: continuation.unwrap(),
        },
        "identity_conflict" => SequenceDecision::IdentityConflict,
        "pending_incomplete" => SequenceDecision::PendingIncomplete,
        "pending_insufficient" => SequenceDecision::PendingInsufficientEvidence,
        "pending_multiple" => SequenceDecision::PendingMultipleCandidates,
        _ => panic!("unknown expectation"),
    };
    assert_eq!(actual, expected, "{name}");
}
#[test]
fn independent_sequence_fixture_proves_mirrors_and_inheritance_or_keeps_pending() {
    let f: Fixture =
        serde_json::from_str(include_str!("../../../fixtures/accounting-sequences.json")).unwrap();
    for case in &f.mirror_cases {
        let mut id = identity("session");
        if case.unknown_creation {
            id.created_at_ms = None;
        }
        let a = signatures(&f, &case.reference);
        let b = signatures(&f, &case.incoming);
        let mut reference = sequence(&id, &a);
        reference.scanned_to_upper_bound = !case.reference_incomplete;
        assert_decision(
            align_mirror(&reference, &sequence(&id, &b)),
            &case.expected,
            case.prefix,
            case.continuation,
            &case.name,
        );
    }
    for case in &f.lineage_cases {
        let parent_id = identity("parent");
        let mut child_id = identity("child");
        child_id.parent_provider_id = Some("parent".into());
        let a = signatures(&f, &case.parent);
        let b = signatures(&f, &case.child);
        let mut parents = vec![];
        if !case.missing_parent {
            parents.push(sequence(&parent_id, &a));
        }
        if case.multiple_parents {
            parents.push(sequence(&parent_id, &a));
        }
        assert_decision(
            align_lineage(&parents, &sequence(&child_id, &b)),
            &case.expected,
            case.prefix,
            case.continuation,
            &case.name,
        );
    }
}
fn usage() -> UsageObservation {
    UsageObservation {
        physical_position: PhysicalPosition {
            file_generation_id: "a".into(),
            byte_offset: 0,
            byte_end: 1,
        },
        session_key: "session".into(),
        event_time_ms: Some(123),
        request_identity: Some(VerifiedRequestIdentity {
            namespace: "verified-layout".into(),
            request_id: "request-1".into(),
        }),
        stream_hint: None,
        last: Some(vector([100, 60, 10, 2, 110])),
        cumulative: Some(vector([100, 60, 10, 2, 110])),
        effective_metadata: EffectiveMetadata::default(),
        explicit_episode_start: false,
        model_context_window: None,
    }
}
#[test]
fn fingerprints_exclude_correctable_metadata_and_physical_locations() {
    let a = usage();
    let mut b = a.clone();
    b.physical_position.file_generation_id = "mirror".into();
    b.effective_metadata.model = Some("corrected".into());
    b.effective_metadata.cwd = Some("new-path".into());
    assert_eq!(UsageSignature::from(&a), UsageSignature::from(&b));
    assert_eq!(
        UsageSignature::from(&a).candidate_fingerprint(),
        UsageSignature::from(&b).candidate_fingerprint()
    );
    b.last.as_mut().unwrap().cached_input = Some(50);
    assert_ne!(
        UsageSignature::from(&a).candidate_fingerprint(),
        UsageSignature::from(&b).candidate_fingerprint()
    );
}
#[test]
fn stable_request_identity_is_scoped_and_conflicts_are_not_swallowed() {
    let a = usage();
    let mut b = a.clone();
    b.event_time_ms = Some(999);
    assert_eq!(
        verify_request_identity(&a, &b),
        RequestIdentityDecision::Duplicate
    );
    b.last.as_mut().unwrap().cached_input = Some(50);
    assert_eq!(
        verify_request_identity(&a, &b),
        RequestIdentityDecision::Conflict
    );
    b = a.clone();
    b.session_key = "other".into();
    assert_eq!(
        verify_request_identity(&a, &b),
        RequestIdentityDecision::Unrelated
    );
    b = a.clone();
    b.request_identity.as_mut().unwrap().namespace = "other".into();
    assert_eq!(
        verify_request_identity(&a, &b),
        RequestIdentityDecision::Unrelated
    );
    b = a.clone();
    b.last = None;
    assert_eq!(
        verify_request_identity(&a, &b),
        RequestIdentityDecision::InsufficientEvidence
    );
}
#[test]
fn absent_head_unknown_time_and_conflicting_creation_do_not_prove_identity() {
    let signature = UsageSignature::from(&usage());
    let records = [signature];
    let id = identity("session");
    let a = sequence(&id, &records);
    let mut b = sequence(&id, &records);
    b.starts_at_session_head = false;
    assert_eq!(align_mirror(&a, &b), SequenceDecision::PendingIncomplete);
    let mut conflicting = id.clone();
    conflicting.created_at_ms = Some(2);
    assert_eq!(
        align_mirror(&a, &sequence(&conflicting, &records)),
        SequenceDecision::IdentityConflict
    );
    let mut unknown = records.clone();
    unknown[0].event_time_ms = None;
    assert_eq!(
        align_mirror(&sequence(&id, &unknown), &sequence(&id, &unknown)),
        SequenceDecision::PendingInsufficientEvidence
    );
}

#[test]
fn aligned_parent_prefix_seeds_child_without_billing_inherited_usage() {
    let f: Fixture =
        serde_json::from_str(include_str!("../../../fixtures/accounting-sequences.json")).unwrap();
    let parent_id = identity("parent");
    let mut child_id = identity("child");
    child_id.parent_provider_id = Some("parent".into());
    let parent_signatures = signatures(&f, &["a".into(), "b".into()]);
    let child_signatures = signatures(&f, &["a".into(), "c".into()]);
    assert_eq!(
        align_lineage(
            &[sequence(&parent_id, &parent_signatures)],
            &sequence(&child_id, &child_signatures)
        ),
        SequenceDecision::Inherited {
            aligned_prefix: 1,
            child_continuation: 1
        }
    );
    let from_signature = |signature: &UsageSignature, session: &str, index: u64| UsageObservation {
        physical_position: PhysicalPosition {
            file_generation_id: session.into(),
            byte_offset: index,
            byte_end: index + 1,
        },
        session_key: session.into(),
        event_time_ms: signature.event_time_ms,
        request_identity: signature.request_identity.clone(),
        stream_hint: signature.stream_hint.clone(),
        last: signature.last,
        cumulative: signature.cumulative,
        effective_metadata: EffectiveMetadata::default(),
        explicit_episode_start: false,
        model_context_window: None,
    };
    let parent_a = from_signature(&parent_signatures[0], "parent", 0);
    let parent_first = account(
        &AccountingState::new("parent".into()),
        &parent_a,
        &AccountingEvidence {
            independent_new_stream: true,
            ..Default::default()
        },
    );
    let inherited_baseline = parent_first.state.streams.values().next().unwrap().clone();
    let parent_second = account(
        &parent_first.state,
        &from_signature(&parent_signatures[1], "parent", 1),
        &AccountingEvidence::default(),
    );
    let child_first = account(
        &AccountingState::new("child".into()),
        &from_signature(&child_signatures[0], "child", 0),
        &AccountingEvidence {
            lineage: LineageEvidence::VerifiedInherited {
                reference: Box::new(CanonicalReference {
                    event_id: "parent-a".into(),
                    usage: parent_a.last.unwrap(),
                    observation: UsageSignature::from(&parent_a),
                }),
                baseline: Some(Box::new(inherited_baseline)),
            },
            ..Default::default()
        },
    );
    assert_eq!(child_first.method, CalculationMethod::Inherited);
    assert!(child_first.event_usage.is_none());
    let child_second = account(
        &child_first.state,
        &from_signature(&child_signatures[1], "child", 1),
        &AccountingEvidence::default(),
    );
    assert_eq!(child_second.method, CalculationMethod::LastWithBaseline);
    // Independent expected canonical events: parent a=110, parent b=25, child c=13.
    let events = [
        parent_first.event_usage.unwrap(),
        parent_second.event_usage.unwrap(),
        child_second.event_usage.unwrap(),
    ];
    assert_eq!(
        events.map(|v| v.validated_total().unwrap().unwrap()),
        [110, 25, 13]
    );
}

#[test]
fn cumulative_only_mirror_proof_keeps_canonical_delta_and_request_replay_can_change_time() {
    let mut a = usage();
    a.request_identity = None;
    a.stream_hint = Some("verified".into());
    a.last = None;
    let first = account(
        &AccountingState::new("session".into()),
        &a,
        &AccountingEvidence::default(),
    );
    let mut b = a.clone();
    b.physical_position.byte_offset = 1;
    b.physical_position.byte_end = 2;
    b.cumulative = Some(vector([120, 70, 15, 3, 135]));
    let canonical = account(&first.state, &b, &AccountingEvidence::default());
    assert_eq!(
        canonical.event_usage.unwrap().validated_total(),
        Ok(Some(25))
    );
    let mut mirror = b.clone();
    mirror.physical_position.file_generation_id = "mirror".into();
    let duplicate = account(
        &canonical.state,
        &mirror,
        &AccountingEvidence {
            verified_duplicate: Some(CanonicalReference {
                event_id: "delta".into(),
                usage: canonical.event_usage.unwrap(),
                observation: UsageSignature::from(&b),
            }),
            ..Default::default()
        },
    );
    assert_eq!(duplicate.method, CalculationMethod::VerifiedDuplicate);
    assert_eq!(duplicate.state, canonical.state);
    assert!(duplicate.event_usage.is_none());
    let request = usage();
    let mut replay = request.clone();
    replay.event_time_ms = Some(9999);
    let result = account(
        &AccountingState::new("session".into()),
        &replay,
        &AccountingEvidence {
            verified_duplicate: Some(CanonicalReference {
                event_id: "request".into(),
                usage: request.last.unwrap(),
                observation: UsageSignature::from(&request),
            }),
            ..Default::default()
        },
    );
    assert_eq!(result.method, CalculationMethod::VerifiedDuplicate);
}
