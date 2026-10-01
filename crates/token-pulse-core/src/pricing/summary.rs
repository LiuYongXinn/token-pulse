//! Streaming currency subtotals; round only the final display, never each event.
use super::PriceOutcome;
use crate::{
    error::ErrorCode,
    numeric::{DecimalInt, DecimalMoney},
    protocol::{CurrencyEstimate, PriceBasis, PricingSummary, UnpricedReason},
};
use std::collections::BTreeMap;

#[derive(Default, Clone, Copy)]
struct CurrencySum {
    atoms: i128,
    tokens: i128,
}
#[derive(Default, Clone, Copy)]
struct ReasonSum {
    tokens: i128,
    events: i128,
}
pub struct PricingAccumulator {
    basis: PriceBasis,
    currencies: BTreeMap<String, CurrencySum>,
    reasons: BTreeMap<&'static str, ReasonSum>,
    priced: i128,
    unpriced: i128,
}
fn add(a: i128, b: i128) -> Result<i128, ErrorCode> {
    a.checked_add(b).ok_or(ErrorCode::NumericOverflow)
}
impl PricingAccumulator {
    pub fn new(basis: PriceBasis) -> Self {
        Self {
            basis,
            currencies: BTreeMap::new(),
            reasons: BTreeMap::new(),
            priced: 0,
            unpriced: 0,
        }
    }
    /// Caller supplies the confirmed event's total, never pending / inherited.
    /// A failed push leaves this accumulator unchanged.
    pub fn push(&mut self, total: i64, outcome: PriceOutcome) -> Result<(), ErrorCode> {
        if total < 0 {
            return Err(ErrorCode::InvalidUsage);
        }
        let tokens = i128::from(total);
        match outcome {
            PriceOutcome::Redacted {} => return Err(ErrorCode::InvalidQuery),
            PriceOutcome::Priced {
                currency,
                cost_atoms,
                ..
            } => {
                if currency.len() != 3 || !currency.bytes().all(|b| b.is_ascii_uppercase()) {
                    return Err(ErrorCode::InvalidQuery);
                }
                let previous = self.currencies.get(&currency).copied().unwrap_or_default();
                let next = CurrencySum {
                    atoms: add(previous.atoms, cost_atoms.value())?,
                    tokens: add(previous.tokens, tokens)?,
                };
                let priced = add(self.priced, tokens)?;
                add(priced, self.unpriced)?;
                self.currencies.insert(currency, next);
                self.priced = priced;
            }
            PriceOutcome::Unpriced { reason } => {
                let key = reason.as_str();
                let previous = self.reasons.get(key).copied().unwrap_or_default();
                let next = ReasonSum {
                    tokens: add(previous.tokens, tokens)?,
                    events: add(previous.events, 1)?,
                };
                let unpriced = add(self.unpriced, tokens)?;
                add(self.priced, unpriced)?;
                self.reasons.insert(key, next);
                self.unpriced = unpriced;
            }
        }
        Ok(())
    }
    pub fn summary(&self, redacted: bool) -> Result<PricingSummary, ErrorCode> {
        Ok(PricingSummary {
            redacted,
            basis: self.basis.clone(),
            currencies: self
                .currencies
                .iter()
                .map(|(currency, sum)| {
                    Ok(CurrencyEstimate {
                        currency: currency.clone(),
                        estimated_cost: if redacted {
                            None
                        } else {
                            Some(DecimalMoney::from_atoms(sum.atoms)?)
                        },
                        priced_total_tokens: DecimalInt::from_nonnegative(sum.tokens)?,
                    })
                })
                .collect::<Result<_, ErrorCode>>()?,
            priced_total_tokens: DecimalInt::from_nonnegative(self.priced)?,
            unpriced_total_tokens: DecimalInt::from_nonnegative(self.unpriced)?,
            reasons: self
                .reasons
                .iter()
                .map(|(code, sum)| {
                    Ok(UnpricedReason {
                        code: (*code).into(),
                        total_tokens: DecimalInt::from_nonnegative(sum.tokens)?,
                        event_count: DecimalInt::from_nonnegative(sum.events)?,
                    })
                })
                .collect::<Result<_, ErrorCode>>()?,
            calculating: false,
        })
    }
}
