//! Deterministic inference: callers supply verified ordering/identity evidence, never clock heuristics.
use crate::{domain::*, error::ErrorCode, sequence::UsageSignature};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const MAX_STREAMS: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamBaseline {
    pub stream_key: String,
    pub episode_id: String,
    pub cumulative: UsageVector,
    pub last_snapshot: Option<UsageVector>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountingState {
    pub session_key: String,
    pub streams: BTreeMap<String, StreamBaseline>,
}
impl AccountingState {
    pub fn new(session_key: String) -> Self {
        Self {
            session_key,
            streams: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalculationMethod {
    PhysicalDuplicate,
    VerifiedDuplicate,
    Inherited,
    LineagePending,
    RepeatedSnapshot,
    LastWithBaseline,
    LastNewStream,
    CumulativeDelta,
    LastOnly,
    UnattributedAnchor,
    UnattributedUsage,
    EpisodeReset,
    AmbiguousUsage,
    InvalidUsage,
    StreamCapacityExceeded,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalReference {
    pub event_id: String,
    pub usage: UsageVector,
    pub observation: UsageSignature,
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LineageEvidence {
    #[default]
    Independent,
    Pending,
    VerifiedInherited {
        reference: Box<CanonicalReference>,
        baseline: Option<Box<StreamBaseline>>,
    },
}
/// These proofs come from the physical-position index and sequence/identity verifier.
/// Missing proof is false. A provider session ID, timestamp or model is never sufficient.
#[derive(Debug, Clone, Default)]
pub struct AccountingEvidence {
    pub physical_duplicate: bool,
    pub verified_duplicate: Option<CanonicalReference>,
    pub lineage: LineageEvidence,
    pub independent_new_stream: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestContext {
    pub observed_at_ms: Option<i64>,
    pub usage: UsageVector,
    pub model_context_window: Option<i64>,
    pub quality: ObservationQuality,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountingResult {
    pub state: AccountingState,
    pub method: CalculationMethod,
    pub quality: ObservationQuality,
    pub event_usage: Option<UsageVector>,
    pub context: Option<RequestContext>,
    pub stream_key: Option<String>,
    pub episode_id: Option<String>,
    pub candidate_stream_keys: Vec<String>,
    pub prior_anchor: Option<UsageVector>,
    pub canonical_event_id: Option<String>,
    pub error: Option<ErrorCode>,
}

pub fn account(
    state: &AccountingState,
    observation: &UsageObservation,
    evidence: &AccountingEvidence,
) -> AccountingResult {
    let mut result = AccountingResult {
        state: state.clone(),
        method: CalculationMethod::AmbiguousUsage,
        quality: ObservationQuality::Pending,
        event_usage: None,
        context: None,
        stream_key: None,
        episode_id: None,
        candidate_stream_keys: vec![],
        prior_anchor: None,
        canonical_event_id: None,
        error: Some(ErrorCode::AmbiguousUsage),
    };
    if observation.session_key != state.session_key
        || observation.physical_position.validate().is_err()
        || state.streams.len() > MAX_STREAMS
        || state.streams.iter().any(|(key, b)| {
            key != &b.stream_key
                || b.episode_id.is_empty()
                || b.cumulative.validated_total().is_err()
        })
    {
        result.method = CalculationMethod::InvalidUsage;
        result.error = Some(ErrorCode::InvalidQuery);
        return result;
    }
    if evidence.physical_duplicate {
        result.method = CalculationMethod::PhysicalDuplicate;
        result.quality = ObservationQuality::Duplicate;
        result.error = None;
        return result;
    }
    let last_valid = observation
        .last
        .filter(|v| v.validated_total().is_ok_and(|t| t.is_some()));
    if let Some(last) = last_valid {
        result.context = Some(RequestContext {
            observed_at_ms: observation.event_time_ms,
            usage: last,
            model_context_window: observation.model_context_window.filter(|&n| n > 0),
            quality: ObservationQuality::Pending,
        });
    }
    for vector in [observation.last, observation.cumulative]
        .into_iter()
        .flatten()
    {
        if let Err(error) = vector.validated_total() {
            result.method = CalculationMethod::InvalidUsage;
            result.error = Some(error);
            return result;
        }
    }
    if let Some(reference) = &evidence.verified_duplicate {
        // Both a verified request identity and an aligned mirror sequence may supply a reference.
        if reference_matches(reference, observation) {
            result.method = CalculationMethod::VerifiedDuplicate;
            result.quality = ObservationQuality::Duplicate;
            result.canonical_event_id = Some(reference.event_id.clone());
            result.error = None;
            if let Some(context) = &mut result.context {
                context.quality = ObservationQuality::Duplicate;
            }
        }
        return result;
    }
    match &evidence.lineage {
        LineageEvidence::Pending => {
            result.method = CalculationMethod::LineagePending;
            return result;
        }
        LineageEvidence::VerifiedInherited {
            reference,
            baseline,
        } => {
            if reference_matches(reference, observation) {
                if let Some(baseline) = baseline {
                    if observation.cumulative != Some(baseline.cumulative)
                        || baseline.stream_key.is_empty()
                        || baseline.episode_id.is_empty()
                        || baseline.cumulative.validated_total().is_err()
                    {
                        return result;
                    }
                    if !result.state.streams.contains_key(&baseline.stream_key)
                        && result.state.streams.len() >= MAX_STREAMS
                    {
                        return capacity(result);
                    }
                    result
                        .state
                        .streams
                        .insert(baseline.stream_key.clone(), (**baseline).clone());
                    result.stream_key = Some(baseline.stream_key.clone());
                    result.episode_id = Some(baseline.episode_id.clone());
                }
                result.method = CalculationMethod::Inherited;
                result.quality = ObservationQuality::Inherited;
                result.canonical_event_id = Some(reference.event_id.clone());
                result.error = None;
                if let Some(context) = &mut result.context {
                    context.quality = ObservationQuality::Inherited;
                }
            }
            return result;
        }
        LineageEvidence::Independent => {}
    }
    let last = observation.last;
    let cumulative = observation.cumulative;
    if last.is_some_and(|v| v.validated_total().is_ok_and(|t| t.is_none())) {
        return result;
    }
    let hinted_key = observation
        .stream_hint
        .as_ref()
        .filter(|s| !s.is_empty())
        .map(|s| format!("hint:{s}"));
    let existing = hinted_key.as_ref().and_then(|key| state.streams.get(key));
    let reset = observation.explicit_episode_start && existing.is_some();
    if let Some(total) = cumulative {
        if let Some(baseline) = existing.filter(|_| !reset) {
            result.stream_key = Some(baseline.stream_key.clone());
            result.episode_id = Some(baseline.episode_id.clone());
            if total == baseline.cumulative && (last.is_none() || last == baseline.last_snapshot) {
                result.method = CalculationMethod::RepeatedSnapshot;
                result.quality = ObservationQuality::Duplicate;
                result.error = None;
                if let Some(context) = &mut result.context {
                    context.quality = ObservationQuality::Duplicate;
                }
                return result;
            }
            if let Some(last) = last {
                if subtract(total, last).is_ok_and(|before| before == baseline.cumulative) {
                    return confirm(
                        result,
                        observation,
                        last,
                        Some((baseline.stream_key.clone(), baseline.episode_id.clone())),
                        CalculationMethod::LastWithBaseline,
                    );
                }
                return result;
            }
            return match subtract(total, baseline.cumulative) {
                Ok(delta) if delta.validated_total().is_ok_and(|t| t.is_some()) => confirm(
                    result,
                    observation,
                    delta,
                    Some((baseline.stream_key.clone(), baseline.episode_id.clone())),
                    CalculationMethod::CumulativeDelta,
                ),
                _ => result,
            };
        }
        if let Some(last) = last {
            let before = match subtract(total, last) {
                Ok(v) => v,
                Err(_) => return result,
            };
            // An explicit new stream cannot be merged with an equal baseline in another stream.
            let candidates: Vec<_> = if hinted_key.is_none() && !evidence.independent_new_stream {
                state
                    .streams
                    .values()
                    .filter(|b| comparable(before, b.cumulative) && before == b.cumulative)
                    .collect()
            } else {
                vec![]
            };
            result.candidate_stream_keys =
                candidates.iter().map(|b| b.stream_key.clone()).collect();
            if candidates.len() == 1 {
                let baseline = candidates[0];
                return confirm(
                    result,
                    observation,
                    last,
                    Some((baseline.stream_key.clone(), baseline.episode_id.clone())),
                    CalculationMethod::LastWithBaseline,
                );
            }
            if candidates.len() > 1
                || !evidence.independent_new_stream && !observation.explicit_episode_start
            {
                return result;
            }
            if result.state.streams.len() >= MAX_STREAMS && !reset {
                return capacity(result);
            }
            let key = hinted_key.unwrap_or_else(|| inferred_stream_key(observation));
            let episode = episode_key(observation);
            if before
                .validated_total()
                .is_ok_and(|t| t.is_some_and(|n| n > 0))
            {
                result.prior_anchor = Some(before);
            }
            return confirm(
                result,
                observation,
                last,
                Some((key, episode)),
                if reset {
                    CalculationMethod::EpisodeReset
                } else {
                    CalculationMethod::LastNewStream
                },
            );
        }
        // Initial cumulative values establish a baseline and never become today's consumption.
        if result.state.streams.len() >= MAX_STREAMS && !reset {
            return capacity(result);
        }
        let key = hinted_key.unwrap_or_else(|| inferred_stream_key(observation));
        let episode = episode_key(observation);
        result.state.streams.insert(
            key.clone(),
            StreamBaseline {
                stream_key: key.clone(),
                episode_id: episode.clone(),
                cumulative: total,
                last_snapshot: None,
            },
        );
        result.stream_key = Some(key);
        result.episode_id = Some(episode);
        result.method = CalculationMethod::UnattributedAnchor;
        result.quality = ObservationQuality::Unattributed;
        result.prior_anchor = Some(total);
        result.error = None;
        return result;
    }
    if let Some(last) = last_valid {
        return confirm(result, observation, last, None, CalculationMethod::LastOnly);
    }
    result
}

fn reference_matches(reference: &CanonicalReference, observation: &UsageObservation) -> bool {
    !reference.event_id.is_empty()
        && reference.usage.validated_total().is_ok_and(|t| t.is_some())
        && match &reference.observation.request_identity {
            Some(identity) => {
                observation.request_identity.as_ref() == Some(identity)
                    && observation.last == Some(reference.usage)
            }
            None => UsageSignature::from(observation) == reference.observation,
        }
}
fn capacity(mut result: AccountingResult) -> AccountingResult {
    result.method = CalculationMethod::StreamCapacityExceeded;
    result
}
fn confirm(
    mut result: AccountingResult,
    observation: &UsageObservation,
    usage: UsageVector,
    stream: Option<(String, String)>,
    method: CalculationMethod,
) -> AccountingResult {
    if usage.validated_total().is_err() {
        return result;
    }
    if let Some((key, episode)) = stream {
        result.state.streams.insert(
            key.clone(),
            StreamBaseline {
                stream_key: key.clone(),
                episode_id: episode.clone(),
                cumulative: observation
                    .cumulative
                    .expect("stream update requires cumulative"),
                last_snapshot: observation.last,
            },
        );
        result.stream_key = Some(key);
        result.episode_id = Some(episode);
    }
    result.method = method;
    result.quality = ObservationQuality::Confirmed;
    result.error = None;
    if let Some(context) = &mut result.context {
        context.quality = ObservationQuality::Confirmed;
    }
    match (observation.event_time_ms, usage.validated_total()) {
        (Some(_), Ok(Some(total))) if total > 0 => result.event_usage = Some(usage),
        (None, Ok(Some(total))) if total > 0 => {
            result.method = CalculationMethod::UnattributedUsage;
            result.quality = ObservationQuality::Unattributed;
            result.prior_anchor = Some(usage);
        }
        _ => {}
    }
    result
}
/// Missing fields stay missing; mismatched masks and invalid component deltas are not comparable.
fn subtract(current: UsageVector, previous: UsageVector) -> Result<UsageVector, ErrorCode> {
    fn difference(a: Option<i64>, b: Option<i64>) -> Result<Option<i64>, ErrorCode> {
        match (a, b) {
            (None, None) => Ok(None),
            (Some(a), Some(b)) => a
                .checked_sub(b)
                .filter(|&n| n >= 0)
                .map(Some)
                .ok_or(ErrorCode::InvalidUsage),
            _ => Err(ErrorCode::AmbiguousUsage),
        }
    }
    let delta = UsageVector {
        input_total: difference(current.input_total, previous.input_total)?,
        cached_input: difference(current.cached_input, previous.cached_input)?,
        output_total: difference(current.output_total, previous.output_total)?,
        reasoning_output: difference(current.reasoning_output, previous.reasoning_output)?,
        reported_total: difference(current.reported_total, previous.reported_total)?,
    };
    delta.validated_total()?;
    Ok(delta)
}
fn comparable(a: UsageVector, b: UsageVector) -> bool {
    a.input_total.is_some()
        && a.output_total.is_some()
        && b.input_total.is_some()
        && b.output_total.is_some()
}
fn position_hash(observation: &UsageObservation) -> String {
    let bytes = serde_json::to_vec(&(&observation.session_key, &observation.physical_position))
        .expect("typed position serializes");
    format!("{:x}", Sha256::digest(bytes))
}
fn inferred_stream_key(observation: &UsageObservation) -> String {
    format!("inferred:{}", position_hash(observation))
}
fn episode_key(observation: &UsageObservation) -> String {
    format!("episode:{}", position_hash(observation))
}
