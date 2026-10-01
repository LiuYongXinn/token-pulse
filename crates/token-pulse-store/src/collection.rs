//! Typed collector reads use genuine snapshots; no SQL crosses the renderer boundary.
use crate::rusqlite::{OptionalExtension, params};
use crate::{Database, ErrorCode, StoreResult};
use std::collections::BTreeMap;
use token_pulse_core::{
    accounting::{AccountingState, StreamBaseline},
    domain::*,
};

pub struct FileCheckpoint {
    pub file_id: String,
    pub source_id: String,
    pub file_generation_id: String,
    pub file_identity: Option<String>,
    pub observed_size: i64,
    pub committed_offset: i64,
    pub checkpoint_revision: i64,
    pub anchors: Vec<ContentAnchor>,
    pub context: ReaderContext,
}
pub struct SessionAccounting {
    pub ledger_id: String,
    pub state: AccountingState,
    pub revisions: BTreeMap<(String, String), i64>,
}
impl Database {
    pub fn enabled_sources(&self) -> StoreResult<Vec<crate::SourceRecord>> {
        self.snapshot(|tx,_| {let mut s=tx.prepare("SELECT source_id,root_path,directory_identity,kind,enabled,created_at_ms FROM sources WHERE enabled=1")?;
            let rows=s.query_map([],|r|Ok(crate::SourceRecord{source_id:r.get(0)?,root_path:r.get(1)?,directory_identity:r.get(2)?,kind:r.get(3)?,enabled:r.get(4)?,created_at_ms:r.get(5)?}))?;
            rows.collect::<Result<Vec<_>,_>>().map_err(Into::into)
        })
    }
    pub fn update_source_runtime(
        &self,
        source_id: String,
        readability: token_pulse_core::sources::SourceReadability,
        capabilities: token_pulse_core::sources::SourceCapabilities,
        scan_at: Option<i64>,
        success_at: Option<i64>,
    ) -> StoreResult<()> {
        self.write(move|conn| {conn.execute("UPDATE sources SET readability=?1,capabilities_json=?2,last_scan_at_ms=COALESCE(?3,last_scan_at_ms),last_success_at_ms=COALESCE(?4,last_success_at_ms) WHERE source_id=?5",params![serde_json::to_value(readability)?.as_str(),serde_json::to_string(&capabilities)?,scan_at,success_at,source_id])?;Ok(())})
    }
    pub fn enabled_source_root(&self, source_id: &str) -> StoreResult<Option<String>> {
        self.snapshot(|tx, _| {
            Ok(tx
                .query_row(
                    "SELECT root_path FROM sources WHERE source_id=?1 AND enabled=1",
                    [source_id],
                    |r| r.get(0),
                )
                .optional()?)
        })
    }
    pub fn session_has_usage(&self, session: &str) -> StoreResult<bool> {
        self.snapshot(|tx, _| {
            Ok(tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM observations WHERE session_key=?1 AND kind='usage')",
                [session],
                |r| r.get(0),
            )?)
        })
    }
    pub fn source_is_enabled(&self, source_id: &str) -> StoreResult<bool> {
        self.snapshot(|tx, _| {
            Ok(tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM sources WHERE source_id=?1 AND enabled=1)",
                [source_id],
                |r| r.get(0),
            )?)
        })
    }
    pub fn file_checkpoint(
        &self,
        source_id: &str,
        path: &str,
        identity: Option<&str>,
    ) -> StoreResult<Option<FileCheckpoint>> {
        self.snapshot(|tx,_| {
            let row=tx.query_row("SELECT f.file_id,f.source_id,g.file_generation_id,g.identity_json,g.observed_size,g.committed_offset,g.checkpoint_revision,g.anchor_json,g.reader_context_json FROM source_files f JOIN file_generations g ON g.file_generation_id=f.current_generation_id WHERE f.source_id=?1 AND (f.canonical_path=?2 OR (?3 IS NOT NULL AND f.file_identity=?3)) AND g.state='current' ORDER BY f.canonical_path=?2 DESC LIMIT 1",params![source_id,path,identity],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,i64>(4)?,r.get::<_,i64>(5)?,r.get::<_,i64>(6)?,r.get::<_,String>(7)?,r.get::<_,String>(8)?))).optional()?;
            row.map(|(file_id,source_id,file_generation_id,identity,observed_size,committed_offset,checkpoint_revision,anchors,context)|Ok(FileCheckpoint{file_id,source_id,file_generation_id,file_identity:serde_json::from_str(&identity)?,observed_size,committed_offset,checkpoint_revision,anchors:serde_json::from_str(&anchors)?,context:serde_json::from_str(&context)?})).transpose()
        })
    }
    pub fn relocate_file(&self, file_id: String, path: String, at_ms: i64) -> StoreResult<()> {
        self.write(move |conn| {
            conn.execute(
                "UPDATE source_files SET canonical_path=?1,last_seen_at_ms=?2 WHERE file_id=?3",
                params![path, at_ms, file_id],
            )?;
            Ok(())
        })
    }
    pub fn related_session_exists(
        &self,
        provider_id: &str,
        exclude_session: &str,
    ) -> StoreResult<bool> {
        self.snapshot(|tx,_|Ok(tx.query_row("SELECT EXISTS(SELECT 1 FROM sessions WHERE provider='codex' AND provider_session_id=?1 AND session_key<>?2)",params![provider_id,exclude_session],|r|r.get(0))?))
    }
    pub fn session_accounting(&self, session: &str) -> StoreResult<SessionAccounting> {
        self.snapshot(|tx,_| {
            let ledger_id:String=tx.query_row("SELECT active_ledger_id FROM sessions WHERE session_key=?1",[session],|r|r.get(0))?;
            let mut statement=tx.prepare("SELECT st.stream_key,st.episode_id,st.baseline_json,st.state_revision,o.normalized_json FROM stream_states st LEFT JOIN observations o ON o.observation_id=st.last_observation_id WHERE st.ledger_id=?1 ORDER BY o.rowid")?;
            let rows=statement.query_map([&ledger_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,i64>(3)?,r.get::<_,Option<String>>(4)?)))?;
            let mut state=AccountingState::new(session.into());let mut revisions=BTreeMap::new();
            for row in rows {let (key,episode,baseline,revision,record)=row?;
                let last_snapshot=match record.map(|s|serde_json::from_str::<NormalizedObservation>(&s)).transpose()? {Some(NormalizedObservation::Usage(u))=>u.last,_=>None};
                let cumulative:UsageVector=serde_json::from_str(&baseline)?;cumulative.validated_total()?;
                revisions.insert((key.clone(),episode.clone()),revision);
                state.streams.insert(key.clone(),StreamBaseline{stream_key:key,episode_id:episode,cumulative,last_snapshot});
            }
            if state.streams.len()>token_pulse_core::accounting::MAX_STREAMS {return Err(ErrorCode::InvalidUsage.into());}
            Ok(SessionAccounting{ledger_id,state,revisions})
        })
    }
}
