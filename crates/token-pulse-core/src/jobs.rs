//! Validated job requests and progress. Persisted checkpoints contain no source text.
use crate::{
    error::ErrorCode,
    numeric::DecimalInt,
    protocol::{JobKind, JobState, validate_request_id},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum JobScope {
    All {},
    Sources { source_ids: Vec<String> },
    Sessions { session_keys: Vec<String> },
}
impl JobScope {
    pub fn canonicalize(&mut self) -> Result<(), ErrorCode> {
        let ids = match self {
            Self::All {} => return Ok(()),
            Self::Sources { source_ids } => source_ids,
            Self::Sessions { session_keys } => session_keys,
        };
        if ids.is_empty() || ids.len() > 100 {
            return Err(ErrorCode::InvalidQuery);
        }
        for id in ids.iter() {
            if id.len() > 256 || id.is_empty() || id.chars().any(char::is_control) {
                return Err(ErrorCode::InvalidQuery);
            }
        }
        ids.sort();
        ids.dedup();
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct JobRequest {
    pub kind: JobKind,
    pub scope: JobScope,
    pub request_key: String,
}
impl JobRequest {
    pub fn validate(&mut self) -> Result<(), ErrorCode> {
        validate_request_id(&self.request_key)?;
        self.scope.canonicalize()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum CancelJobResult {
    Accepted,
    AlreadyFinished,
    TooLate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobProgress {
    pub phase: String,
    pub discovered_files: DecimalInt,
    pub discovery_complete: bool,
    pub processed_files: DecimalInt,
    pub processed_bytes: DecimalInt,
    pub accepted_events: DecimalInt,
    pub pending_observations: DecimalInt,
}
impl Default for JobProgress {
    fn default() -> Self {
        Self {
            phase: "queued".into(),
            discovered_files: DecimalInt::parse("0").unwrap(),
            discovery_complete: false,
            processed_files: DecimalInt::parse("0").unwrap(),
            processed_bytes: DecimalInt::parse("0").unwrap(),
            accepted_events: DecimalInt::parse("0").unwrap(),
            pending_observations: DecimalInt::parse("0").unwrap(),
        }
    }
}
impl JobProgress {
    pub fn validate_after(&self, old: &Self) -> Result<(), ErrorCode> {
        validate_request_id(&self.phase)?;
        if (old.discovery_complete && !self.discovery_complete)
            || self.processed_files.value() > self.discovered_files.value()
        {
            return Err(ErrorCode::InvalidQuery);
        }
        for (next, previous) in [
            (&self.discovered_files, &old.discovered_files),
            (&self.processed_files, &old.processed_files),
            (&self.processed_bytes, &old.processed_bytes),
            (&self.accepted_events, &old.accepted_events),
            (&self.pending_observations, &old.pending_observations),
        ] {
            if next.value() < previous.value() {
                return Err(ErrorCode::RevisionConflict);
            }
        }
        Ok(())
    }
}
pub fn finished(state: JobState) -> bool {
    matches!(
        state,
        JobState::Succeeded | JobState::Cancelled | JobState::Failed | JobState::Interrupted
    )
}
pub fn can_cancel(state: JobState) -> bool {
    matches!(
        state,
        JobState::Queued | JobState::Running | JobState::Validating | JobState::Cancelling
    )
}

/// Only publication is a non-interruptible phase. Workers perform transitions at batch boundaries.
pub fn transition_allowed(from: JobState, to: JobState) -> bool {
    use JobState::*;
    matches!(
        (from, to),
        (Queued, Running)
            | (Queued, Cancelled)
            | (Running, Validating)
            | (Validating, Publishing)
            | (Publishing, Succeeded)
            | (Cancelling, Cancelled)
    ) || (matches!(from, Queued | Running | Validating | Cancelling) && to == Failed)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobCheckpoint {
    pub version: u32,
    pub parser_version: String,
    pub accounting_version: String,
    pub batch_position: DecimalInt,
    pub candidate_ledger_ids: Vec<String>,
}
impl Default for JobCheckpoint {
    fn default() -> Self {
        Self {
            version: 1,
            parser_version: crate::domain::PARSER_VERSION.into(),
            accounting_version: crate::domain::ACCOUNTING_VERSION.into(),
            batch_position: DecimalInt::parse("0").unwrap(),
            candidate_ledger_ids: vec![],
        }
    }
}
impl JobCheckpoint {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if self.version != 1 || self.candidate_ledger_ids.len() > 32768 {
            return Err(ErrorCode::InvalidQuery);
        }
        validate_request_id(&self.parser_version)?;
        validate_request_id(&self.accounting_version)?;
        for id in &self.candidate_ledger_ids {
            validate_request_id(id)?;
        }
        Ok(())
    }
}
