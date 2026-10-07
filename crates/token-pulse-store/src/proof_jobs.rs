//! Quiescent collector scans enqueue one idempotent proof job for actual changed evidence.
use crate::{
    Database, ErrorCode, StoreResult, jobs,
    rusqlite::{OptionalExtension, Transaction, TransactionBehavior},
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use token_pulse_core::{
    domain::{
        ACCOUNTING_VERSION, LEGACY_ACCOUNTING_VERSION, PARSER_VERSION, PREVIOUS_ACCOUNTING_VERSION,
        can_upgrade_accounting_version,
    },
    jobs::{JobRequest, JobScope},
    numeric::EpochMs,
    protocol::{Job, JobKind, JobState},
};
fn busy(tx: &Transaction<'_>) -> StoreResult<bool> {
    Ok(tx.query_row("SELECT EXISTS(SELECT 1 FROM jobs WHERE state IN ('queued','running','validating','publishing','cancelling'))",[],|r|r.get(0))?)
}
#[derive(Clone, Copy)]
enum Trigger {
    Proof,
    AccountingUpgrade,
}
fn request(tx: &Transaction<'_>, trigger: Trigger) -> StoreResult<Option<(String, JobRequest)>> {
    if busy(tx)? {
        return Ok(None);
    }
    let seeds = match trigger {
        Trigger::Proof => {
            let mut q=tx.prepare("SELECT s.session_key FROM sessions s WHERE s.session_key NOT IN (SELECT alias_session_key FROM session_aliases) AND EXISTS(SELECT 1 FROM pending_usage p WHERE p.ledger_id=s.active_ledger_id AND p.kind='pending') AND (EXISTS(SELECT 1 FROM file_usage_cursors c WHERE c.ledger_id=s.active_ledger_id AND c.state='rebuild_required') OR EXISTS(SELECT 1 FROM sessions peer LEFT JOIN session_aliases a ON a.alias_session_key=peer.session_key WHERE peer.active_ledger_id IS NOT NULL AND peer.provider=s.provider AND peer.provider_session_id=s.provider_session_id AND COALESCE(a.canonical_session_key,peer.session_key)<>s.session_key) OR EXISTS(SELECT 1 FROM sessions parent WHERE parent.active_ledger_id IS NOT NULL AND parent.provider=s.provider AND (parent.session_key=s.parent_key OR parent.provider_session_id=s.parent_provider_id))) ORDER BY s.session_key LIMIT 32768")?;
            q.query_map([], |r| r.get::<_, String>(0))?
                .collect::<crate::rusqlite::Result<Vec<_>>>()?
        }
        Trigger::AccountingUpgrade => {
            // Parser upgrades need a separately verified reparse. Stored
            // observations may be replayed only under their current parser.
            let mut q=tx.prepare("SELECT s.session_key FROM sessions s JOIN ledger_generations l ON l.ledger_id=s.active_ledger_id WHERE l.state='active' AND l.parser_version=?1 AND l.accounting_version IN (?2,?3) AND s.session_key NOT IN (SELECT alias_session_key FROM session_aliases) ORDER BY s.session_key LIMIT 32768")?;
            q.query_map(
                [
                    PARSER_VERSION,
                    LEGACY_ACCOUNTING_VERSION,
                    PREVIOUS_ACCOUNTING_VERSION,
                ],
                |r| r.get::<_, String>(0),
            )?
            .collect::<crate::rusqlite::Result<Vec<_>>>()?
        }
    };
    let mut handled = BTreeSet::new();
    for seed in seeds {
        if handled.contains(&seed) {
            continue;
        }
        let scope = JobScope::Sessions {
            session_keys: vec![seed],
        };
        let closure = crate::rebuild::dependency_closure(tx, &scope)?;
        let scope = JobScope::Sessions {
            session_keys: vec![closure.first().ok_or(ErrorCode::InvalidQuery)?.clone()],
        };
        handled.extend(closure.iter().cloned());
        let mut hash = Sha256::new();
        hash.update(serde_json::to_vec(&(PARSER_VERSION, ACCOUNTING_VERSION))?);
        let mut files = BTreeSet::new();
        let mut complete_parser = true;
        for session in closure {
            let identity:(String,Option<String>,Option<String>,Option<String>,Option<i64>)=tx.query_row("SELECT provider,provider_session_id,parent_key,parent_provider_id,created_at_ms FROM sessions WHERE session_key=?1",[&session],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
            hash.update(serde_json::to_vec(&(&session, identity))?);
            if matches!(trigger, Trigger::AccountingUpgrade) {
                let ledger:(String,String,String)=tx.query_row("SELECT l.ledger_id,l.parser_version,l.accounting_version FROM sessions s JOIN ledger_generations l ON l.ledger_id=s.active_ledger_id WHERE s.session_key=?1",[&session],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
                if ledger.1 != PARSER_VERSION
                    || (ledger.2 != ACCOUNTING_VERSION
                        && !can_upgrade_accounting_version(&ledger.2))
                {
                    complete_parser = false;
                }
                hash.update(serde_json::to_vec(&ledger)?);
            }
            files.extend(crate::rebuild::current_inputs(tx, &session)?);
        }
        let mut complete =
            complete_parser && (matches!(trigger, Trigger::AccountingUpgrade) || !files.is_empty());
        for generation in files {
            let input:(i64,i64,String,String,Option<String>)=tx.query_row("SELECT g.committed_offset,g.observed_size,g.identity_json,g.anchor_json,f.current_generation_id FROM file_generations g JOIN source_files f ON f.file_id=g.file_id WHERE g.file_generation_id=?1",[&generation],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
            if matches!(trigger, Trigger::Proof) && input.0 != input.1 {
                complete = false;
                break;
            }
            if matches!(trigger, Trigger::AccountingUpgrade) {
                let availability:(String,bool,String,String)=tx.query_row("SELECT source.source_id,source.enabled,source.root_path,f.canonical_path FROM file_generations g JOIN source_files f ON f.file_id=g.file_id JOIN sources source ON source.source_id=f.source_id WHERE g.file_generation_id=?1",[&generation],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
                hash.update(serde_json::to_vec(&availability)?);
            }
            // Empty polling commits and ledger publication are bookkeeping, not new evidence.
            hash.update(serde_json::to_vec(&(generation, input))?);
        }
        if !complete {
            continue;
        }
        let fingerprint = format!("{:x}", hash.finalize());
        let purpose = match trigger {
            Trigger::Proof => "proof",
            Trigger::AccountingUpgrade => "accounting-upgrade",
        };
        for retry in 0..3 {
            let key = format!("{purpose}-rebuild:{fingerprint}:{retry}");
            let existing: Option<String> = tx
                .query_row(
                    "SELECT job_id FROM jobs WHERE request_key=?1",
                    [&key],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(id) = existing {
                let old = jobs::load(tx, &id)?;
                if old.job.state == JobState::Interrupted
                    || (old.job.state == JobState::Failed
                        && old
                            .job
                            .error
                            .as_ref()
                            .is_some_and(|e| e.code == ErrorCode::CandidateObsolete))
                {
                    continue;
                }
                break;
            }
            let mut request = JobRequest {
                kind: JobKind::Rebuild,
                scope: scope.clone(),
                request_key: key,
            };
            request.validate()?;
            return Ok(Some((
                format!("auto-{purpose}-{fingerprint}-{retry}"),
                request,
            )));
        }
    }
    Ok(None)
}
impl Database {
    pub fn enqueue_proof_rebuild(&self, at_ms: i64) -> StoreResult<Option<Job>> {
        self.enqueue_engine_rebuild(at_ms, Trigger::Proof)
    }
    pub fn enqueue_accounting_upgrade(&self, at_ms: i64) -> StoreResult<Option<Job>> {
        self.enqueue_engine_rebuild(at_ms, Trigger::AccountingUpgrade)
    }
    fn enqueue_engine_rebuild(&self, at_ms: i64, trigger: Trigger) -> StoreResult<Option<Job>> {
        EpochMs::new(at_ms)?;
        let Some((id, request)) = self.snapshot(|tx, _| request(tx, trigger))? else {
            return Ok(None);
        };
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if busy(&tx)?
                || tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM jobs WHERE request_key=?1)",
                    [&request.request_key],
                    |r| r.get::<_, bool>(0),
                )?
            {
                return Ok(None);
            }
            if matches!(trigger,Trigger::AccountingUpgrade) {
                let group=crate::rebuild::dependency_closure(&tx,&request.scope)?;
                let mut outdated=false;
                for session in group {
                    let (parser,accounting):(String,String)=tx.query_row("SELECT l.parser_version,l.accounting_version FROM sessions s JOIN ledger_generations l ON l.ledger_id=s.active_ledger_id WHERE s.session_key=?1",[session],|r|Ok((r.get(0)?,r.get(1)?)))?;
                    if parser!=PARSER_VERSION || (accounting!=ACCOUNTING_VERSION && !can_upgrade_accounting_version(&accounting)) {return Ok(None);}
                    outdated|=accounting!=ACCOUNTING_VERSION;
                }
                if !outdated {return Ok(None);}
            }
            let job = jobs::create_in_tx(&tx, &id, &request, at_ms)?;
            tx.commit()?;
            Ok(Some(job))
        })
    }
}

#[cfg(test)]
mod tests;
