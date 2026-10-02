//! Latest context is independent of date-filtered consumption.
use crate::{Database, ErrorCode, StoreResult};
use rusqlite::{OptionalExtension, Transaction};
use token_pulse_core::{
    domain::ObservationQuality,
    numeric::{DecimalInt, EpochMs},
    protocol::{ContextSnapshot, DimensionSelection},
};

pub fn latest_context(tx: &Transaction<'_>, session_key: &str) -> StoreResult<ContextSnapshot> {
    DimensionSelection::Ids {
        ids: vec![session_key.into()],
        include_unknown: false,
    }
    .validate()?;
    let ledger:String=tx.query_row("SELECT s.active_ledger_id FROM sessions s WHERE s.session_key=COALESCE((SELECT canonical_session_key FROM session_aliases WHERE alias_session_key=?1),?1) AND s.active_ledger_id IS NOT NULL",[session_key],|r|r.get(0)).optional()?.ok_or(ErrorCode::InvalidQuery)?;
    let row=tx.query_row("SELECT context_tokens,model_context_window,observed_at_ms,quality_json FROM context_snapshots WHERE ledger_id=?1 ORDER BY observed_at_ms DESC,context_id COLLATE BINARY ASC LIMIT 1",[ledger],|r|Ok((r.get::<_,Option<i64>>(0)?,r.get::<_,Option<i64>>(1)?,r.get::<_,i64>(2)?,r.get::<_,String>(3)?))).optional()?;
    let Some((tokens, capacity, time, quality)) = row else {
        return Ok(ContextSnapshot {
            context_tokens: None,
            model_context_window: None,
            percentage: None,
            observed_at_ms: None,
            quality: "unknown".into(),
        });
    };
    let quality: ObservationQuality =
        serde_json::from_str(&quality).map_err(|_| ErrorCode::DbCorrupt)?;
    if tokens.is_some_and(|n| n < 0) || capacity.is_some_and(|n| n <= 0) {
        return Err(ErrorCode::DbCorrupt.into());
    }
    let percentage = match (tokens, capacity) {
        (Some(n), Some(c)) => Some(n as f64 / c as f64 * 100.0),
        _ => None,
    };
    // Percentage is approximate display data; the original exact token strings
    // remain available. Usage above capacity is not silently clamped to 100%.
    Ok(ContextSnapshot {
        context_tokens: tokens
            .map(|n| DecimalInt::from_nonnegative(n.into()))
            .transpose()?,
        model_context_window: capacity
            .map(|n| DecimalInt::from_nonnegative(n.into()))
            .transpose()?,
        percentage,
        observed_at_ms: Some(EpochMs::new(time)?),
        quality: serde_json::to_value(quality)?
            .as_str()
            .ok_or(ErrorCode::DbCorrupt)?
            .into(),
    })
}
impl Database {
    pub fn latest_context(&self, session_key: &str) -> StoreResult<ContextSnapshot> {
        self.snapshot(|tx, _| latest_context(tx, session_key))
    }
}
#[cfg(test)]
mod tests;
