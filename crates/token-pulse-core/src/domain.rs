use crate::error::ErrorCode;
use serde::{Deserialize, Serialize};

pub const PARSER_VERSION: &str = "codex-rollout-v1";
pub const ACCOUNTING_VERSION: &str = "accounting-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageVector {
    pub input_total: Option<i64>,
    pub cached_input: Option<i64>,
    pub output_total: Option<i64>,
    pub reasoning_output: Option<i64>,
    pub reported_total: Option<i64>,
}
impl UsageVector {
    pub fn validated_total(&self) -> Result<Option<i64>, ErrorCode> {
        if [
            self.input_total,
            self.cached_input,
            self.output_total,
            self.reasoning_output,
            self.reported_total,
        ]
        .into_iter()
        .flatten()
        .any(|v| v < 0)
        {
            return Err(ErrorCode::InvalidUsage);
        }
        if matches!((self.cached_input, self.input_total), (Some(child), Some(parent)) if child > parent)
            || matches!((self.reasoning_output, self.output_total), (Some(child), Some(parent)) if child > parent)
        {
            return Err(ErrorCode::InvalidUsage);
        }
        let calculated = match (self.input_total, self.output_total) {
            (Some(input), Some(output)) => Some(
                input
                    .checked_add(output)
                    .ok_or(ErrorCode::NumericOverflow)?,
            ),
            _ => None,
        };
        if matches!((calculated, self.reported_total), (Some(sum), Some(total)) if sum != total) {
            return Err(ErrorCode::InvalidUsage);
        }
        Ok(calculated.or(self.reported_total))
    }
    pub fn noncached_input(&self) -> Result<Option<i64>, ErrorCode> {
        self.validated_total()?;
        Ok(match (self.input_total, self.cached_input) {
            (Some(i), Some(c)) => Some(i - c),
            _ => None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicalPosition {
    pub file_generation_id: String,
    pub byte_offset: u64,
    pub byte_end: u64,
}
impl PhysicalPosition {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if self.file_generation_id.is_empty()
            || self.byte_end <= self.byte_offset
            || self.byte_end > i64::MAX as u64
        {
            return Err(ErrorCode::InvalidQuery);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectiveMetadata {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub cwd: Option<String>,
    pub turn_id: Option<String>,
    pub parent_provider_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReaderContext {
    pub provider_session_id: Option<String>,
    pub session_key: Option<String>,
    pub metadata: EffectiveMetadata,
    #[serde(default)]
    pub oversized_line: Option<OversizedLineState>,
    #[serde(default)]
    pub independent_head_available: bool,
    #[serde(default)]
    pub requires_sequence_rebuild: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OversizedLineState {
    pub start_offset: u64,
    pub scan_offset: u64,
    pub anchors: Vec<ContentAnchor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentAnchor {
    pub byte_offset: u64,
    pub byte_length: u32,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifiedRequestIdentity {
    pub namespace: String,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageObservation {
    pub physical_position: PhysicalPosition,
    pub session_key: String,
    pub event_time_ms: Option<i64>,
    pub request_identity: Option<VerifiedRequestIdentity>,
    pub stream_hint: Option<String>,
    pub last: Option<UsageVector>,
    pub cumulative: Option<UsageVector>,
    pub effective_metadata: EffectiveMetadata,
    pub explicit_episode_start: bool,
    pub model_context_window: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationQuality {
    Confirmed,
    Pending,
    Inherited,
    Duplicate,
    Unattributed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NormalizedObservation {
    SessionMetadata {
        physical_position: PhysicalPosition,
        provider_session_id: String,
        metadata: EffectiveMetadata,
        created_at_ms: Option<i64>,
    },
    TurnMetadata {
        physical_position: PhysicalPosition,
        session_key: String,
        metadata: EffectiveMetadata,
    },
    Usage(UsageObservation),
    Context {
        physical_position: PhysicalPosition,
        session_key: String,
        observed_at_ms: Option<i64>,
        usage: UsageVector,
        model_context_window: Option<i64>,
        metadata: EffectiveMetadata,
    },
}
