//! Bundled factual API references. Verification time is not a claim about historical rates.
use super::{MAX_RATE_ATOMS, PriceOrigin, PriceRule, key, rate_atoms};
use crate::{
    error::ErrorCode,
    numeric::{DecimalInt, EpochMs},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use ts_rs::TS;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum OfflinePriceTier {
    Standard,
    Batch,
    Flex,
    Fast,
    Ultrafast,
}
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum OfflineContextBand {
    All,
    Short,
    Long,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum OfflineReferenceBasis {
    GlobalApiReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct OfflinePriceEntry {
    pub model_exact: String,
    pub tier: OfflinePriceTier,
    pub context: OfflineContextBand,
    /// Currency units per million tokens, exact decimal strings (never f64).
    pub input_per_million: String,
    pub cached_per_million: Option<String>,
    pub cache_write_per_million: Option<String>,
    pub output_per_million: String,
    pub reference: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct OfflinePriceCatalog {
    pub format_version: u32,
    pub catalog_id: String,
    pub verified_at_ms: EpochMs,
    pub provider: String,
    pub currency: String,
    pub short_context_max_input: u32,
    pub reference_basis: OfflineReferenceBasis,
    pub entries: Vec<OfflinePriceEntry>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct OfflinePriceCatalogSnapshot {
    pub price_revision: DecimalInt,
    pub catalog: Option<OfflinePriceCatalog>,
}

fn official_reference(value: &str) -> bool {
    value == "https://developers.openai.com/api/docs/pricing"
        || value
            .strip_prefix("https://developers.openai.com/api/docs/models/")
            .is_some_and(|model| {
                !model.is_empty()
                    && model
                        .bytes()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b".-".contains(&c))
            })
}
impl OfflinePriceCatalog {
    pub fn bundled() -> Result<Self, ErrorCode> {
        let catalog: Self = serde_json::from_str(include_str!("../../data/offline-prices.json"))
            .map_err(|_| ErrorCode::InvalidQuery)?;
        catalog.validate()?;
        Ok(catalog)
    }
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if self.format_version != 1
            || !key(&self.catalog_id)
            || !self.catalog_id.starts_with("openai-text-")
            || self.catalog_id.len() > 96
            || !self
                .catalog_id
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            || self.provider != "openai"
            || self.currency != "USD"
            || self.short_context_max_input != 272_000
            || self.entries.is_empty()
            || self.entries.len() > 1024
        {
            return Err(ErrorCode::InvalidQuery);
        }
        let mut seen = BTreeSet::new();
        for entry in &self.entries {
            if !key(&entry.model_exact)
                || entry.model_exact.len() > 128
                || !entry
                    .model_exact
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b".-".contains(&c))
                || !official_reference(&entry.reference)
                || !seen.insert((&entry.model_exact, entry.tier, entry.context))
            {
                return Err(ErrorCode::InvalidQuery);
            }
            for value in [
                Some(&entry.input_per_million),
                entry.cached_per_million.as_ref(),
                entry.cache_write_per_million.as_ref(),
                Some(&entry.output_per_million),
            ]
            .into_iter()
            .flatten()
            {
                if rate_atoms(value)?.value() > MAX_RATE_ATOMS {
                    return Err(ErrorCode::InvalidQuery);
                }
            }
        }
        for (model, tier, context) in &seen {
            if *context == OfflineContextBand::All
                && (seen.contains(&(model, *tier, OfflineContextBand::Short))
                    || seen.contains(&(model, *tier, OfflineContextBand::Long)))
            {
                return Err(ErrorCode::InvalidQuery);
            }
        }
        Ok(())
    }
    /// Only references representable by the current aggregate usage vector enter matching.
    /// Request bands and separately charged cache writes require further evidence, never guesses.
    pub fn flat_standard_rules(
        &self,
        revision: DecimalInt,
        created_at: EpochMs,
    ) -> Result<Vec<PriceRule>, ErrorCode> {
        self.validate()?;
        self.entries
            .iter()
            .filter(|entry| {
                entry.tier == OfflinePriceTier::Standard
                    && entry.context == OfflineContextBand::All
                    && entry.cache_write_per_million.is_none()
            })
            .map(|entry| {
                let rule = PriceRule {
                    rule_id: format!("offline/{}/{}", self.catalog_id, entry.model_exact),
                    introduced_revision: revision.clone(),
                    retired_revision: None,
                    provider: self.provider.clone(),
                    model_exact: entry.model_exact.clone(),
                    source_id: None,
                    currency: self.currency.clone(),
                    effective_from_ms: self.verified_at_ms,
                    effective_to_ms: None,
                    priority: 0,
                    input_rate_atoms: rate_atoms(&entry.input_per_million)?,
                    cache_write_rate_atoms: None,
                    cached_rate_atoms: entry
                        .cached_per_million
                        .as_deref()
                        .map(rate_atoms)
                        .transpose()?,
                    output_rate_atoms: rate_atoms(&entry.output_per_million)?,
                    origin: PriceOrigin::Offline,
                    origin_reference: Some(entry.reference.clone()),
                    created_at_ms: created_at,
                };
                rule.validate()?;
                Ok(rule)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_facts_are_complete_exact_and_conditional_prices_cannot_be_flat_rules() {
        let catalog = OfflinePriceCatalog::bundled().unwrap();
        assert_eq!(catalog.entries.len(), 172);
        assert_eq!(
            catalog
                .entries
                .iter()
                .map(|e| &e.model_exact)
                .collect::<BTreeSet<_>>()
                .len(),
            51
        );
        let rules = catalog
            .flat_standard_rules(DecimalInt::parse("7").unwrap(), catalog.verified_at_ms)
            .unwrap();
        assert_eq!(rules.len(), 37);
        let codex = rules
            .iter()
            .find(|r| r.model_exact == "gpt-5.3-codex")
            .unwrap();
        assert_eq!(codex.input_rate_atoms.as_str(), "1750000000");
        assert_eq!(
            codex.cached_rate_atoms.as_ref().unwrap().as_str(),
            "175000000"
        );
        assert_eq!(codex.output_rate_atoms.as_str(), "14000000000");
        assert_eq!(codex.effective_from_ms.value(), 1_790_899_200_000);
        assert!(
            !rules
                .iter()
                .any(|r| r.model_exact == "gpt-6.1-sol" || r.model_exact == "gpt-5.5")
        );
        let fast = catalog
            .entries
            .iter()
            .find(|e| e.model_exact == "gpt-5.3-codex" && e.tier == OfflinePriceTier::Fast)
            .unwrap();
        assert_eq!(fast.input_per_million, "3.50");
        let long = catalog
            .entries
            .iter()
            .find(|e| {
                e.model_exact == "gpt-6.1-sol"
                    && e.tier == OfflinePriceTier::Standard
                    && e.context == OfflineContextBand::Long
            })
            .unwrap();
        assert_eq!(long.cache_write_per_million.as_deref(), Some("5.00"));
        assert!(
            !catalog
                .entries
                .iter()
                .any(|e| e.model_exact == "gpt-rosalind-research")
        );
    }
    #[test]
    fn catalog_rejects_ambiguous_bands_invalid_rates_sources_and_unknown_fields() {
        let original = OfflinePriceCatalog::bundled().unwrap();
        for change in 0..6 {
            let mut catalog = original.clone();
            match change {
                0 => catalog.entries.push(catalog.entries[0].clone()),
                1 => catalog.entries[0].input_per_million = "0.0000000001".into(),
                2 => {
                    catalog.entries[0].reference =
                        "https://developers.openai.com.evil/api/docs/pricing".into()
                }
                3 => {
                    let mut entry = catalog
                        .entries
                        .iter()
                        .find(|e| e.context == OfflineContextBand::Short)
                        .unwrap()
                        .clone();
                    entry.context = OfflineContextBand::All;
                    catalog.entries.push(entry);
                }
                4 => catalog.short_context_max_input = 272_001,
                _ => catalog.provider = "unknown".into(),
            }
            assert!(catalog.validate().is_err());
        }
        let mut value = serde_json::to_value(original).unwrap();
        value["entries"][0]["unverified"] = true.into();
        assert!(serde_json::from_value::<OfflinePriceCatalog>(value).is_err());
    }
}
