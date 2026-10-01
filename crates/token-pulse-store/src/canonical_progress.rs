//! A live ordinal cursor, derived consumption and physical checkpoint share the Writer transaction.
use crate::{
    ErrorCode, StoreResult,
    batch::{CanonicalStep, WriteBatch},
    rusqlite::{OptionalExtension, Transaction, params},
};
use std::collections::HashSet;
use token_pulse_core::{domain::NormalizedObservation, sequence::UsageSignature};
pub(crate) fn write(
    tx: &Transaction<'_>,
    batch: &WriteBatch,
    allowed: &HashSet<&str>,
) -> StoreResult<()> {
    let mut ledgers = HashSet::new();
    for update in &batch.canonical {
        if !allowed.contains(update.ledger_id.as_str())
            || !ledgers.insert(&update.ledger_id)
            || update.expected_cursor < 0
            || update.expected_length < update.expected_cursor
            || update.steps.len() > 500
        {
            return Err(ErrorCode::InvalidQuery.into());
        }
        let cursor:Option<(i64,String)>=tx.query_row("SELECT next_ordinal,state FROM file_usage_cursors WHERE ledger_id=?1 AND file_generation_id=?2",params![update.ledger_id,batch.file_generation_id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        let length: i64 = tx.query_row(
            "SELECT COUNT(*) FROM canonical_usage_sequence WHERE ledger_id=?1",
            [&update.ledger_id],
            |r| r.get(0),
        )?;
        if cursor != Some((update.expected_cursor, "aligned".into()))
            || length != update.expected_length
        {
            return Err(ErrorCode::CheckpointConflict.into());
        }
        let mut next = update.expected_cursor;
        let mut next_length = length;
        for step in &update.steps {
            let observation = match step {
                CanonicalStep::Append { observation_id }
                | CanonicalStep::Copy { observation_id, .. } => observation_id,
            };
            if !batch.observations.iter().any(|o| {
                o.observation_id == *observation
                    && matches!(o.record, NormalizedObservation::Usage(_))
            }) {
                return Err(ErrorCode::InvalidQuery.into());
            }
            super::batch::same_session(tx, &update.ledger_id, observation)?;
            match step {
                CanonicalStep::Append { .. } => {
                    if next != next_length {
                        return Err(ErrorCode::CheckpointConflict.into());
                    }
                    let classified:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM usage_events WHERE ledger_id=?1 AND origin_observation_id=?2 UNION ALL SELECT 1 FROM pending_usage WHERE ledger_id=?1 AND observation_id=?2)",params![update.ledger_id,observation],|r|r.get(0))?;
                    if !classified {
                        return Err(ErrorCode::InvalidUsage.into());
                    }
                    tx.execute("INSERT INTO canonical_usage_sequence SELECT ?1,?2,?3,(SELECT event_id FROM usage_events WHERE ledger_id=?1 AND origin_observation_id=?3)",params![update.ledger_id,next,observation])?;
                    next_length = next_length
                        .checked_add(1)
                        .ok_or(ErrorCode::NumericOverflow)?;
                }
                CanonicalStep::Copy {
                    origin_observation_id,
                    ..
                } => {
                    let (origin,event):(String,Option<String>)=tx.query_row("SELECT observation_id,event_id FROM canonical_usage_sequence WHERE ledger_id=?1 AND ordinal=?2",params![update.ledger_id,next],|r|Ok((r.get(0)?,r.get(1)?)))?;
                    if origin != *origin_observation_id {
                        return Err(ErrorCode::CheckpointConflict.into());
                    }
                    let signature = |id: &str| -> StoreResult<UsageSignature> {
                        let encoded: String = tx.query_row(
                            "SELECT normalized_json FROM observations WHERE observation_id=?1",
                            [id],
                            |r| r.get(0),
                        )?;
                        let NormalizedObservation::Usage(u) = serde_json::from_str(&encoded)?
                        else {
                            return Err(ErrorCode::InvalidUsage.into());
                        };
                        Ok(UsageSignature::from(&u))
                    };
                    if signature(&origin)? != signature(observation)? {
                        return Err(ErrorCode::InvalidUsage.into());
                    }
                    if tx.query_row("SELECT EXISTS(SELECT 1 FROM usage_events WHERE ledger_id=?1 AND origin_observation_id=?2)",params![update.ledger_id,observation],|r|r.get::<_,bool>(0))? {return Err(ErrorCode::InvalidUsage.into());}
                    let classified:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM pending_usage WHERE ledger_id=?1 AND observation_id=?2 AND kind IN ('duplicate','pending'))",params![update.ledger_id,observation],|r|r.get(0))?;
                    if !classified {
                        return Err(ErrorCode::InvalidUsage.into());
                    }
                    if let Some(event) = event {
                        tx.execute(
                            "INSERT INTO event_provenance VALUES(?1,?2,'mirror')",
                            params![event, observation],
                        )?;
                    }
                }
            }
            next = next.checked_add(1).ok_or(ErrorCode::NumericOverflow)?;
        }
        tx.execute("UPDATE file_usage_cursors SET next_ordinal=?1,state=?2 WHERE ledger_id=?3 AND file_generation_id=?4",params![next,if update.requires_rebuild{"rebuild_required"}else{"aligned"},update.ledger_id,batch.file_generation_id])?;
        // Canonical progression outranks physical row insertion order across different sources.
        tx.execute(
            "DELETE FROM stream_frontiers WHERE ledger_id=?1",
            [&update.ledger_id],
        )?;
        tx.execute("INSERT INTO stream_frontiers SELECT ledger_id,stream_key,episode_id FROM (SELECT st.ledger_id,st.stream_key,st.episode_id,ROW_NUMBER() OVER(PARTITION BY st.stream_key ORDER BY c.ordinal DESC) AS rank FROM stream_states st JOIN canonical_usage_sequence c ON c.ledger_id=st.ledger_id AND c.observation_id=st.last_observation_id WHERE st.ledger_id=?1) WHERE rank=1",[&update.ledger_id])?;
    }
    Ok(())
}
