//! Source-reported completion timings. This reader never publishes consumption or changes its cursor.
use crate::{Database, ErrorCode, StoreResult, collection::FileCheckpoint};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use token_pulse_core::{
    domain::{OversizedLineState, PhysicalPosition},
    numeric::DecimalInt,
    protocol::validate_request_id,
};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimingCheckpoint {
    pub offset: u64,
    pub provider_session_id: Option<String>,
    pub oversized_line: Option<OversizedLineState>,
}

#[derive(Debug)]
pub struct TurnTimingRecord {
    pub position: PhysicalPosition,
    pub provider_session_id: String,
    pub turn_id: String,
    pub duration_ms: Option<i64>,
    pub time_to_first_token_ms: Option<i64>,
}
impl TurnTimingRecord {
    fn validate(&self) -> StoreResult<()> {
        self.position.validate()?;
        validate_request_id(&self.provider_session_id)?;
        validate_request_id(&self.turn_id)?;
        if self.duration_ms.is_none() && self.time_to_first_token_ms.is_none()
            || self.duration_ms.is_some_and(|v| v < 0)
            || self.time_to_first_token_ms.is_some_and(|v| v < 0)
            || matches!((self.duration_ms, self.time_to_first_token_ms), (Some(d), Some(t)) if t > d)
        {
            return Err(ErrorCode::InvalidQuery.into());
        }
        Ok(())
    }
}

impl Database {
    pub fn turn_timing_checkpoint(&self, generation: &str) -> StoreResult<TimingCheckpoint> {
        self.snapshot(|tx, _| {
            let encoded: Option<String> = tx
                .query_row(
                    "SELECT context_json FROM turn_timing_scans WHERE file_generation_id=?1",
                    [generation],
                    |r| r.get(0),
                )
                .optional()?;
            encoded
                .map(|s| serde_json::from_str(&s).map_err(Into::into))
                .transpose()
                .map(|v| v.unwrap_or_default())
        })
    }

    /// Commit one supplemental slice only while the imported physical generation still matches.
    pub fn publish_turn_timings(
        &self,
        file: FileCheckpoint,
        expected_offset: u64,
        next: TimingCheckpoint,
        records: Vec<TurnTimingRecord>,
    ) -> StoreResult<bool> {
        if records.len() > 500
            || next.offset < expected_offset
            || next.offset > file.committed_offset as u64
        {
            return Err(ErrorCode::InvalidQuery.into());
        }
        for record in &records {
            record.validate()?;
            if record.position.file_generation_id != file.file_generation_id
                || record.position.byte_offset < expected_offset
                || record.position.byte_end > next.offset
            {
                return Err(ErrorCode::InvalidQuery.into());
            }
        }
        if let Some(id) = &next.provider_session_id {
            validate_request_id(id)?;
        }
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let current = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM file_generations fg JOIN source_files sf ON sf.file_id=fg.file_id JOIN sources s ON s.source_id=sf.source_id WHERE fg.file_generation_id=?1 AND fg.checkpoint_revision=?2 AND fg.committed_offset=?3 AND sf.current_generation_id=fg.file_generation_id AND fg.state='current' AND s.source_id=?4 AND s.enabled=1)",
                params![file.file_generation_id,file.checkpoint_revision,file.committed_offset,file.source_id], |r| r.get::<_,bool>(0),
            )?;
            let previous: i64 = tx.query_row("SELECT scanned_offset FROM turn_timing_scans WHERE file_generation_id=?1", [&file.file_generation_id], |r| r.get(0)).optional()?.unwrap_or(0);
            if !current || previous < 0 || previous as u64 != expected_offset { return Err(ErrorCode::CheckpointConflict.into()); }
            let mut changed = false;
            for record in records {
                let bound = tx.query_row("SELECT EXISTS(SELECT 1 FROM file_session_bindings b JOIN sessions s ON s.session_key=b.session_key WHERE b.file_generation_id=?1 AND s.provider_session_id=?2)", params![file.file_generation_id,record.provider_session_id], |r|r.get::<_,bool>(0))?;
                if !bound { return Err(ErrorCode::CheckpointConflict.into()); }
                changed |= tx.execute("INSERT INTO turn_timing_records(file_generation_id,byte_offset,byte_end,provider_session_id,turn_id,duration_ms,time_to_first_token_ms) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![file.file_generation_id,record.position.byte_offset as i64,record.position.byte_end as i64,record.provider_session_id,record.turn_id,record.duration_ms,record.time_to_first_token_ms])? > 0;
            }
            tx.execute("INSERT INTO turn_timing_scans(file_generation_id,scanned_offset,context_json) VALUES(?1,?2,?3) ON CONFLICT(file_generation_id) DO UPDATE SET scanned_offset=excluded.scanned_offset,context_json=excluded.context_json",params![file.file_generation_id,next.offset as i64,serde_json::to_string(&next)?])?;
            tx.commit()?;
            Ok(changed)
        })
    }
}

/// Only timing from a physical sequence contributing to this active turn is eligible.
/// Conflicting completions remain unknown; agreeing mirrors are not added together.
pub(crate) fn read(
    tx: &Transaction<'_>,
    session: &str,
    turn: &str,
) -> StoreResult<(Option<DecimalInt>, Option<DecimalInt>)> {
    let mut statement = tx.prepare(
        "SELECT DISTINCT t.duration_ms,t.time_to_first_token_ms FROM turn_timing_records t JOIN file_generations fg ON fg.file_generation_id=t.file_generation_id JOIN source_files sf ON sf.current_generation_id=fg.file_generation_id WHERE t.turn_id=?2 AND fg.state='current' AND EXISTS(SELECT 1 FROM active_usage_events e JOIN event_provenance ep ON ep.event_id=e.event_id JOIN observations o ON o.observation_id=ep.observation_id JOIN sessions s ON s.session_key=o.session_key WHERE e.session_key=?1 AND e.turn_id=?2 AND o.file_generation_id=t.file_generation_id AND s.provider_session_id=t.provider_session_id) LIMIT 2",
    )?;
    let records = statement
        .query_map(params![session, turn], |r| {
            Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, Option<i64>>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if records.len() != 1 {
        return Ok((None, None));
    }
    let (duration, first) = records[0];
    if duration.is_some_and(|v| v < 0)
        || first.is_some_and(|v| v < 0)
        || matches!((duration,first),(Some(d),Some(t)) if t>d)
    {
        return Err(ErrorCode::DbCorrupt.into());
    }
    Ok((
        duration
            .map(|v| DecimalInt::from_nonnegative(v.into()))
            .transpose()?,
        first
            .map(|v| DecimalInt::from_nonnegative(v.into()))
            .transpose()?,
    ))
}
