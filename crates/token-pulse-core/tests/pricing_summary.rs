//! Independent, explicit synthetic expected subtotals; no market-price fixtures.
use token_pulse_core::{
    error::ErrorCode,
    numeric::{DecimalInt, DecimalMoney},
    pricing::{MAX_RATE_ATOMS, PriceOutcome, PricingAccumulator, UnpricedCode},
    protocol::PriceBasis,
};
fn priced(currency: &str, atoms: i128) -> PriceOutcome {
    PriceOutcome::Priced {
        rule_id: "synthetic".into(),
        currency: currency.into(),
        cost_atoms: DecimalInt::from_nonnegative(atoms).unwrap(),
        estimated_cost: DecimalMoney::from_atoms(atoms).unwrap(),
    }
}
#[test]
fn currency_and_reason_subtotals_remain_exact_independent_and_redactable() {
    let mut sums = PricingAccumulator::new(PriceBasis::EventTime {});
    assert!(sums.summary(false).unwrap().currencies.is_empty());
    sums.push(9_007_199_254_740_993, priced("USD", 1)).unwrap();
    sums.push(3, priced("USD", 2)).unwrap();
    sums.push(7, priced("EUR", 0)).unwrap();
    sums.push(
        11,
        PriceOutcome::Unpriced {
            reason: UnpricedCode::MissingRule,
        },
    )
    .unwrap();
    sums.push(
        13,
        PriceOutcome::Unpriced {
            reason: UnpricedCode::MissingRule,
        },
    )
    .unwrap();
    sums.push(
        17,
        PriceOutcome::Unpriced {
            reason: UnpricedCode::UnknownModel,
        },
    )
    .unwrap();
    let s = sums.summary(false).unwrap();
    assert_eq!(s.priced_total_tokens.as_str(), "9007199254741003");
    assert_eq!(s.unpriced_total_tokens.as_str(), "41");
    assert_eq!(s.currencies.len(), 2);
    assert_eq!(s.currencies[0].currency, "EUR");
    assert_eq!(
        s.currencies[0].estimated_cost.as_ref().unwrap().as_str(),
        "0.000000000000000"
    );
    assert_eq!(s.currencies[0].priced_total_tokens.as_str(), "7");
    assert_eq!(
        s.currencies[1].estimated_cost.as_ref().unwrap().as_str(),
        "0.000000000000003"
    );
    assert_eq!(
        s.currencies[1].priced_total_tokens.as_str(),
        "9007199254740996"
    );
    assert_eq!(s.reasons[0].code, "missing_rule");
    assert_eq!(s.reasons[0].total_tokens.as_str(), "24");
    assert_eq!(s.reasons[0].event_count.as_str(), "2");
    let private = sums.summary(true).unwrap();
    assert!(
        private.redacted
            && private
                .currencies
                .iter()
                .all(|c| c.estimated_cost.is_none())
    );
    assert_eq!(private.priced_total_tokens, s.priced_total_tokens);
    assert!(!s.redacted && !s.calculating);
}
#[test]
fn wholly_unpriced_is_not_a_zero_currency_estimate() {
    let mut sums = PricingAccumulator::new(PriceBasis::EventTime {});
    for reason in [
        UnpricedCode::UnknownModel,
        UnpricedCode::MissingRule,
        UnpricedCode::AmbiguousRule,
        UnpricedCode::InsufficientUsage,
        UnpricedCode::Overflow,
    ] {
        sums.push(19, PriceOutcome::Unpriced { reason }).unwrap();
    }
    let s = sums.summary(false).unwrap();
    assert!(s.currencies.is_empty());
    assert_eq!(s.unpriced_total_tokens.as_str(), "95");
    assert_eq!(s.priced_total_tokens.as_str(), "0");
    assert_eq!(s.reasons.len(), 5);
    for reason in s.reasons {
        assert_eq!(reason.event_count.as_str(), "1");
        assert_eq!(reason.total_tokens.as_str(), "19");
    }
}
#[test]
fn genuine_aggregate_overflow_is_explicit_and_the_failed_push_does_not_change_coverage() {
    let per_event = i128::from(i64::MAX) * MAX_RATE_ATOMS;
    let valid_count = i128::MAX / per_event;
    let mut sums = PricingAccumulator::new(PriceBasis::EventTime {});
    for _ in 0..valid_count {
        sums.push(i64::MAX, priced("USD", per_event)).unwrap();
    }
    let before = serde_json::to_value(sums.summary(false).unwrap()).unwrap();
    assert_eq!(
        sums.push(i64::MAX, priced("USD", per_event)).unwrap_err(),
        ErrorCode::NumericOverflow
    );
    assert_eq!(
        serde_json::to_value(sums.summary(false).unwrap()).unwrap(),
        before
    );
    assert_eq!(
        sums.push(
            -1,
            PriceOutcome::Unpriced {
                reason: UnpricedCode::MissingRule
            }
        )
        .unwrap_err(),
        ErrorCode::InvalidUsage
    );
    assert_eq!(
        serde_json::to_value(sums.summary(false).unwrap()).unwrap(),
        before
    );
}
