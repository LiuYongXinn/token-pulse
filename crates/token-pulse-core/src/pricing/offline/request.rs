//! Conditional selection is separate from the legacy flat Standard reference rules.
use super::{OfflineContextBand, OfflinePriceCatalog, OfflinePriceTier, OfflineReferenceBasis};
use crate::{
    domain::RequestUsageEvidence,
    numeric::{DecimalInt, EpochMs},
    pricing::{
        PriceOrigin, PriceRule, PricingEvent, rate_atoms,
        request::{RequestConsumptionBinding, RequestInputContext},
    },
};

/// Internal evidence only. Callers must supply a response-confirmed tier, never configuration.
/// Current rollout adapters do not supply actual_tier; this is not an IPC override.
pub struct RequestPriceEvidence<'a> {
    pub response: &'a RequestUsageEvidence,
    pub context: RequestInputContext<'a>,
    pub actual_tier: Option<OfflinePriceTier>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestPriceSelectionError {
    InvalidCatalog,
    UnknownModel,
    UnsupportedProvider,
    MissingInputEvidence,
    DifferentConsumption,
    MissingActualTier,
    MissingQuote,
}
/// A global API reference only: it does not prove a region, subscription bill or tool cost.
pub struct SelectedRequestReference {
    pub catalog_id: String,
    pub rule: PriceRule,
    pub tier: OfflinePriceTier,
    pub context: OfflineContextBand,
    pub basis: OfflineReferenceBasis,
}
impl OfflinePriceCatalog {
    /// One full request selects one band for all four rates, including its whole output.
    /// Historical evaluation uses the returned immutable rule with the existing PriceCatalog.
    pub fn select_request_reference(
        &self,
        event: &PricingEvent<'_>,
        evidence: Option<RequestPriceEvidence<'_>>,
        revision: DecimalInt,
        created_at: EpochMs,
    ) -> Result<SelectedRequestReference, RequestPriceSelectionError> {
        use RequestPriceSelectionError as E;
        self.validate().map_err(|_| E::InvalidCatalog)?;
        self.select_request_reference_validated(event, evidence, revision, created_at)
    }
    /// PriceCatalog owns a validated immutable clone, so it need not revalidate every event.
    pub(crate) fn select_request_reference_validated(
        &self,
        event: &PricingEvent<'_>,
        evidence: Option<RequestPriceEvidence<'_>>,
        revision: DecimalInt,
        created_at: EpochMs,
    ) -> Result<SelectedRequestReference, RequestPriceSelectionError> {
        use RequestPriceSelectionError as E;
        let (Some(provider), Some(model)) = (event.provider, event.model) else {
            return Err(E::UnknownModel);
        };
        if provider != self.provider {
            return Err(E::UnsupportedProvider);
        }
        // Canonical model resolution remains the existing alias layer's responsibility.
        if !self.entries.iter().any(|entry| entry.model_exact == model) {
            return Err(E::MissingQuote);
        }
        let evidence = evidence.ok_or(E::MissingInputEvidence)?;
        let input = evidence
            .response
            .project_input(evidence.context, event.usage)
            .ok_or(E::MissingInputEvidence)?;
        if input.binding != RequestConsumptionBinding::FullRequest {
            return Err(E::DifferentConsumption);
        }
        let tier = evidence.actual_tier.ok_or(E::MissingActualTier)?;
        let band = if input.input_tokens.value() <= i128::from(self.short_context_max_input) {
            OfflineContextBand::Short
        } else {
            OfflineContextBand::Long
        };
        let entry = self
            .entries
            .iter()
            .find(|entry| {
                entry.model_exact == model
                    && entry.tier == tier
                    && (entry.context == OfflineContextBand::All || entry.context == band)
            })
            .ok_or(E::MissingQuote)?;
        self.reference_for_entry(entry, revision, created_at)
    }
    /// Immutable quote identities for persistence, never unconditional matching rules.
    /// Actual request evidence must still pass select_request_reference before pricing.
    pub fn request_reference_rules(
        &self,
        revision: DecimalInt,
        created_at: EpochMs,
    ) -> Result<Vec<SelectedRequestReference>, RequestPriceSelectionError> {
        self.validate()
            .map_err(|_| RequestPriceSelectionError::InvalidCatalog)?;
        self.entries
            .iter()
            .map(|entry| self.reference_for_entry(entry, revision.clone(), created_at))
            .collect()
    }
    fn reference_for_entry(
        &self,
        entry: &super::OfflinePriceEntry,
        revision: DecimalInt,
        created_at: EpochMs,
    ) -> Result<SelectedRequestReference, RequestPriceSelectionError> {
        use RequestPriceSelectionError as E;
        let tier = entry.tier;
        let model = &entry.model_exact;
        let tier_key = match tier {
            OfflinePriceTier::Standard => "standard",
            OfflinePriceTier::Batch => "batch",
            OfflinePriceTier::Flex => "flex",
            OfflinePriceTier::Fast => "fast",
            OfflinePriceTier::Ultrafast => "ultrafast",
        };
        let band_key = match entry.context {
            OfflineContextBand::All => "all",
            OfflineContextBand::Short => "short",
            OfflineContextBand::Long => "long",
        };
        let parse = |value: &str| rate_atoms(value).map_err(|_| E::InvalidCatalog);
        let rule = PriceRule {
            rule_id: format!("offline/{}/{model}/{tier_key}/{band_key}", self.catalog_id),
            introduced_revision: revision,
            retired_revision: None,
            provider: self.provider.clone(),
            model_exact: model.clone(),
            source_id: None,
            currency: self.currency.clone(),
            // Verification is not evidence of rates before this instant.
            effective_from_ms: self.verified_at_ms,
            effective_to_ms: None,
            priority: 0,
            input_rate_atoms: parse(&entry.input_per_million)?,
            cached_rate_atoms: entry.cached_per_million.as_deref().map(parse).transpose()?,
            cache_write_rate_atoms: entry
                .cache_write_per_million
                .as_deref()
                .map(parse)
                .transpose()?,
            output_rate_atoms: parse(&entry.output_per_million)?,
            origin: PriceOrigin::Offline,
            origin_reference: Some(entry.reference.clone()),
            created_at_ms: created_at,
        };
        rule.validate().map_err(|_| E::InvalidCatalog)?;
        Ok(SelectedRequestReference {
            catalog_id: self.catalog_id.clone(),
            rule,
            tier,
            context: entry.context,
            basis: self.reference_basis,
        })
    }
}
