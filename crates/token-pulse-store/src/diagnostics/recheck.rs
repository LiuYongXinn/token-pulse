//! Bounded auxiliary-only diagnostic reconciliation. Consumption facts are immutable.
use crate::rusqlite::{OptionalExtension, TransactionBehavior, params};
use crate::{Database, ErrorCode, StoreResult};
use token_pulse_core::adapter::AUXILIARY_CLASSIFIER_VERSION;

pub struct DiagnosticPosition {
    pub diagnostic_id: String,
    pub byte_offset: u64,
}
impl Database {
    pub fn auxiliary_diagnostic_positions(
        &self,
        generation: &str,
        before: i64,
    ) -> StoreResult<(Vec<DiagnosticPosition>, bool)> {
        self.snapshot(|tx,_| {
            let mut q=tx.prepare("SELECT diagnostic_id,byte_offset FROM diagnostics WHERE file_generation_id=?1 AND code='UNSUPPORTED_FORMAT' AND resolved_at_ms IS NULL AND byte_offset>=0 AND byte_offset<?2 AND COALESCE(json_extract(metadata_json,'$.auxiliary_classifier'),'')<>?3 ORDER BY byte_offset,diagnostic_id LIMIT 65")?;
            let mut rows=q.query(params![generation,before,AUXILIARY_CLASSIFIER_VERSION])?;
            let mut positions=vec![];
            while let Some(r)=rows.next()? {
                let offset:i64=r.get(1)?;
                positions.push(DiagnosticPosition{diagnostic_id:r.get(0)?,byte_offset:offset as u64});
            }
            let more=positions.len()>64;positions.truncate(64);Ok((positions,more))
        })
    }
    /// Checkpoint CAS prevents a recheck from resolving diagnostics on a replaced input.
    pub fn publish_auxiliary_diagnostic_recheck(
        &self,
        generation: String,
        checkpoint_revision: i64,
        rechecked: Vec<(String, bool)>,
        at: i64,
    ) -> StoreResult<(i64, bool)> {
        if rechecked.is_empty() || rechecked.len() > 64 {
            return Err(ErrorCode::InvalidQuery.into());
        }
        self.write(move|conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let revision:Option<i64>=tx.query_row("SELECT g.checkpoint_revision FROM file_generations g JOIN source_files f ON f.current_generation_id=g.file_generation_id JOIN sources s ON s.source_id=f.source_id WHERE g.file_generation_id=?1 AND g.state='current' AND s.enabled=1",[&generation],|r|r.get(0)).optional()?;
            if revision!=Some(checkpoint_revision) {return Err(ErrorCode::CheckpointConflict.into());}
            let frozen:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM rebuild_manifests m JOIN jobs j USING(job_id) JOIN json_each(m.manifest_json,'$.files') input WHERE j.state IN ('running','validating','publishing','cancelling') AND json_extract(input.value,'$.generation_id')=?1) OR EXISTS(SELECT 1 FROM file_read_candidates c JOIN file_rebuild_candidates r ON r.generation_id=c.generation_id JOIN jobs j ON j.job_id=r.job_id WHERE json_extract(c.base_json,'$.generation_id')=?1 AND c.state IN ('reading','ready','claimed') AND j.state IN ('running','validating','publishing','cancelling'))",[&generation],|r|r.get(0))?;
            // Frozen jobs use their recorded diagnostics as continuity evidence.
            if frozen {return Err(ErrorCode::CheckpointConflict.into());}
            let mut resolved=false;
            for (diagnostic,auxiliary) in rechecked {
                let n=tx.execute("UPDATE diagnostics SET metadata_json=json_set(metadata_json,'$.auxiliary_classifier',?1),resolved_at_ms=CASE WHEN ?2 THEN ?3 ELSE resolved_at_ms END WHERE diagnostic_id=?4 AND file_generation_id=?5 AND code='UNSUPPORTED_FORMAT' AND resolved_at_ms IS NULL AND COALESCE(json_extract(metadata_json,'$.auxiliary_classifier'),'')<>?1",params![AUXILIARY_CLASSIFIER_VERSION,auxiliary,at,diagnostic,generation])?;
                resolved |= n>0 && auxiliary;
            }
            let mut data:i64=tx.query_row("SELECT data_revision FROM app_state WHERE singleton=1",[],|r|r.get(0))?;
            if resolved {data=data.checked_add(1).ok_or(ErrorCode::NumericOverflow)?;tx.execute("UPDATE app_state SET data_revision=?1 WHERE singleton=1",[data])?;}
            tx.commit()?;Ok((data,resolved))
        })
    }
}
