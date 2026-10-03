//! Explicit manual reads select only registered files of enabled local sources.
use super::*;
use token_pulse_core::protocol::JobState;

impl Database {
    pub fn manual_source_read_files(&self, job_id: &str) -> StoreResult<Vec<(String, FrozenFile)>> {
        self.snapshot(|tx, _| {
            let job = crate::jobs::load(tx, job_id)?;
            if !job.checkpoint.reread_sources || job.job.state != JobState::Running {
                return Err(ErrorCode::InvalidQuery.into());
            }
            let mut query = tx.prepare("SELECT f.file_id FROM source_files f JOIN sources s USING(source_id) JOIN file_generations g ON g.file_generation_id=f.current_generation_id WHERE s.enabled=1 AND s.kind='local' AND g.state='current' ORDER BY f.file_id")?;
            let mut files = Vec::new();
            for row in query.query_map([], |r| r.get::<_, String>(0))? {
                let id = row?;
                let base = frozen(tx, &id)?;
                if rebuild::permitted(&job, &base.source_id) {
                    files.push((id, base));
                    if files.len() > 32768 { return Err(ErrorCode::InvalidQuery.into()); }
                }
            }
            if files.len() > 32768 { return Err(ErrorCode::InvalidQuery.into()); }
            Ok(files)
        })
    }
}
