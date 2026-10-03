use super::*;
use token_pulse_core::pricing::offline::{
    OfflinePriceCatalog, OfflinePriceCatalogSnapshot, OfflineReferencePublication,
    historical_flat_rule_id,
};
mod conditional;

fn insert_rule(tx: &Transaction<'_>, rule: &PriceRule, conditional: bool) -> StoreResult<()> {
    rule.validate()?;
    tx.execute("INSERT INTO price_rules(rule_id,introduced_revision,retired_revision,provider,model_exact,source_id,currency,effective_from_ms,effective_to_ms,priority,input_rate_atoms,cached_rate_atoms,output_rate_atoms,origin,origin_reference,created_at_ms,cache_write_rate_atoms,request_conditional) VALUES(?1,?2,NULL,?3,?4,NULL,?5,?6,?7,?8,?9,?10,?11,'offline',?12,?13,?14,?15)",params![rule.rule_id,i64::try_from(rule.introduced_revision.value()).map_err(|_|ErrorCode::NumericOverflow)?,rule.provider,rule.model_exact,rule.currency,rule.effective_from_ms.value(),rule.effective_to_ms.map(|v|v.value()),rule.priority,rule.input_rate_atoms.as_str(),rule.cached_rate_atoms.as_ref().map(|v|v.as_str()),rule.output_rate_atoms.as_str(),rule.origin_reference,rule.created_at_ms.value(),rule.cache_write_rate_atoms.as_ref().map(|v|v.as_str()),conditional])?;
    Ok(())
}
pub(super) fn snapshot_at(
    tx: &Transaction<'_>,
    revision: i64,
) -> StoreResult<OfflinePriceCatalogSnapshot> {
    let mut statement = tx.prepare("SELECT catalog_id,catalog_json,content_sha256,verified_at_ms FROM offline_price_catalogs WHERE introduced_revision<=?1 ORDER BY introduced_revision DESC LIMIT 1")?;
    let mut rows = statement.query([revision])?;
    let catalog = if let Some(row) = rows.next()? {
        let json: String = row.get(1)?;
        let catalog: OfflinePriceCatalog =
            serde_json::from_str(&json).map_err(|_| ErrorCode::DbCorrupt)?;
        catalog.validate().map_err(|_| ErrorCode::DbCorrupt)?;
        if catalog.catalog_id != row.get::<_, String>(0)?
            || catalog.verified_at_ms.value() != row.get::<_, i64>(3)?
            || format!("{:x}", Sha256::digest(json.as_bytes())) != row.get::<_, String>(2)?
        {
            return Err(ErrorCode::DbCorrupt.into());
        }
        Some(catalog)
    } else {
        None
    };
    Ok(OfflinePriceCatalogSnapshot {
        price_revision: decimal(revision)?,
        catalog,
    })
}
pub(super) fn history_at(
    tx: &Transaction<'_>,
    revision: i64,
) -> StoreResult<Vec<OfflineReferencePublication>> {
    let mut statement = tx.prepare("SELECT catalog_id,catalog_json,content_sha256,verified_at_ms,introduced_revision,installed_at_ms FROM offline_price_catalogs WHERE introduced_revision<=?1 ORDER BY introduced_revision")?;
    let mut rows = statement.query([revision])?;
    let mut history = Vec::new();
    while let Some(row) = rows.next()? {
        let json: String = row.get(1)?;
        let catalog: OfflinePriceCatalog =
            serde_json::from_str(&json).map_err(|_| ErrorCode::DbCorrupt)?;
        catalog.validate().map_err(|_| ErrorCode::DbCorrupt)?;
        let introduced: i64 = row.get(4)?;
        let installed = EpochMs::new(row.get(5)?).map_err(|_| ErrorCode::DbCorrupt)?;
        if catalog.catalog_id != row.get::<_, String>(0)?
            || catalog.verified_at_ms.value() != row.get::<_, i64>(3)?
            || format!("{:x}", Sha256::digest(json.as_bytes())) != row.get::<_, String>(2)?
            || introduced <= 0
            || installed < catalog.verified_at_ms
        {
            return Err(ErrorCode::DbCorrupt.into());
        }
        history.push(OfflineReferencePublication {
            catalog,
            introduced_revision: decimal(introduced)?,
            installed_at_ms: installed,
        });
    }
    Ok(history)
}
impl Database {
    /// Boot-time publication through the same price writer as user edits. Idempotent by content.
    pub fn install_offline_price_catalog(
        &self,
        catalog: OfflinePriceCatalog,
        at_ms: i64,
    ) -> StoreResult<i64> {
        catalog.validate()?;
        let at = EpochMs::new(at_ms)?;
        if at < catalog.verified_at_ms {
            return Err(ErrorCode::InvalidQuery.into());
        }
        let json = serde_json::to_string(&catalog)?;
        let digest = format!("{:x}", Sha256::digest(json.as_bytes()));
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let current: i64 = tx.query_row("SELECT price_revision FROM app_state WHERE singleton=1",[],|r|r.get(0))?;
            let mut statement = tx.prepare("SELECT content_sha256 FROM offline_price_catalogs WHERE catalog_id=?1")?;
            let existing = statement.query_map([&catalog.catalog_id], |r|r.get::<_,String>(0))?.next().transpose()?;
            drop(statement);
            if let Some(existing) = existing {
                if existing != digest { return Err(ErrorCode::PriceRuleConflict.into()); }
                // Deterministic quote metadata may be absent in pre-v14 installations.
                // Its identity derives from the original publication, not this boot time.
                conditional::materialize_all(&tx)?;
                catalog_at(&tx,current)?;
                tx.commit()?;
                return Ok(current);
            }
            if snapshot_at(&tx,current)?.catalog.is_some_and(|old|old.verified_at_ms > catalog.verified_at_ms) {
                return Err(ErrorCode::InvalidQuery.into());
            }
            let next = current.checked_add(1).ok_or(ErrorCode::NumericOverflow)?;
            let previous = rules_at(&tx,current)?;
            // Close the prior reference interval by publishing new immutable history rows.
            // Captured old revisions keep their original open interval and exact cost.
            for old in previous.rules.iter().filter(|r|r.origin == token_pulse_core::pricing::PriceOrigin::Offline && r.rule_id.starts_with("offline/")) {
                tx.execute("UPDATE price_rules SET retired_revision=?1 WHERE rule_id=?2 AND retired_revision IS NULL",params![next,old.rule_id])?;
                if old.effective_from_ms < catalog.verified_at_ms {
                    let mut history = old.clone();
                    history.rule_id = historical_flat_rule_id(next,&old.rule_id)?;
                    history.introduced_revision = decimal(next)?;
                    history.retired_revision = None;
                    history.effective_to_ms = Some(catalog.verified_at_ms);
                    history.created_at_ms = at;
                    insert_rule(&tx,&history,false)?;
                }
            }
            for rule in catalog.flat_standard_rules(decimal(next)?,at)? { insert_rule(&tx,&rule,false)?; }
            tx.execute("INSERT INTO offline_price_catalogs(catalog_id,content_sha256,catalog_json,introduced_revision,verified_at_ms,installed_at_ms) VALUES(?1,?2,?3,?4,?5,?6)",params![catalog.catalog_id,digest,json,next,catalog.verified_at_ms.value(),at.value()])?;
            conditional::materialize_all(&tx)?;
            // Validate the complete candidate before making its revision visible.
            catalog_at(&tx,next)?;
            tx.execute("UPDATE app_state SET price_revision=?1 WHERE singleton=1",[next])?;
            tx.commit()?;
            Ok(next)
        })
    }
    pub fn offline_price_catalog_at(
        &self,
        requested: Option<i64>,
    ) -> StoreResult<OfflinePriceCatalogSnapshot> {
        self.snapshot(|tx, current| {
            let revision = requested.unwrap_or(current.price);
            if revision < 0 || revision > current.price {
                return Err(ErrorCode::InvalidQuery.into());
            }
            snapshot_at(tx, revision)
        })
    }
}

#[cfg(test)]
mod history_tests;
#[cfg(test)]
mod tests;
