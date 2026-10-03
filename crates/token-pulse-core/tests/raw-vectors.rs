use token_pulse_core::{
    domain::UsageVector,
    query::{RawTokenCount, RawUsageVector},
};

#[test]
fn raw_counters_preserve_signed_i64_and_null_as_exact_strings_only() {
    for value in [i64::MIN, -1, 0, 9007199254740993, i64::MAX] {
        let encoded = format!("\"{value}\"");
        let raw: RawTokenCount = serde_json::from_str(&encoded).unwrap();
        assert_eq!(raw.value(), value);
        assert_eq!(serde_json::to_string(&raw).unwrap(), encoded);
    }
    for invalid in [
        "0",
        "\"01\"",
        "\"-0\"",
        "\"1e3\"",
        "\"9223372036854775808\"",
        "\"-9223372036854775809\"",
    ] {
        assert!(serde_json::from_str::<RawTokenCount>(invalid).is_err());
    }
    let dto = RawUsageVector::from(UsageVector {
        input_total: Some(-1),
        cache_write_input: None,
        cached_input: None,
        output_total: Some(9007199254740993),
        reasoning_output: None,
        reported_total: None,
    });
    let json = serde_json::to_value(dto).unwrap();
    assert_eq!(json["input_total"], "-1");
    assert_eq!(json["output_total"], "9007199254740993");
    assert!(json["cached_input"].is_null() && json["reported_total"].is_null());
}
