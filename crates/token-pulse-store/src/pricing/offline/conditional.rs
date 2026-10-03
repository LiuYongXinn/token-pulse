//! Deterministic quote materialization; never activate a mode or mutate catalog facts.
use super::*;
use std::collections::BTreeMap;
use token_pulse_core::pricing::offline::SelectedRequestReference;

fn enum_key(value: impl serde::Serialize) -> StoreResult<String> {
    serde_json::to_value(value)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| ErrorCode::DbCorrupt.into())
}

pub(super) fn materialize_all(tx: &Transaction<'_>) -> StoreResult<()> {
    let mut statement = tx.prepare("SELECT catalog_id,catalog_json,content_sha256,introduced_revision,verified_at_ms,installed_at_ms FROM offline_price_catalogs ORDER BY introduced_revision")?;
    let publications = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    for (id, json, digest, revision, verified, installed) in publications {
        let catalog: OfflinePriceCatalog =
            serde_json::from_str(&json).map_err(|_| ErrorCode::DbCorrupt)?;
        catalog.validate().map_err(|_| ErrorCode::DbCorrupt)?;
        if id != catalog.catalog_id
            || digest != format!("{:x}", Sha256::digest(json.as_bytes()))
            || verified != catalog.verified_at_ms.value()
            || installed < verified
            || revision <= 0
        {
            return Err(ErrorCode::DbCorrupt.into());
        }
        let references = catalog
            .request_reference_rules(decimal(revision)?, EpochMs::new(installed)?)
            .map_err(|_| ErrorCode::DbCorrupt)?;
        materialize(tx, &id, references)?;
    }
    Ok(())
}

fn materialize(
    tx: &Transaction<'_>,
    catalog_id: &str,
    references: Vec<SelectedRequestReference>,
) -> StoreResult<()> {
    let mut statement = tx.prepare("SELECT rule_id,model_exact,tier,context_band FROM conditional_price_rules WHERE catalog_id=?1 LIMIT 1025")?;
    let existing = statement
        .query_map([catalog_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                (
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ),
            ))
        })?
        .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
    drop(statement);
    if !existing.is_empty() && existing.len() != references.len() {
        // An interrupted publication cannot leave a partial set after an atomic commit.
        return Err(ErrorCode::DbCorrupt.into());
    }
    for reference in references {
        let rule = reference.rule;
        let expected = (
            rule.model_exact.clone(),
            enum_key(reference.tier)?,
            enum_key(reference.context)?,
        );
        if existing.is_empty() {
            insert_rule(tx, &rule, true)?;
            tx.execute("INSERT INTO conditional_price_rules(rule_id,catalog_id,model_exact,tier,context_band) VALUES(?1,?2,?3,?4,?5)",params![rule.rule_id,catalog_id,expected.0,expected.1,expected.2])?;
        } else {
            if existing.get(&rule.rule_id) != Some(&expected) {
                return Err(ErrorCode::DbCorrupt.into());
            }
            let mut statement = tx.prepare(&format!(
                "SELECT {COLUMNS} FROM price_rules WHERE rule_id=?1"
            ))?;
            let mut rows = statement.query([&rule.rule_id])?;
            let stored = read_rule(rows.next()?.ok_or(ErrorCode::DbCorrupt)?)?;
            let flag: i64 = tx.query_row(
                "SELECT request_conditional FROM price_rules WHERE rule_id=?1",
                [&rule.rule_id],
                |r| r.get(0),
            )?;
            if flag != 1 || serde_json::to_value(&stored)? != serde_json::to_value(&rule)? {
                return Err(ErrorCode::DbCorrupt.into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
