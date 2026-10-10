use crate::{
    Database, ErrorCode, StoreResult,
    rusqlite::{OptionalExtension, TransactionBehavior, params},
};
use sha2::{Digest, Sha256};
use token_pulse_core::{
    numeric::{DecimalInt, EpochMs},
    sources::*,
};

pub enum SourceMutation {
    Add(Vec<SourceCandidate>),
    Pause(String),
    Resume(String),
    RetainRemove(String),
}
pub(crate) fn configuration(
    value: &str,
    kind: &str,
) -> StoreResult<(SourceOrigin, bool, SourceCapabilities)> {
    let mut value: serde_json::Value = serde_json::from_str(value)?;
    let map = value.as_object_mut().ok_or(ErrorCode::DbCorrupt)?;
    let origin = map
        .remove("configuration_origin")
        .map(serde_json::from_value)
        .transpose()?
        .unwrap_or(if kind == "wsl" {
            SourceOrigin::Wsl
        } else {
            SourceOrigin::Custom
        });
    let removed = map
        .remove("configuration_removed")
        .map(serde_json::from_value)
        .transpose()?
        .unwrap_or(false);
    Ok((origin, removed, serde_json::from_value(value)?))
}
pub(crate) fn encoded(
    origin: SourceOrigin,
    removed: bool,
    capabilities: SourceCapabilities,
) -> StoreResult<String> {
    let mut value = serde_json::to_value(capabilities)?;
    value["configuration_origin"] = serde_json::to_value(origin)?;
    value["configuration_removed"] = removed.into();
    Ok(serde_json::to_string(&value)?)
}
impl Database {
    pub fn sources_snapshot(&self) -> StoreResult<SourcesSnapshot> {
        self.light_snapshot(|tx,revision| {
            let mut s=tx.prepare("SELECT source_id,root_path,kind,enabled,readability,capabilities_json,last_scan_at_ms,last_success_at_ms FROM sources ORDER BY created_at_ms,source_id")?;
            let rows=s.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,bool>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?,r.get::<_,Option<i64>>(6)?,r.get::<_,Option<i64>>(7)?)))?;
            let mut sources=vec![];
            for row in rows {let (source_id,root_path,kind,enabled,readability,config,scan,success)=row?;let (origin,removed,capabilities)=configuration(&config,&kind)?;
                let readability=if enabled {serde_json::from_value(serde_json::Value::String(readability))?} else {SourceReadability::Disabled};
                sources.push(SourceSummary{source_id,root_path,origin,enabled,removed,readability,capabilities,last_scan_at_ms:scan.map(EpochMs::new).transpose()?,last_success_at_ms:success.map(EpochMs::new).transpose()?,error:if readability==SourceReadability::Unreadable {Some(ErrorCode::SourceUnreadable)} else {None}});
            }
            Ok(SourcesSnapshot{settings_revision:DecimalInt::from_nonnegative(i128::from(revision.settings))?,sources})
        })
    }
    pub fn mutate_sources(
        &self,
        mutation: SourceMutation,
        expected_settings_revision: i64,
        at_ms: i64,
    ) -> StoreResult<()> {
        // Resolve filesystem identity before entering the Writer or acquiring a database transaction.
        let additions = match &mutation {
            SourceMutation::Add(candidates) => {
                let mut result = vec![];
                for candidate in candidates {
                    validate_root(&candidate.root, candidate.origin)?;
                    result.push((
                        candidate
                            .root
                            .to_str()
                            .ok_or(ErrorCode::InvalidQuery)?
                            .to_owned(),
                        format!("canonical-path:{}", directory_key(&candidate.root)),
                        candidate.origin,
                    ));
                }
                result
            }
            _ => vec![],
        };
        self.write(move|conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let revision:i64=tx.query_row("SELECT settings_revision FROM app_state",[],|r|r.get(0))?;
            if revision!=expected_settings_revision {return Err(ErrorCode::RevisionConflict.into());}
            let mut changed=false;
            match mutation {
                SourceMutation::Add(_)=>{
                    for (root,directory_identity,origin) in additions {
                        let existing:Option<(String,bool)>=tx.query_row("SELECT source_id,enabled FROM sources WHERE provider='codex' AND directory_identity=?1",[&directory_identity],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
                        if existing.is_some() {continue;}
                        let source_id=format!("source-{:x}",Sha256::digest(directory_identity.as_bytes()));
                        let config=encoded(origin,false,SourceCapabilities::default())?;
                        let kind=if origin==SourceOrigin::Wsl {"wsl"} else {"local"};
                        tx.execute("INSERT INTO sources(source_id,provider,root_path,directory_identity,kind,enabled,readability,capabilities_json,created_at_ms) VALUES(?1,'codex',?2,?3,?4,1,'awaiting_directory',?5,?6)",params![source_id,root,directory_identity,kind,config,at_ms])?;changed=true;
                    }
                },
                action=>{
                    let (source_id,enabled,removed)=match action {SourceMutation::Pause(s)=>(s,false,false),SourceMutation::Resume(s)=>(s,true,false),SourceMutation::RetainRemove(s)=>(s,false,true),_=>unreachable!()};
                    let (kind,config):(String,String)=tx.query_row("SELECT kind,capabilities_json FROM sources WHERE source_id=?1",[&source_id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?.ok_or(ErrorCode::InvalidQuery)?;
                    let (origin,_,capabilities)=configuration(&config,&kind)?;
                    tx.execute("UPDATE sources SET enabled=?1,retained=1,capabilities_json=?2 WHERE source_id=?3",params![enabled,encoded(origin,removed,capabilities)?,source_id])?;changed=true;
                }
            }
            if changed {let next=revision.checked_add(1).ok_or(ErrorCode::NumericOverflow)?;tx.execute("UPDATE app_state SET settings_revision=?1",[next])?;}
            tx.commit()?;Ok(())
        })
    }
}
