//! Public, bounded explanation of the same immutable selection, without raw evidence.
use super::*;
use offline::{OfflineContextBand, OfflinePriceTier, OfflineReferenceBasis};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PriceMatchBasis {
    CustomRule {
        source_specific: bool,
    },
    /// Standard is a reference assumption; the request's actual mode remains unknown.
    OfflineStandardReference {
        #[schemars(length(min = 1, max = 96))]
        catalog_id: String,
        reference_basis: OfflineReferenceBasis,
    },
    OfflineRule {},
    /// Standard is assumed; flags disclose missing context/write quantities.
    OfflineAssumedReference {
        #[schemars(length(min = 1, max = 96))]
        catalog_id: String,
        context: OfflineContextBand,
        context_assumed: bool,
        cache_write_assumed_zero: bool,
        reference_basis: OfflineReferenceBasis,
    },
    OfflineRequestReference {
        #[schemars(length(min = 1, max = 96))]
        catalog_id: String,
        actual_tier: OfflinePriceTier,
        context: OfflineContextBand,
        reference_basis: OfflineReferenceBasis,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MatchedPrice {
    #[schemars(length(min = 1, max = 256))]
    pub rule_id: String,
    #[schemars(length(min = 1, max = 256))]
    pub model_exact: String,
    pub introduced_revision: DecimalInt,
    pub basis: PriceMatchBasis,
}
impl PriceEvaluation {
    /// Cached amounts do not independently establish a request's mode. Expose only
    /// a selection whose identity/status agrees with this evaluation's result.
    pub fn matched_price(&self, outcome: &PriceOutcome) -> Option<MatchedPrice> {
        let agrees = match (&self.outcome, outcome) {
            (
                PriceOutcome::Priced {
                    rule_id: a,
                    currency: x,
                    ..
                },
                PriceOutcome::Priced {
                    rule_id: b,
                    currency: y,
                    ..
                },
            ) => a == b && x == y,
            (PriceOutcome::Unpriced { reason: a }, PriceOutcome::Unpriced { reason: b }) => a == b,
            _ => false,
        };
        if !agrees {
            return None;
        }
        let selected = self.selection.as_ref()?;
        let rule = selected.rule();
        let basis = match selected {
            SelectedPrice::Rule(rule) if rule.origin == PriceOrigin::Custom => {
                PriceMatchBasis::CustomRule {
                    source_specific: rule.source_id.is_some(),
                }
            }
            SelectedPrice::Rule(_) => PriceMatchBasis::OfflineRule {},
            SelectedPrice::OfflineStandardReference {
                catalog_id,
                reference_basis,
                ..
            } => PriceMatchBasis::OfflineStandardReference {
                catalog_id: catalog_id.clone(),
                reference_basis: *reference_basis,
            },
            SelectedPrice::Request(reference) => PriceMatchBasis::OfflineRequestReference {
                catalog_id: reference.catalog_id.clone(),
                actual_tier: reference.tier,
                context: reference.context,
                reference_basis: reference.basis,
            },
            SelectedPrice::AssumedRequest {
                reference,
                context_assumed,
                cache_write_assumed_zero,
            } => PriceMatchBasis::OfflineAssumedReference {
                catalog_id: reference.catalog_id.clone(),
                context: reference.context,
                context_assumed: *context_assumed,
                cache_write_assumed_zero: *cache_write_assumed_zero,
                reference_basis: reference.basis,
            },
        };
        Some(MatchedPrice {
            rule_id: rule.rule_id.clone(),
            model_exact: rule.model_exact.clone(),
            introduced_revision: rule.introduced_revision.clone(),
            basis,
        })
    }
}
