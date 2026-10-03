//! Versioned price configuration. Published rates never change in place.
use crate::{Database, ErrorCode, StoreResult};
use rusqlite::{Row, Transaction, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use token_pulse_core::{
    numeric::{DecimalInt, EpochMs},
    pricing::{
        ModelAlias, ModelAliasMutation, PriceCatalog, PriceRule, PriceRuleMutation,
        PriceRulesSnapshot,
    },
};

const COLUMNS: &str = "rule_id,introduced_revision,retired_revision,provider,model_exact,source_id,currency,effective_from_ms,effective_to_ms,priority,input_rate_atoms,cached_rate_atoms,output_rate_atoms,origin,origin_reference,created_at_ms,cache_write_rate_atoms";
mod offline;
fn decimal(value: i64) -> StoreResult<DecimalInt> {
    DecimalInt::from_nonnegative(value.into()).map_err(|_| ErrorCode::DbCorrupt.into())
}
fn read_rule(row: &Row<'_>) -> StoreResult<PriceRule> {
    let rule = PriceRule {
        rule_id: row.get(0)?,
        introduced_revision: decimal(row.get(1)?)?,
        retired_revision: row.get::<_, Option<i64>>(2)?.map(decimal).transpose()?,
        provider: row.get(3)?,
        model_exact: row.get(4)?,
        source_id: row.get(5)?,
        currency: row.get(6)?,
        effective_from_ms: EpochMs::new(row.get(7)?)?,
        effective_to_ms: row
            .get::<_, Option<i64>>(8)?
            .map(EpochMs::new)
            .transpose()?,
        priority: row.get(9)?,
        input_rate_atoms: DecimalInt::parse(&row.get::<_, String>(10)?)?,
        cache_write_rate_atoms: row
            .get::<_, Option<String>>(16)?
            .map(|s| DecimalInt::parse(&s))
            .transpose()?,
        cached_rate_atoms: row
            .get::<_, Option<String>>(11)?
            .map(|s| DecimalInt::parse(&s))
            .transpose()?,
        output_rate_atoms: DecimalInt::parse(&row.get::<_, String>(12)?)?,
        origin: serde_json::from_value(serde_json::Value::String(row.get(13)?))
            .map_err(|_| ErrorCode::DbCorrupt)?,
        origin_reference: row.get(14)?,
        created_at_ms: EpochMs::new(row.get(15)?)?,
    };
    rule.validate().map_err(|_| ErrorCode::DbCorrupt)?;
    Ok(rule)
}
pub fn rules_at(tx: &Transaction<'_>, revision: i64) -> StoreResult<PriceRulesSnapshot> {
    let mut statement=tx.prepare(&format!("SELECT {COLUMNS} FROM price_rules WHERE introduced_revision<=?1 AND (retired_revision IS NULL OR retired_revision>?1) ORDER BY provider,model_exact,rule_id LIMIT 4097"))?;
    let mut rows = statement.query([revision])?;
    let mut rules = Vec::new();
    while let Some(row) = rows.next()? {
        rules.push(read_rule(row)?);
    }
    if rules.len() > 4096 {
        return Err(ErrorCode::InvalidQuery.into());
    }
    drop(rows);
    drop(statement);
    let mut statement=tx.prepare("SELECT alias_id,provider,alias,canonical_model,introduced_revision,retired_revision FROM model_aliases WHERE introduced_revision<=?1 AND (retired_revision IS NULL OR retired_revision>?1) ORDER BY provider,alias,alias_id LIMIT 4097")?;
    let mut rows = statement.query([revision])?;
    let mut aliases = Vec::new();
    while let Some(row) = rows.next()? {
        let alias = ModelAlias {
            alias_id: row.get(0)?,
            provider: row.get(1)?,
            alias: row.get(2)?,
            canonical_model: row.get(3)?,
            introduced_revision: decimal(row.get(4)?)?,
            retired_revision: row.get::<_, Option<i64>>(5)?.map(decimal).transpose()?,
        };
        alias.validate().map_err(|_| ErrorCode::DbCorrupt)?;
        aliases.push(alias);
    }
    if aliases.len() > 4096 {
        return Err(ErrorCode::InvalidQuery.into());
    }
    Ok(PriceRulesSnapshot {
        price_revision: decimal(revision)?,
        rules,
        aliases,
    })
}
pub fn catalog_at(tx: &Transaction<'_>, revision: i64) -> StoreResult<PriceCatalog> {
    let rules = rules_at(tx, revision)?;
    let mut result = PriceCatalog::new(rules.rules, rules.aliases, rules.price_revision)?;
    if let Some(reference) = offline::snapshot_at(tx, revision)?.catalog {
        result = result
            .with_offline_reference(&reference)
            .map_err(|_| ErrorCode::DbCorrupt)?;
    }
    Ok(result)
}

fn apply(
    tx: &Transaction<'_>,
    mutation: PriceRuleMutation,
    expected: i64,
    at: EpochMs,
) -> StoreResult<i64> {
    if expected < 0 {
        return Err(ErrorCode::InvalidQuery.into());
    }
    let revision: i64 = tx.query_row(
        "SELECT price_revision FROM app_state WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    if revision != expected {
        return Err(ErrorCode::RevisionConflict.into());
    }
    let next = revision.checked_add(1).ok_or(ErrorCode::NumericOverflow)?;
    let (retire, draft) = match mutation {
        PriceRuleMutation::Create { draft } => (None, Some(draft)),
        PriceRuleMutation::Replace { rule_id, draft } => (Some(rule_id), Some(draft)),
        PriceRuleMutation::Retire { rule_id } => (Some(rule_id), None),
    };
    if let Some(id) = retire {
        if id.is_empty() || id.len() > 256 || id.chars().any(char::is_control) {
            return Err(ErrorCode::InvalidQuery.into());
        }
        if tx.execute("UPDATE price_rules SET retired_revision=?1 WHERE rule_id=?2 AND introduced_revision<=?3 AND retired_revision IS NULL AND origin='custom'",params![next,id,revision])?!=1 {return Err(ErrorCode::InvalidQuery.into());}
    }
    if let Some(draft) = draft {
        draft.validate()?;
        if let Some(source) = &draft.source_id {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM sources WHERE source_id=?1)",
                [source],
                |r| r.get(0),
            )?;
            if !exists {
                return Err(ErrorCode::InvalidQuery.into());
            }
        }
        let overlaps:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM price_rules WHERE introduced_revision<=?1 AND (retired_revision IS NULL OR retired_revision>?1) AND provider=?2 AND model_exact=?3 AND source_id IS ?4 AND priority=?5 AND origin='custom' AND (?6 IS NULL OR effective_from_ms<?6) AND (effective_to_ms IS NULL OR effective_to_ms>?7))",params![next,draft.provider,draft.model_exact,draft.source_id,draft.priority,draft.effective_to_ms.map(|t|t.value()),draft.effective_from_ms.value()],|r|r.get(0))?;
        if overlaps {
            return Err(ErrorCode::PriceRuleConflict.into());
        }
        let count:i64=tx.query_row("SELECT COUNT(*) FROM price_rules WHERE introduced_revision<=?1 AND (retired_revision IS NULL OR retired_revision>?1)",[next],|r|r.get(0))?;
        if count >= 4096 {
            return Err(ErrorCode::InvalidQuery.into());
        }
        let id = format!(
            "price-{:x}",
            Sha256::digest(serde_json::to_vec(&(next, &draft, at))?)
        );
        let rule = draft.into_rule(id, decimal(next)?, at);
        tx.execute("INSERT INTO price_rules(rule_id,introduced_revision,retired_revision,provider,model_exact,source_id,currency,effective_from_ms,effective_to_ms,priority,input_rate_atoms,cached_rate_atoms,output_rate_atoms,origin,origin_reference,created_at_ms,cache_write_rate_atoms) VALUES(?1,?2,NULL,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,'custom',?13,?14,?15)",params![rule.rule_id,next,rule.provider,rule.model_exact,rule.source_id,rule.currency,rule.effective_from_ms.value(),rule.effective_to_ms.map(|t|t.value()),rule.priority,rule.input_rate_atoms.as_str(),rule.cached_rate_atoms.as_ref().map(|v|v.as_str()),rule.output_rate_atoms.as_str(),rule.origin_reference,rule.created_at_ms.value(),rule.cache_write_rate_atoms.as_ref().map(|v|v.as_str())])?;
    }
    tx.execute(
        "UPDATE app_state SET price_revision=?1 WHERE singleton=1",
        [next],
    )?;
    Ok(next)
}
impl Database {
    /// Exact one-hop aliases share the immutable price revision; consumption is untouched.
    pub fn mutate_model_alias_snapshot(
        &self,
        mutation: ModelAliasMutation,
        expected_revision: i64,
        at_ms: i64,
    ) -> StoreResult<PriceRulesSnapshot> {
        let at = EpochMs::new(at_ms)?;
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let revision = apply_alias(&tx, mutation, expected_revision, at)?;
            let result = rules_at(&tx, revision)?;
            tx.commit()?;
            Ok(result)
        })
    }
    pub fn price_rules(&self) -> StoreResult<PriceRulesSnapshot> {
        self.snapshot(|tx, revision| rules_at(tx, revision.price))
    }
    pub fn price_rules_at(&self, requested: Option<i64>) -> StoreResult<PriceRulesSnapshot> {
        self.snapshot(|tx, current| {
            let revision = requested.unwrap_or(current.price);
            if revision < 0 || revision > current.price {
                return Err(ErrorCode::InvalidQuery.into());
            }
            rules_at(tx, revision)
        })
    }
    pub fn mutate_price_rule_snapshot(
        &self,
        mutation: PriceRuleMutation,
        expected_revision: i64,
        at_ms: i64,
    ) -> StoreResult<PriceRulesSnapshot> {
        let at = EpochMs::new(at_ms)?;
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let revision = apply(&tx, mutation, expected_revision, at)?;
            let result = rules_at(&tx, revision)?;
            tx.commit()?;
            Ok(result)
        })
    }
    pub fn mutate_price_rule(
        &self,
        mutation: PriceRuleMutation,
        expected_revision: i64,
        at_ms: i64,
    ) -> StoreResult<i64> {
        let at = EpochMs::new(at_ms)?;
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let revision = apply(&tx, mutation, expected_revision, at)?;
            tx.commit()?;
            Ok(revision)
        })
    }
}

