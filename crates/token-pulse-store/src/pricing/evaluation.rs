//! Verify the rule selected by the fixed catalog against retained immutable rows.
use super::*;
use rusqlite::OptionalExtension;
use token_pulse_core::{
    pricing::{
        PriceEvaluation, PriceOutcome, PricingEvent, SelectedPrice, offline::RequestPriceEvidence,
    },
    protocol::PriceBasis,
};

pub(crate) fn evaluate(
    tx: &Transaction<'_>,
    catalog: &PriceCatalog,
    event: &PricingEvent<'_>,
    basis: &PriceBasis,
    request: Option<RequestPriceEvidence<'_>>,
) -> StoreResult<PriceEvaluation> {
    let evaluation = catalog.evaluate_with_request(event, basis, request);
    if let Some(selection) = &evaluation.selection {
        verify_selection(tx, selection, &evaluation.outcome)?;
    }
    Ok(evaluation)
}
fn enum_key(value: impl serde::Serialize) -> StoreResult<String> {
    serde_json::to_value(value)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| ErrorCode::DbCorrupt.into())
}
fn verify_selection(
    tx: &Transaction<'_>,
    selection: &SelectedPrice,
    outcome: &PriceOutcome,
) -> StoreResult<()> {
    let expected = selection.rule();
    let mut statement = tx.prepare(&format!(
        "SELECT {COLUMNS} FROM price_rules WHERE rule_id=?1"
    ))?;
    let mut rows = statement.query([&expected.rule_id])?;
    let stored = read_rule(rows.next()?.ok_or(ErrorCode::DbCorrupt)?)?;
    if serde_json::to_value(&stored)? != serde_json::to_value(expected)? {
        return Err(ErrorCode::DbCorrupt.into());
    }
    let (flag, identity): (i64, Option<(String, String, String, String)>) = (
        tx.query_row("SELECT request_conditional FROM price_rules WHERE rule_id=?1", [&expected.rule_id], |row|row.get(0))?,
        tx.query_row("SELECT catalog_id,model_exact,tier,context_band FROM conditional_price_rules WHERE rule_id=?1", [&expected.rule_id], |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional()?,
    );
    match selection {
        SelectedPrice::Rule(_) | SelectedPrice::OfflineStandardReference { .. }
            if flag == 0 && identity.is_none() => {}
        SelectedPrice::Request(reference)
            if flag == 1
                && identity
                    == Some((
                        reference.catalog_id.clone(),
                        expected.model_exact.clone(),
                        enum_key(reference.tier)?,
                        enum_key(reference.context)?,
                    )) => {}
        _ => return Err(ErrorCode::DbCorrupt.into()),
    }
    if let PriceOutcome::Priced {
        rule_id, currency, ..
    } = outcome
    {
        if rule_id != &expected.rule_id || currency != &expected.currency {
            return Err(ErrorCode::DbCorrupt.into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
