//! Sequence proofs preserve source order. Fingerprints retrieve candidates; they never prove identity.
use crate::domain::{UsageObservation, UsageVector, VerifiedRequestIdentity};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageSignature {
    pub event_time_ms: Option<i64>,
    pub request_identity: Option<VerifiedRequestIdentity>,
    pub stream_hint: Option<String>,
    pub last: Option<UsageVector>,
    pub cumulative: Option<UsageVector>,
    pub explicit_episode_start: bool,
}
impl From<&UsageObservation> for UsageSignature {
    fn from(record: &UsageObservation) -> Self {
        Self {
            event_time_ms: record.event_time_ms,
            request_identity: record.request_identity.clone(),
            stream_hint: record.stream_hint.clone(),
            last: record.last,
            cumulative: record.cumulative,
            explicit_episode_start: record.explicit_episode_start,
        }
    }
}
impl UsageSignature {
    pub fn candidate_fingerprint(&self) -> String {
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(self).expect("typed signature serializes"))
        )
    }
    fn credible(&self) -> bool {
        self.event_time_ms.is_some()
            && [self.last, self.cumulative]
                .into_iter()
                .flatten()
                .all(|v| v.validated_total().is_ok())
            && [self.last, self.cumulative]
                .into_iter()
                .flatten()
                .any(|v| v.validated_total().is_ok_and(|t| t.is_some()))
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceIdentity {
    pub provider_namespace: String,
    pub provider_session_id: String,
    pub created_at_ms: Option<i64>,
    pub parent_provider_id: Option<String>,
}
pub struct SessionSequence<'a> {
    pub identity: &'a SequenceIdentity,
    pub records: &'a [UsageSignature],
    pub starts_at_session_head: bool,
    /// True only after all records through a pinned file size have been scanned.
    pub scanned_to_upper_bound: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceDecision {
    Unrelated,
    IdentityConflict,
    PendingIncomplete,
    PendingInsufficientEvidence,
    PendingMultipleCandidates,
    Mirror {
        aligned_prefix: usize,
        incoming_continuation: usize,
    },
    Inherited {
        aligned_prefix: usize,
        child_continuation: usize,
    },
}

pub fn align_mirror(
    reference: &SessionSequence<'_>,
    incoming: &SessionSequence<'_>,
) -> SequenceDecision {
    let a = reference.identity;
    let b = incoming.identity;
    if a.provider_namespace != b.provider_namespace
        || a.provider_session_id != b.provider_session_id
    {
        return SequenceDecision::Unrelated;
    }
    if matches!((a.created_at_ms,b.created_at_ms),(Some(a),Some(b)) if a!=b)
        || matches!((&a.parent_provider_id,&b.parent_provider_id),(Some(a),Some(b)) if a!=b)
    {
        return SequenceDecision::IdentityConflict;
    }
    if a.parent_provider_id != b.parent_provider_id {
        return SequenceDecision::PendingInsufficientEvidence;
    }
    if !reference.starts_at_session_head || !incoming.starts_at_session_head {
        return SequenceDecision::PendingIncomplete;
    }
    let prefix = common_prefix(reference.records, incoming.records);
    let shorter = reference.records.len().min(incoming.records.len());
    if prefix < shorter {
        return SequenceDecision::IdentityConflict;
    }
    if prefix == 0 {
        return SequenceDecision::PendingIncomplete;
    }
    let strong_header = a.created_at_ms.is_some() && a.created_at_ms == b.created_at_ms;
    if !prefix_is_proven(&incoming.records[..prefix], strong_header) {
        return SequenceDecision::PendingInsufficientEvidence;
    }
    let continuation = incoming.records.len().saturating_sub(prefix);
    if continuation > 0 && !reference.scanned_to_upper_bound {
        return SequenceDecision::PendingIncomplete;
    }
    SequenceDecision::Mirror {
        aligned_prefix: prefix,
        incoming_continuation: continuation,
    }
}

pub fn align_lineage(
    parents: &[SessionSequence<'_>],
    child: &SessionSequence<'_>,
) -> SequenceDecision {
    let Some(parent_id) = child.identity.parent_provider_id.as_ref() else {
        return SequenceDecision::Unrelated;
    };
    if parent_id == &child.identity.provider_session_id {
        return SequenceDecision::IdentityConflict;
    }
    let candidates: Vec<_> = parents
        .iter()
        .filter(|p| {
            p.identity.provider_namespace == child.identity.provider_namespace
                && &p.identity.provider_session_id == parent_id
        })
        .collect();
    if candidates.len() > 1 {
        return SequenceDecision::PendingMultipleCandidates;
    }
    let Some(parent) = candidates.first() else {
        return SequenceDecision::PendingIncomplete;
    };
    if !parent.starts_at_session_head
        || !child.starts_at_session_head
        || !parent.scanned_to_upper_bound
    {
        return SequenceDecision::PendingIncomplete;
    }
    let prefix = common_prefix(parent.records, child.records);
    // The explicit, uniquely resolved parent relation strengthens an exact ordered prefix.
    if prefix == 0 || !prefix_is_proven(&child.records[..prefix], true) {
        return SequenceDecision::PendingInsufficientEvidence;
    }
    if prefix == child.records.len() && !child.scanned_to_upper_bound {
        return SequenceDecision::PendingIncomplete;
    }
    SequenceDecision::Inherited {
        aligned_prefix: prefix,
        child_continuation: child.records.len() - prefix,
    }
}

fn common_prefix(a: &[UsageSignature], b: &[UsageSignature]) -> usize {
    a.iter().zip(b).take_while(|(a, b)| a == b).count()
}
fn prefix_is_proven(prefix: &[UsageSignature], strong_identity: bool) -> bool {
    if !prefix.iter().all(UsageSignature::credible) {
        return false;
    }
    if strong_identity {
        return true;
    }
    if prefix.iter().any(|s| {
        s.request_identity
            .as_ref()
            .is_some_and(|id| !id.namespace.is_empty() && !id.request_id.is_empty())
    }) {
        return true;
    }
    // Without a creation-time identity, require ordered full-vector counter progression.
    prefix.windows(2).any(
        |pair| match (pair[0].cumulative, pair[1].cumulative, pair[1].last) {
            (Some(before), Some(after), Some(last)) => {
                let (Some(bi), Some(bo), Some(ai), Some(ao), Some(li), Some(lo)) = (
                    before.input_total,
                    before.output_total,
                    after.input_total,
                    after.output_total,
                    last.input_total,
                    last.output_total,
                ) else {
                    return false;
                };
                bi.checked_add(li) == Some(ai)
                    && bo.checked_add(lo) == Some(ao)
                    && (li > 0 || lo > 0)
                    && component_progression(
                        before.cached_input,
                        after.cached_input,
                        last.cached_input,
                    )
                    && component_progression(
                        before.cache_write_input,
                        after.cache_write_input,
                        last.cache_write_input,
                    )
                    && component_progression(
                        before.reasoning_output,
                        after.reasoning_output,
                        last.reasoning_output,
                    )
                    && component_progression(
                        before.reported_total,
                        after.reported_total,
                        last.reported_total,
                    )
            }
            _ => false,
        },
    )
}
fn component_progression(before: Option<i64>, after: Option<i64>, last: Option<i64>) -> bool {
    match (before, after, last) {
        (None, None, None) => true,
        (Some(b), Some(a), Some(l)) => b.checked_add(l) == Some(a),
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestIdentityDecision {
    Unrelated,
    Duplicate,
    Conflict,
    InsufficientEvidence,
}
pub fn verify_request_identity(
    reference: &UsageObservation,
    incoming: &UsageObservation,
) -> RequestIdentityDecision {
    let (Some(a), Some(b)) = (&reference.request_identity, &incoming.request_identity) else {
        return RequestIdentityDecision::Unrelated;
    };
    if reference.session_key != incoming.session_key
        || a != b
        || a.namespace.is_empty()
        || a.request_id.is_empty()
    {
        return RequestIdentityDecision::Unrelated;
    }
    match (reference.last, incoming.last) {
        (Some(a), Some(b)) if a == b && a.validated_total().is_ok_and(|t| t.is_some()) => {
            RequestIdentityDecision::Duplicate
        }
        (Some(_), Some(_)) => RequestIdentityDecision::Conflict,
        _ => RequestIdentityDecision::InsufficientEvidence,
    }
}
