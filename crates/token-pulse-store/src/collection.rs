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
    pub canonical: Option<CanonicalFileState>,
}
pub struct CanonicalFileState {
    pub cursor: i64,
    pub length: i64,
    pub aligned: bool,
    pub head_proven: bool,
    pub slots: Vec<CanonicalSlot>,
}
pub struct CanonicalSlot {
    pub ordinal: i64,
    pub observation_id: String,
    pub signature: token_pulse_core::sequence::UsageSignature,
    pub reference_usage: Option<UsageVector>,
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
        self.write(move|conn| {
            let (kind,config):(String,String)=conn.query_row("SELECT kind,capabilities_json FROM sources WHERE source_id=?1",[&source_id],|r|Ok((r.get(0)?,r.get(1)?)))?;
            let (origin,removed,_)=crate::source_management::configuration(&config,&kind)?;
            conn.execute("UPDATE sources SET readability=?1,capabilities_json=?2,last_scan_at_ms=COALESCE(?3,last_scan_at_ms),last_success_at_ms=COALESCE(?4,last_success_at_ms) WHERE source_id=?5",params![serde_json::to_value(readability)?.as_str(),crate::source_management::encoded(origin,removed,capabilities)?,scan_at,success_at,source_id])?;Ok(())})
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
        self.snapshot(|tx,_|Ok(tx.query_row("SELECT EXISTS(SELECT 1 FROM sessions s LEFT JOIN session_aliases a ON a.alias_session_key=s.session_key WHERE s.provider='codex' AND s.provider_session_id=?1 AND COALESCE(a.canonical_session_key,s.session_key)<>?2)",params![provider_id,exclude_session],|r|r.get(0))?))
    }
    pub fn resolve_session(&self, session: &str) -> StoreResult<String> {
        self.snapshot(|tx,_|{
            let result:String=tx.query_row("SELECT COALESCE((SELECT canonical_session_key FROM session_aliases WHERE alias_session_key=?1),?1)",[session],|r|r.get(0))?;
            if tx.query_row("SELECT EXISTS(SELECT 1 FROM session_aliases WHERE alias_session_key=?1)",[&result],|r|r.get::<_,bool>(0))?{return Err(ErrorCode::DbCorrupt.into());}Ok(result)
        })
    }
    pub fn session_accounting(&self, session: &str) -> StoreResult<SessionAccounting> {
        self.accounting_snapshot(session, None)
    }
    pub fn session_file_accounting(
        &self,
        session: &str,
        generation: &str,
    ) -> StoreResult<SessionAccounting> {
        self.accounting_snapshot(session, Some(generation))
    }
    fn accounting_snapshot(
        &self,
        session: &str,
        generation: Option<&str>,
    ) -> StoreResult<SessionAccounting> {
        self.snapshot(|tx,_| {
            let ledger_id:String=tx.query_row("SELECT active_ledger_id FROM sessions WHERE session_key=?1",[session],|r|r.get(0))?;
            let mut statement=tx.prepare("SELECT st.stream_key,st.episode_id,st.baseline_json,st.state_revision,o.normalized_json FROM stream_frontiers fr JOIN stream_states st ON st.ledger_id=fr.ledger_id AND st.stream_key=fr.stream_key AND st.episode_id=fr.episode_id LEFT JOIN observations o ON o.observation_id=st.last_observation_id WHERE st.ledger_id=?1 ORDER BY st.stream_key")?;
            let rows=statement.query_map([&ledger_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,i64>(3)?,r.get::<_,Option<String>>(4)?)))?;
            let mut state=AccountingState::new(session.into());let mut revisions=BTreeMap::new();
            for row in rows {let (key,episode,baseline,revision,record)=row?;
                let last_snapshot=match record.map(|s|serde_json::from_str::<NormalizedObservation>(&s)).transpose()? {Some(NormalizedObservation::Usage(u))=>u.last,_=>None};
                let cumulative:UsageVector=serde_json::from_str(&baseline)?;cumulative.validated_total()?;
                revisions.insert((key.clone(),episode.clone()),revision);
                state.streams.insert(key.clone(),StreamBaseline{stream_key:key,episode_id:episode,cumulative,last_snapshot});
            }
            if state.streams.len()>token_pulse_core::accounting::MAX_STREAMS {return Err(ErrorCode::InvalidUsage.into());}
            let canonical=if let Some(generation)=generation {
                let cursor:Option<(i64,String)>=tx.query_row("SELECT next_ordinal,state FROM file_usage_cursors WHERE ledger_id=?1 AND file_generation_id=?2",params![ledger_id,generation],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
                if let Some((cursor,status))=cursor {
                    let length:i64=tx.query_row("SELECT COUNT(*) FROM canonical_usage_sequence WHERE ledger_id=?1",[&ledger_id],|r|r.get(0))?;
                    if cursor>length{return Err(ErrorCode::DbCorrupt.into());}
                    let mut slots=vec![];
                    if status=="aligned" {
                        let mut q=tx.prepare("SELECT c.ordinal,c.observation_id,o.normalized_json FROM canonical_usage_sequence c JOIN observations o ON o.observation_id=c.observation_id WHERE c.ledger_id=?1 AND c.ordinal>=?2 ORDER BY c.ordinal LIMIT 500")?;
                        let mut rows=q.query(params![ledger_id,cursor])?;
                        while let Some(row)=rows.next()? {
                            let encoded:String=row.get(2)?;if encoded.len()>16*1024*1024{return Err(ErrorCode::DbCorrupt.into());}
                            let NormalizedObservation::Usage(u)=serde_json::from_str(&encoded)? else{return Err(ErrorCode::DbCorrupt.into());};
                            slots.push(CanonicalSlot{ordinal:row.get(0)?,observation_id:row.get(1)?,signature:token_pulse_core::sequence::UsageSignature::from(&u),reference_usage:u.last.or(u.cumulative)});
                        }
                    }
                    let head_proven:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM observations WHERE file_generation_id=?1 AND session_key=?2 AND byte_offset=0 AND kind='session_meta')",params![generation,session],|r|r.get(0))?;
                    Some(CanonicalFileState{cursor,length,aligned:status=="aligned",head_proven,slots})
                }else{None}
            }else{None};
            Ok(SessionAccounting{ledger_id,state,revisions,canonical})
        })
    }
}