fn apply_alias(
    tx: &Transaction<'_>,
    mutation: ModelAliasMutation,
    expected: i64,
    at: EpochMs,
) -> StoreResult<i64> {
    if expected < 0 {
        return Err(ErrorCode::InvalidQuery.into());
    }
    let revision: i64 = tx.query_row(
        "SELECT price_revision FROM app_state WHERE singleton=1",
        [],
        |row| row.get(0),
    )?;
    if revision != expected {
        return Err(ErrorCode::RevisionConflict.into());
    }
    let next = revision.checked_add(1).ok_or(ErrorCode::NumericOverflow)?;
    let (retire, draft) = match mutation {
        ModelAliasMutation::Create { draft } => (None, Some(draft)),
        ModelAliasMutation::Replace { alias_id, draft } => (Some(alias_id), Some(draft)),
        ModelAliasMutation::Retire { alias_id } => (Some(alias_id), None),
    };
    if let Some(id) = retire {
        // This namespace is generated by user mutations only; imported catalog rows are read-only.
        if !id.starts_with("alias-custom-") || id.len() > 256 || id.chars().any(char::is_control) {
            return Err(ErrorCode::InvalidQuery.into());
        }
        if tx.execute("UPDATE model_aliases SET retired_revision=?1 WHERE alias_id=?2 AND introduced_revision<=?3 AND retired_revision IS NULL", params![next, id, revision])? != 1 {
            return Err(ErrorCode::InvalidQuery.into());
        }
    }
    if let Some(draft) = draft {
        draft.validate()?;
        // A canonical target must remain canonical. Prevent both chain directions and duplicate
        // mappings; ambiguous legacy/imported data stays isolated by the existing catalog.
        let conflict: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM model_aliases WHERE introduced_revision<=?1 AND (retired_revision IS NULL OR retired_revision>?1) AND provider=?2 AND (alias=?3 OR alias=?4 OR canonical_model=?3))", params![next, draft.provider, draft.alias, draft.canonical_model], |row| row.get(0))?;
        if conflict {
            return Err(ErrorCode::PriceRuleConflict.into());
        }
        let count: i64 = tx.query_row("SELECT COUNT(*) FROM model_aliases WHERE introduced_revision<=?1 AND (retired_revision IS NULL OR retired_revision>?1)", [next], |row| row.get(0))?;
        if count >= 4096 {
            return Err(ErrorCode::InvalidQuery.into());
        }
        let id = format!(
            "alias-custom-{:x}",
            Sha256::digest(serde_json::to_vec(&(next, &draft, at))?)
        );
        tx.execute("INSERT INTO model_aliases(alias_id,provider,alias,canonical_model,introduced_revision,retired_revision) VALUES(?1,?2,?3,?4,?5,NULL)", params![id, draft.provider, draft.alias, draft.canonical_model, next])?;
    }
    tx.execute(
        "UPDATE app_state SET price_revision=?1 WHERE singleton=1",
        [next],
    )?;
    Ok(next)
}

#[cfg(test)]
mod alias_tests;
#[cfg(test)]
mod tests;
