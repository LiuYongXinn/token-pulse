//! Immutable price revisions and checked fixed-point estimates, with no IO.
use crate::{
    domain::UsageVector,
    error::ErrorCode,
    numeric::{DecimalInt, DecimalMoney, EpochMs},
    protocol::PriceBasis,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use ts_rs::TS;

/// Confirmed storage contract: at most 1,000,000 currency units per million.
pub const MAX_RATE_ATOMS: i128 = 1_000_000_000_000_000;

mod summary;
pub use summary::PricingAccumulator;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum PriceOrigin {
    Custom,
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct PriceRule {
    pub rule_id: String,
    pub introduced_revision: DecimalInt,
    pub retired_revision: Option<DecimalInt>,
    pub provider: String,
    pub model_exact: String,
    pub source_id: Option<String>,
    pub currency: String,
    pub effective_from_ms: EpochMs,
    pub effective_to_ms: Option<EpochMs>,
    #[schemars(range(min = 0, max = 10000))]
    pub priority: i32,
    pub input_rate_atoms: DecimalInt,
    pub cached_rate_atoms: Option<DecimalInt>,
    pub output_rate_atoms: DecimalInt,
    pub origin: PriceOrigin,
    pub origin_reference: Option<String>,
    pub created_at_ms: EpochMs,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct ModelAlias {
    pub alias_id: String,
    pub provider: String,
    pub alias: String,
    pub canonical_model: String,
    pub introduced_revision: DecimalInt,
    pub retired_revision: Option<DecimalInt>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct PriceRuleDraft {
    pub provider: String,
    pub model_exact: String,
    pub source_id: Option<String>,
    pub currency: String,
    pub effective_from_ms: EpochMs,
    pub effective_to_ms: Option<EpochMs>,
    #[schemars(range(min = 0, max = 10000))]
    pub priority: i32,
    pub input_rate_atoms: DecimalInt,
    pub cached_rate_atoms: Option<DecimalInt>,
    pub output_rate_atoms: DecimalInt,
    pub origin_reference: Option<String>,
}
impl PriceRuleDraft {
    pub fn into_rule(self, id: String, revision: DecimalInt, at: EpochMs) -> PriceRule {
        PriceRule {
            rule_id: id,
            introduced_revision: revision,
            retired_revision: None,
            provider: self.provider,
            model_exact: self.model_exact,
            source_id: self.source_id,
            currency: self.currency,
            effective_from_ms: self.effective_from_ms,
            effective_to_ms: self.effective_to_ms,
            priority: self.priority,
            input_rate_atoms: self.input_rate_atoms,
            cached_rate_atoms: self.cached_rate_atoms,
            output_rate_atoms: self.output_rate_atoms,
            origin: PriceOrigin::Custom,
            origin_reference: self.origin_reference,
            created_at_ms: at,
        }
    }
    pub fn validate(&self) -> Result<(), ErrorCode> {
        self.clone()
            .into_rule(
                "validation".into(),
                DecimalInt::from_nonnegative(0)?,
                EpochMs::new(0)?,
            )
            .validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PriceRuleMutation {
    Create {
        draft: PriceRuleDraft,
    },
    Replace {
        rule_id: String,
        draft: PriceRuleDraft,
    },
    Retire {
        rule_id: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct PriceRulesSnapshot {
    pub price_revision: DecimalInt,
    pub rules: Vec<PriceRule>,
    pub aliases: Vec<ModelAlias>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct PriceChanged {
    pub price_revision: DecimalInt,
    pub all_models: bool,
}

fn key(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
fn valid_revisions(introduced: &DecimalInt, retired: &Option<DecimalInt>) -> bool {
    retired
        .as_ref()
        .is_none_or(|r| r.value() > introduced.value())
}
fn active(introduced: &DecimalInt, retired: &Option<DecimalInt>, revision: &DecimalInt) -> bool {
    introduced.value() <= revision.value()
        && retired
            .as_ref()
            .is_none_or(|r| r.value() > revision.value())
}
impl PriceRule {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if ![&self.rule_id, &self.provider, &self.model_exact]
            .iter()
            .all(|s| key(s))
            || self.source_id.as_ref().is_some_and(|s| !key(s))
            || self.currency.len() != 3
            || !self.currency.bytes().all(|c| c.is_ascii_uppercase())
            || !valid_revisions(&self.introduced_revision, &self.retired_revision)
            || self
                .effective_to_ms
                .is_some_and(|end| end <= self.effective_from_ms)
            || !(0..=10_000).contains(&self.priority)
            || [
                Some(&self.input_rate_atoms),
                self.cached_rate_atoms.as_ref(),
                Some(&self.output_rate_atoms),
            ]
            .into_iter()
            .flatten()
            .any(|rate| rate.value() > MAX_RATE_ATOMS)
            || self.origin == PriceOrigin::Offline && self.source_id.is_some()
            || self
                .origin_reference
                .as_ref()
                .is_some_and(|s| s.is_empty() || s.len() > 2048 || s.chars().any(char::is_control))
        {
            return Err(ErrorCode::InvalidQuery);
        }
        Ok(())
    }
    fn rank(&self) -> (u8, i32) {
        (
            match (&self.origin, &self.source_id) {
                (PriceOrigin::Custom, Some(_)) => 3,
                (PriceOrigin::Custom, None) => 2,
                (PriceOrigin::Offline, _) => 1,
            },
            self.priority,
        )
    }
}
impl ModelAlias {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if ![
            &self.alias_id,
            &self.provider,
            &self.alias,
            &self.canonical_model,
        ]
        .iter()
        .all(|s| key(s))
            || !valid_revisions(&self.introduced_revision, &self.retired_revision)
        {
            return Err(ErrorCode::InvalidQuery);
        }
        Ok(())
    }
}

/// Parse a user-entered price per million with at most nine decimal places.
/// The result is atoms, not a floating-point currency value.
pub fn rate_atoms(price: &str) -> Result<DecimalInt, ErrorCode> {
    let (whole, fraction) = price.split_once('.').unwrap_or((price, ""));
    let whole = DecimalInt::parse(whole)?.value();
    if whole > 1_000_000 {
        return Err(ErrorCode::InvalidQuery);
    }
    if price.ends_with('.') || fraction.len() > 9 || !fraction.bytes().all(|c| c.is_ascii_digit()) {
        return Err(ErrorCode::InvalidQuery);
    }
    let fraction = format!("{fraction:0<9}")
        .parse::<i128>()
        .map_err(|_| ErrorCode::InvalidQuery)?;
    let atoms = whole
        .checked_mul(1_000_000_000)
        .and_then(|v| v.checked_add(fraction))
        .ok_or(ErrorCode::NumericOverflow)?;
    if atoms > MAX_RATE_ATOMS {
        return Err(ErrorCode::InvalidQuery);
    }
    DecimalInt::from_nonnegative(atoms)
}
pub fn rate_per_million(atoms: &DecimalInt) -> String {
    let value = atoms.value();
    let fraction = format!("{:09}", value % 1_000_000_000);
    let fraction = fraction.trim_end_matches('0');
    if fraction.is_empty() {
        (value / 1_000_000_000).to_string()
    } else {
        format!("{}.{fraction}", value / 1_000_000_000)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum UnpricedCode {
    UnknownModel,
    MissingRule,
    AmbiguousRule,
    InsufficientUsage,
    Overflow,
}
impl UnpricedCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnknownModel => "unknown_model",
            Self::MissingRule => "missing_rule",
            Self::AmbiguousRule => "ambiguous_rule",
            Self::InsufficientUsage => "insufficient_usage",
            Self::Overflow => "overflow",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum PriceOutcome {
    /// Transport-only display state; the price engine never creates this outcome.
    Redacted {},
    Priced {
        rule_id: String,
        currency: String,
        cost_atoms: DecimalInt,
        estimated_cost: DecimalMoney,
    },
    Unpriced {
        reason: UnpricedCode,
    },
}
fn unpriced(reason: UnpricedCode) -> PriceOutcome {
    PriceOutcome::Unpriced { reason }
}

pub struct PricingEvent<'a> {
    pub provider: Option<&'a str>,
    pub model: Option<&'a str>,
    /// All actual evidence sources; UI filtering does not change rule identity.
    pub source_ids: &'a [String],
    pub occurred_at_ms: EpochMs,
    pub usage: UsageVector,
}

type Models<T> = BTreeMap<String, BTreeMap<String, T>>;
pub struct PriceCatalog {
    rules: Vec<PriceRule>,
    index: Models<Vec<usize>>,
    aliases: Models<BTreeSet<String>>,
    pub revision: DecimalInt,
}
impl PriceCatalog {
    pub fn new(
        rules: Vec<PriceRule>,
        aliases: Vec<ModelAlias>,
        revision: DecimalInt,
    ) -> Result<Self, ErrorCode> {
        if rules.len() > 4096 || aliases.len() > 4096 {
            return Err(ErrorCode::InvalidQuery);
        }
        let mut rule_ids = BTreeSet::new();
        let mut alias_ids = BTreeSet::new();
        let mut result = Self {
            rules: Vec::new(),
            index: BTreeMap::new(),
            aliases: BTreeMap::new(),
            revision,
        };
        for rule in rules {
            rule.validate()?;
            if !rule_ids.insert(rule.rule_id.clone()) {
                return Err(ErrorCode::InvalidQuery);
            }
            if active(
                &rule.introduced_revision,
                &rule.retired_revision,
                &result.revision,
            ) {
                result
                    .index
                    .entry(rule.provider.clone())
                    .or_default()
                    .entry(rule.model_exact.clone())
                    .or_default()
                    .push(result.rules.len());
                result.rules.push(rule);
            }
        }
        for alias in aliases {
            alias.validate()?;
            if !alias_ids.insert(alias.alias_id.clone()) {
                return Err(ErrorCode::InvalidQuery);
            }
            if active(
                &alias.introduced_revision,
                &alias.retired_revision,
                &result.revision,
            ) {
                result
                    .aliases
                    .entry(alias.provider)
                    .or_default()
                    .entry(alias.alias)
                    .or_default()
                    .insert(alias.canonical_model);
            }
        }
        Ok(result)
    }
    pub fn estimate(&self, event: &PricingEvent<'_>, basis: &PriceBasis) -> PriceOutcome {
        let (Some(provider), Some(model)) = (event.provider, event.model) else {
            return unpriced(UnpricedCode::UnknownModel);
        };
        let canonical = match self.aliases.get(provider).and_then(|m| m.get(model)) {
            Some(names) if names.len() != 1 => return unpriced(UnpricedCode::AmbiguousRule),
            Some(names) => names.first().expect("nonempty alias set").as_str(),
            None => model,
        };
        let time = match basis {
            PriceBasis::EventTime {} => event.occurred_at_ms,
            PriceBasis::SpecifiedTime { specified_at_ms } => *specified_at_ms,
        };
        let mut winner: Option<&PriceRule> = None;
        let mut ambiguous = false;
        for index in self
            .index
            .get(provider)
            .and_then(|m| m.get(canonical))
            .into_iter()
            .flatten()
        {
            let rule = &self.rules[*index];
            if rule.effective_from_ms > time
                || rule.effective_to_ms.is_some_and(|end| time >= end)
                || rule
                    .source_id
                    .as_ref()
                    .is_some_and(|id| !event.source_ids.contains(id))
            {
                continue;
            }
            match winner {
                None => {
                    winner = Some(rule);
                    ambiguous = false;
                }
                Some(old) if rule.rank() > old.rank() => {
                    winner = Some(rule);
                    ambiguous = false;
                }
                Some(old) if rule.rank() == old.rank() => {
                    ambiguous = true;
                }
                _ => {}
            }
        }
        if ambiguous {
            return unpriced(UnpricedCode::AmbiguousRule);
        }
        let Some(rule) = winner else {
            return unpriced(UnpricedCode::MissingRule);
        };
        match estimate_atoms(event.usage, rule) {
            Ok(atoms) => match (
                DecimalInt::from_nonnegative(atoms),
                DecimalMoney::from_atoms(atoms),
            ) {
                (Ok(cost_atoms), Ok(estimated_cost)) => PriceOutcome::Priced {
                    rule_id: rule.rule_id.clone(),
                    currency: rule.currency.clone(),
                    cost_atoms,
                    estimated_cost,
                },
                _ => unpriced(UnpricedCode::Overflow),
            },
            Err(reason) => unpriced(reason),
        }
    }
}

fn estimate_atoms(usage: UsageVector, rule: &PriceRule) -> Result<i128, UnpricedCode> {
    usage
        .validated_total()
        .map_err(|e| {
            if e == ErrorCode::NumericOverflow {
                UnpricedCode::Overflow
            } else {
                UnpricedCode::InsufficientUsage
            }
        })?
        .ok_or(UnpricedCode::InsufficientUsage)?;
    let (Some(input), Some(output)) = (usage.input_total, usage.output_total) else {
        return Err(UnpricedCode::InsufficientUsage);
    };
    let multiply = |tokens: i64, rate: &DecimalInt| {
        i128::from(tokens)
            .checked_mul(rate.value())
            .ok_or(UnpricedCode::Overflow)
    };
    let input_cost = if input == 0 {
        0
    } else {
        match usage.cached_input {
            Some(0) => multiply(input, &rule.input_rate_atoms)?,
            Some(cached) => multiply(input - cached, &rule.input_rate_atoms)?
                .checked_add(multiply(
                    cached,
                    rule.cached_rate_atoms
                        .as_ref()
                        .ok_or(UnpricedCode::InsufficientUsage)?,
                )?)
                .ok_or(UnpricedCode::Overflow)?,
            None if rule.cached_rate_atoms.as_ref() == Some(&rule.input_rate_atoms) => {
                multiply(input, &rule.input_rate_atoms)?
            }
            _ => return Err(UnpricedCode::InsufficientUsage),
        }
    };
    input_cost
        .checked_add(multiply(output, &rule.output_rate_atoms)?)
        .ok_or(UnpricedCode::Overflow)
}
