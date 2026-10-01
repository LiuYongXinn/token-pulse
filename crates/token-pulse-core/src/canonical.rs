//! Physical sequence planning. Equal IDs and fingerprints are insufficient to merge consumption.
use crate::{error::ErrorCode, sequence::*};
use std::collections::{BTreeMap, BTreeSet};

pub struct PhysicalSequence {
    pub sequence_key: String,
    /// Existing aliases are resolved by the trusted caller before planning.
    pub owner_session_key: String,
    pub identity: SequenceIdentity,
    pub records: Vec<UsageSignature>,
    pub starts_at_session_head: bool,
    pub scanned_to_upper_bound: bool,
    pub previously_canonical: bool,
    pub has_trusted_history: bool,
}
impl PhysicalSequence {
    fn view(&self) -> SessionSequence<'_> {
        SessionSequence {
            identity: &self.identity,
            records: &self.records,
            starts_at_session_head: self.starts_at_session_head,
            scanned_to_upper_bound: self.scanned_to_upper_bound,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalGroup {
    pub session_key: String,
    pub primary_sequence_key: String,
    pub longest_sequence_key: String,
    pub primary_record_count: usize,
    pub record_count: usize,
    pub member_sequence_keys: Vec<String>,
    pub alias_session_keys: Vec<String>,
}
impl CanonicalGroup {
    /// Metadata comes from the stable primary wherever possible, then the proven continuation.
    pub fn origin_sequence_for(&self, ordinal: usize) -> Option<&str> {
        if ordinal >= self.record_count {
            None
        } else if ordinal < self.primary_record_count {
            Some(&self.primary_sequence_key)
        } else {
            Some(&self.longest_sequence_key)
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsolatedSequence {
    pub sequence_key: String,
    pub session_key: String,
    pub reason: SequenceDecision,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalPlan {
    pub groups: Vec<CanonicalGroup>,
    pub isolated: Vec<IsolatedSequence>,
}
fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

pub fn plan(sequences: &[PhysicalSequence]) -> Result<CanonicalPlan, ErrorCode> {
    if sequences.len() > 32768 {
        return Err(ErrorCode::InvalidQuery);
    }
    let mut keys = BTreeSet::new();
    let mut buckets: BTreeMap<(String, bool, String), Vec<&PhysicalSequence>> = BTreeMap::new();
    for sequence in sequences {
        if !valid_id(&sequence.sequence_key)
            || !valid_id(&sequence.owner_session_key)
            || !keys.insert(&sequence.sequence_key)
        {
            return Err(ErrorCode::InvalidQuery);
        }
        // Unknown provider IDs never collect into one anonymous bucket.
        let anonymous = sequence.identity.provider_session_id.is_empty();
        let provider_id = if anonymous {
            sequence.sequence_key.clone()
        } else {
            sequence.identity.provider_session_id.clone()
        };
        buckets
            .entry((
                sequence.identity.provider_namespace.clone(),
                anonymous,
                provider_id,
            ))
            .or_default()
            .push(sequence);
    }
    let mut result = CanonicalPlan {
        groups: vec![],
        isolated: vec![],
    };
    for (_, mut bucket) in buckets {
        bucket.sort_by(|a, b| a.sequence_key.cmp(&b.sequence_key));
        let preferred = bucket
            .iter()
            .filter(|s| s.previously_canonical)
            .copied()
            .collect::<Vec<_>>();
        let historical = bucket
            .iter()
            .filter(|s| s.has_trusted_history)
            .copied()
            .collect::<Vec<_>>();
        let primary = if preferred.len() == 1 {
            preferred[0]
        } else if historical.len() == 1 {
            historical[0]
        } else {
            bucket[0]
        };
        let mut proven = vec![primary];
        for sequence in &bucket {
            if sequence.sequence_key == primary.sequence_key {
                continue;
            }
            let decision = align_mirror(&primary.view(), &sequence.view());
            if matches!(decision, SequenceDecision::Mirror { .. }) {
                proven.push(*sequence);
            } else {
                result.isolated.push(IsolatedSequence {
                    sequence_key: sequence.sequence_key.clone(),
                    session_key: sequence.owner_session_key.clone(),
                    reason: decision,
                });
            }
        }
        // Extensions of a short primary must also agree with each other in their overlap.
        let longest = *proven
            .iter()
            .max_by(|a, b| {
                a.records
                    .len()
                    .cmp(&b.records.len())
                    .then_with(|| b.sequence_key.cmp(&a.sequence_key))
            })
            .unwrap();
        let mut safe = vec![];
        for sequence in proven {
            if sequence.sequence_key == longest.sequence_key
                || sequence.sequence_key == primary.sequence_key
                || matches!(
                    align_mirror(&longest.view(), &sequence.view()),
                    SequenceDecision::Mirror { .. }
                )
            {
                safe.push(sequence);
            } else {
                result.isolated.push(IsolatedSequence {
                    sequence_key: sequence.sequence_key.clone(),
                    session_key: sequence.owner_session_key.clone(),
                    reason: SequenceDecision::IdentityConflict,
                });
            }
        }
        // A candidate that agrees with the short primary but conflicts with another continuation
        // cannot make either branch a trusted extension. Keep the primary and isolate all tails.
        let extension_conflict = result.isolated.iter().any(|isolated| {
            isolated.reason == SequenceDecision::IdentityConflict
                && bucket.iter().any(|s| {
                    s.sequence_key == isolated.sequence_key
                        && s.records.len() > primary.records.len()
                })
        });
        if extension_conflict {
            for sequence in &safe {
                if sequence.sequence_key != primary.sequence_key {
                    result.isolated.push(IsolatedSequence {
                        sequence_key: sequence.sequence_key.clone(),
                        session_key: sequence.owner_session_key.clone(),
                        reason: SequenceDecision::IdentityConflict,
                    });
                }
            }
            safe = vec![primary];
        }
        let longest = *safe
            .iter()
            .max_by(|a, b| {
                a.records
                    .len()
                    .cmp(&b.records.len())
                    .then_with(|| b.sequence_key.cmp(&a.sequence_key))
            })
            .unwrap();
        let mut aliases = safe
            .iter()
            .map(|s| s.owner_session_key.clone())
            .filter(|s| s != &primary.owner_session_key)
            .collect::<Vec<_>>();
        aliases.sort();
        aliases.dedup();
        let mut members = safe
            .iter()
            .map(|s| s.sequence_key.clone())
            .collect::<Vec<_>>();
        members.sort();
        result.groups.push(CanonicalGroup {
            session_key: primary.owner_session_key.clone(),
            primary_sequence_key: primary.sequence_key.clone(),
            longest_sequence_key: longest.sequence_key.clone(),
            primary_record_count: primary.records.len(),
            record_count: longest.records.len(),
            member_sequence_keys: members,
            alias_session_keys: aliases,
        });
    }
    result
        .groups
        .sort_by(|a, b| a.session_key.cmp(&b.session_key));
    result
        .isolated
        .sort_by(|a, b| a.sequence_key.cmp(&b.sequence_key));
    Ok(result)
}
