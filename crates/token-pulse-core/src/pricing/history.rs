//! Ordered, revision-bound catalog intervals; a removed quote never falls back to old facts.
use super::{PriceOrigin, PriceRule, offline::*};
use crate::{
    error::ErrorCode,
    numeric::{DecimalInt, EpochMs},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(super) struct OfflineReferences {
    versions: Vec<ReferenceVersion>,
}
pub(super) struct ReferenceVersion {
    pub publication: OfflineReferencePublication,
    models: BTreeSet<String>,
    flat_models: BTreeSet<String>,
    flat_ids: BTreeMap<String, String>,
}
impl ReferenceVersion {
    pub fn has_model(&self, model: &str) -> bool {
        self.models.contains(model)
    }
    pub fn has_conditional_model(&self, model: &str) -> bool {
        self.has_model(model) && !self.flat_models.contains(model)
    }
    pub fn owns_flat_rule(&self, rule: &PriceRule, canonical: &str) -> bool {
        rule.origin == PriceOrigin::Offline
            && rule.priority == 0
            && self.flat_ids.get(canonical) == Some(&rule.rule_id)
    }
}
impl OfflineReferences {
    pub fn new(
        publications: Vec<OfflineReferencePublication>,
        revision: &DecimalInt,
    ) -> Result<Self, ErrorCode> {
        let mut ids = BTreeSet::new();
        for (index, publication) in publications.iter().enumerate() {
            publication.catalog.validate()?;
            if publication.introduced_revision.value() > revision.value()
                || publication.installed_at_ms < publication.catalog.verified_at_ms
                || !ids.insert(publication.catalog.catalog_id.clone())
                || index > 0
                    && (publications[index - 1].introduced_revision.value()
                        >= publication.introduced_revision.value()
                        || publications[index - 1].catalog.verified_at_ms
                            > publication.catalog.verified_at_ms)
            {
                return Err(ErrorCode::InvalidQuery);
            }
        }
        let mut versions = Vec::new();
        for (index, publication) in publications.iter().enumerate() {
            let catalog = &publication.catalog;
            let rules = catalog.flat_standard_rules(
                publication.introduced_revision.clone(),
                publication.installed_at_ms,
            )?;
            let mut flat_ids = BTreeMap::new();
            for rule in rules {
                let id = if let Some(next) = publications.get(index + 1) {
                    let next_revision = i64::try_from(next.introduced_revision.value())
                        .map_err(|_| ErrorCode::InvalidQuery)?;
                    historical_flat_rule_id(next_revision, &rule.rule_id)?
                } else {
                    rule.rule_id
                };
                flat_ids.insert(rule.model_exact, id);
            }
            versions.push(ReferenceVersion {
                publication: publication.clone(),
                models: catalog
                    .entries
                    .iter()
                    .map(|entry| entry.model_exact.clone())
                    .collect(),
                flat_models: flat_ids.keys().cloned().collect(),
                flat_ids,
            });
        }
        Ok(Self { versions })
    }
    pub fn at(&self, provider: &str, time: EpochMs) -> Option<&ReferenceVersion> {
        // Equal verification instants are resolved by the later immutable publication.
        self.versions.iter().rev().find(|version| {
            let catalog = &version.publication.catalog;
            catalog.provider == provider && catalog.verified_at_ms <= time
        })
    }
}
