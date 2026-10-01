use proptest::prelude::*;
use serde::Deserialize;
use token_pulse_core::{
    domain::{NormalizedObservation, UsageVector},
    error::ErrorCode,
    numeric::*,
    protocol::*,
};

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<VectorCase>,
}
#[derive(Deserialize)]
struct VectorCase {
    name: String,
    vector: UsageVector,
    expected_total: Option<i64>,
    expected_noncached: Option<i64>,
    expected_error: Option<ErrorCode>,
}

#[test]
fn independently_listed_vector_expectations() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("../../../fixtures/usage-vectors.json")).unwrap();
    for case in fixture.cases {
        if let Some(error) = case.expected_error {
            assert_eq!(case.vector.validated_total(), Err(error), "{}", case.name);
        } else {
            assert_eq!(
                case.vector.validated_total(),
                Ok(case.expected_total),
                "{}",
                case.name
            );
            assert_eq!(
                case.vector.noncached_input(),
                Ok(case.expected_noncached),
                "{}",
                case.name
            );
        }
    }
}
#[test]
fn lossless_decimal_and_unknown_roundtrip() {
    let value = DecimalInt::parse("9007199254740993").unwrap();
    assert_eq!(
        serde_json::to_string(&value).unwrap(),
        "\"9007199254740993\""
    );
    let dto = TokenMeasure {
        value: None,
        covered_total_tokens: DecimalInt::parse("0").unwrap(),
        complete: false,
    };
    let json = serde_json::to_string(&dto).unwrap();
    assert!(json.contains("\"value\":null"));
    assert!(
        serde_json::from_str::<TokenMeasure>(&json)
            .unwrap()
            .value
            .is_none()
    );
    for bad in [
        "-1",
        "+1",
        "01",
        "1e3",
        "1.0",
        "",
        " 1",
        "170141183460469231731687303715884105728",
    ] {
        assert!(DecimalInt::parse(bad).is_err(), "{bad}");
    }
    assert_eq!(sum_checked([i128::MAX, 1]), Err(ErrorCode::NumericOverflow));
    assert_eq!(
        DecimalMoney::from_atoms(1).unwrap().as_str(),
        "0.000000000000001"
    );
    for bad in ["-0.1", "1.0000000000000001", "1.", "NaN", "1.2.3", "01.2"] {
        assert!(DecimalMoney::parse(bad).is_err(), "{bad}");
    }
}
#[test]
fn invalid_query_is_rejected_without_downgrade() {
    assert!(
        serde_json::from_str::<Response<()>>(r#"{"api_version":2,"request_id":"r","data":null}"#)
            .is_err()
    );
    let mut range = DateRange {
        start_ms: EpochMs::new(1000).unwrap(),
        end_ms: EpochMs::new(2000).unwrap(),
        timezone: "Asia/Shanghai".into(),
    };
    assert!(range.validate().is_ok());
    range.timezone = "Made/Up".into();
    assert_eq!(range.validate(), Err(ErrorCode::InvalidQuery));
    range.timezone = "UTC".into();
    range.end_ms = range.start_ms;
    assert_eq!(range.validate(), Err(ErrorCode::InvalidQuery));
    assert!(EpochMs::new(i64::MAX).is_err());
    assert!(
        serde_json::from_str::<DimensionSelection>(r#"{"kind":"all","ids":["unexpected"]}"#)
            .is_err()
    );
    assert!(serde_json::from_str::<WindowAction>(r#""execute_shell""#).is_err());
    assert!(
        serde_json::from_str::<PriceBasis>(r#"{"mode":"event_time","specified_at_ms":1}"#).is_err()
    );
    assert!(
        serde_json::from_str::<MiniScope>(
            r#"{"kind":"today_all_sources","session_key":"unexpected"}"#
        )
        .is_err()
    );
    assert_eq!(
        DimensionSelection::Ids {
            ids: vec!["s".into(); 101],
            include_unknown: false
        }
        .validate(),
        Err(ErrorCode::InvalidQuery)
    );
}
#[test]
fn normalized_storage_cannot_accept_chat_content() {
    assert!(serde_json::from_str::<UsageVector>(r#"{"input_total":1,"content":"chat"}"#).is_err());
    assert!(serde_json::from_str::<NormalizedObservation>(r#"{"kind":"turn_metadata","physical_position":{"file_generation_id":"g","byte_offset":0,"byte_end":10},"session_key":"s","metadata":{"provider":null,"model":null,"cwd":null,"turn_id":null,"parent_provider_id":null},"messages":[{"content":"secret"}]}"#).is_err());
}
proptest! {
    #[test]
    fn decimal_int_roundtrips_all_nonnegative_i128(n in 0..=i128::MAX) {
        let value = DecimalInt::from_nonnegative(n).unwrap();
        let json = serde_json::to_string(&value).unwrap();
        prop_assert_eq!(serde_json::from_str::<DecimalInt>(&json).unwrap().value(), n);
    }
}
