use chrono::DateTime;
use token_pulse_core::{
    calendar::{Grain, buckets},
    numeric::EpochMs,
    protocol::DateRange,
};

fn range(start: &str, end: &str, timezone: &str) -> DateRange {
    let ms = |s| EpochMs::new(DateTime::parse_from_rfc3339(s).unwrap().timestamp_millis()).unwrap();
    DateRange {
        start_ms: ms(start),
        end_ms: ms(end),
        timezone: timezone.into(),
    }
}
fn hours(start: &str, end: &str, timezone: &str) -> Vec<(String, String, i64)> {
    buckets(&range(start, end, timezone), Grain::Hour)
        .unwrap()
        .into_iter()
        .map(|b| {
            (
                b.display_label,
                b.utc_offset,
                b.end_ms.value() - b.start_ms.value(),
            )
        })
        .collect()
}

#[test]
fn new_york_spring_hour_and_calendar_day() {
    let r = range(
        "2024-03-10T00:00:00-05:00",
        "2024-03-11T00:00:00-04:00",
        "America/New_York",
    );
    let days = buckets(&r, Grain::Day).unwrap();
    assert_eq!(days.len(), 1);
    assert_eq!(
        days[0].end_ms.value() - days[0].start_ms.value(),
        23 * 3_600_000
    );
    let h = hours(
        "2024-03-10T00:00:00-05:00",
        "2024-03-10T05:00:00-04:00",
        "America/New_York",
    );
    assert_eq!(
        h,
        vec![
            ("2024-03-10 00:00".into(), "-05:00".into(), 3_600_000),
            ("2024-03-10 01:00".into(), "-05:00".into(), 3_600_000),
            ("2024-03-10 03:00".into(), "-04:00".into(), 3_600_000),
            ("2024-03-10 04:00".into(), "-04:00".into(), 3_600_000),
        ]
    );
}

#[test]
fn new_york_repeated_hours_have_distinct_utc_bounds() {
    let r = range(
        "2024-11-03T00:00:00-04:00",
        "2024-11-04T00:00:00-05:00",
        "America/New_York",
    );
    let d = buckets(&r, Grain::Day).unwrap();
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].end_ms.value() - d[0].start_ms.value(), 25 * 3_600_000);
    let h = hours(
        "2024-11-03T00:00:00-04:00",
        "2024-11-03T03:00:00-05:00",
        "America/New_York",
    );
    assert_eq!(
        h,
        vec![
            ("2024-11-03 00:00".into(), "-04:00".into(), 3_600_000),
            ("2024-11-03 01:00".into(), "-04:00".into(), 3_600_000),
            ("2024-11-03 01:00".into(), "-05:00".into(), 3_600_000),
            ("2024-11-03 02:00".into(), "-05:00".into(), 3_600_000),
        ]
    );
}

#[test]
fn half_hour_dst_changes_split_at_real_offset_transition() {
    let fall = hours(
        "2024-04-07T00:00:00+11:00",
        "2024-04-07T04:00:00+10:30",
        "Australia/Lord_Howe",
    );
    assert_eq!(
        fall,
        vec![
            ("2024-04-07 00:00".into(), "+11:00".into(), 3_600_000),
            ("2024-04-07 01:00".into(), "+11:00".into(), 3_600_000),
            ("2024-04-07 01:30".into(), "+10:30".into(), 1_800_000),
            ("2024-04-07 02:00".into(), "+10:30".into(), 3_600_000),
            ("2024-04-07 03:00".into(), "+10:30".into(), 3_600_000),
        ]
    );
    let spring = hours(
        "2024-10-06T00:00:00+10:30",
        "2024-10-06T04:00:00+11:00",
        "Australia/Lord_Howe",
    );
    assert_eq!(
        spring.iter().map(|b| (&*b.0, b.2)).collect::<Vec<_>>(),
        vec![
            ("2024-10-06 00:00", 3_600_000),
            ("2024-10-06 01:00", 3_600_000),
            ("2024-10-06 02:30", 1_800_000),
            ("2024-10-06 03:00", 3_600_000),
        ]
    );
}

#[test]
fn quarter_hour_offset_and_clipping_preserve_labels_and_exact_bounds() {
    let h = hours(
        "2024-02-29T01:12:30+05:45",
        "2024-02-29T02:00:00+05:45",
        "Asia/Kathmandu",
    );
    assert_eq!(
        h,
        vec![("2024-02-29 01:00".into(), "+05:45".into(), 2_850_000)]
    );
}

#[test]
fn historical_seconds_are_preserved_in_offset_labels() {
    let h = hours(
        "1899-12-31T23:50:39Z",
        "1900-01-01T00:50:39Z",
        "Europe/Paris",
    );
    assert_eq!(
        h,
        vec![("1900-01-01 00:00".into(), "+00:09:21".into(), 3_600_000)]
    );
}

#[test]
fn months_use_calendar_lengths_and_half_open_end() {
    let r = range(
        "2024-01-31T23:30:00+08:00",
        "2024-03-01T00:00:00+08:00",
        "Asia/Shanghai",
    );
    let b = buckets(&r, Grain::Month).unwrap();
    assert_eq!(
        b.iter()
            .map(|b| (&*b.display_label, b.end_ms.value() - b.start_ms.value()))
            .collect::<Vec<_>>(),
        vec![("2024-01", 1_800_000), ("2024-02", 29 * 86_400_000)]
    );
}

#[test]
fn skipped_calendar_date_does_not_create_empty_bucket() {
    let r = range(
        "2011-12-29T00:00:00-10:00",
        "2012-01-01T00:00:00+14:00",
        "Pacific/Apia",
    );
    let b = buckets(&r, Grain::Day).unwrap();
    assert_eq!(
        b.iter().map(|b| &*b.display_label).collect::<Vec<_>>(),
        vec!["2011-12-29", "2011-12-31"]
    );
    assert!(
        b.iter()
            .all(|b| b.end_ms.value() - b.start_ms.value() == 86_400_000)
    );
}

#[test]
fn bucket_limits_and_unsupported_calendar_instants_are_rejected() {
    let r = DateRange {
        start_ms: EpochMs::new(0).unwrap(),
        end_ms: EpochMs::new(2000 * 3_600_000).unwrap(),
        timezone: "UTC".into(),
    };
    assert_eq!(buckets(&r, Grain::Hour).unwrap().len(), 2000);
    let long = DateRange {
        end_ms: EpochMs::new(2000 * 3_600_000 + 1).unwrap(),
        ..r.clone()
    };
    assert!(buckets(&long, Grain::Hour).is_err());
    let invalid = DateRange {
        timezone: "not/a/timezone".into(),
        ..r.clone()
    };
    assert!(buckets(&invalid, Grain::Day).is_err());
    let unsupported = DateRange {
        start_ms: EpochMs::new(8_639_999_999_999_000).unwrap(),
        end_ms: EpochMs::new(8_640_000_000_000_000).unwrap(),
        timezone: "UTC".into(),
    };
    assert!(buckets(&unsupported, Grain::Month).is_err());
}

fn selection(
    timezone: &str,
    choice: token_pulse_core::calendar::CalendarSelection,
    at: &str,
) -> Result<token_pulse_core::calendar::CalendarSelectionResult, token_pulse_core::error::ErrorCode>
{
    token_pulse_core::calendar::resolve_selection(
        &token_pulse_core::calendar::CalendarSelectionRequest {
            timezone: timezone.into(),
            selection: choice,
        },
        EpochMs::new(DateTime::parse_from_rfc3339(at).unwrap().timestamp_millis()).unwrap(),
    )
}
fn custom(start: &str, end: &str) -> token_pulse_core::calendar::CalendarSelection {
    token_pulse_core::calendar::CalendarSelection::Custom {
        start_date: start.into(),
        end_date_inclusive: end.into(),
    }
}
#[test]
fn date_selections_use_requested_timezone_and_real_dst_day_lengths() {
    use token_pulse_core::calendar::CalendarSelection;
    let autumn = selection(
        "America/New_York",
        CalendarSelection::Today {},
        "2026-11-01T12:00:00Z",
    )
    .unwrap();
    let expected = range(
        "2026-11-01T04:00:00Z",
        "2026-11-02T05:00:00Z",
        "America/New_York",
    );
    assert_eq!(autumn.range.start_ms, expected.start_ms);
    assert_eq!(autumn.range.end_ms, expected.end_ms);
    assert_eq!(autumn.local_today, "2026-11-01");
    let seven = selection(
        "America/New_York",
        CalendarSelection::Last7 {},
        "2026-11-01T12:00:00Z",
    )
    .unwrap();
    assert_eq!(
        seven.range.start_ms,
        range(
            "2026-10-26T04:00:00Z",
            "2026-11-02T05:00:00Z",
            "America/New_York"
        )
        .start_ms
    );
    assert_eq!(seven.range.end_ms, autumn.range.end_ms);
    let spring = selection(
        "America/New_York",
        custom("2026-03-08", "2026-03-08"),
        "2026-10-02T12:00:00Z",
    )
    .unwrap();
    assert_eq!(
        spring.range.end_ms.value() - spring.range.start_ms.value(),
        23 * 3_600_000
    );
    assert_eq!(
        buckets(&spring.heatmap_range, Grain::Day).unwrap().len(),
        182
    );
}
#[test]
fn inclusive_date_selection_and_current_date_do_not_depend_on_machine_timezone() {
    use token_pulse_core::calendar::CalendarSelection;
    let utc = selection("UTC", CalendarSelection::Today {}, "2026-10-02T00:30:00Z").unwrap();
    let hawaii = selection(
        "Pacific/Honolulu",
        CalendarSelection::Today {},
        "2026-10-02T00:30:00Z",
    )
    .unwrap();
    assert_eq!(utc.local_today, "2026-10-02");
    assert_eq!(hawaii.local_today, "2026-10-01");
    let leap = selection(
        "UTC",
        custom("2024-02-28", "2024-02-29"),
        "2026-10-02T00:30:00Z",
    )
    .unwrap();
    let expected = range("2024-02-28T00:00:00Z", "2024-03-01T00:00:00Z", "UTC");
    assert_eq!(leap.range.start_ms, expected.start_ms);
    assert_eq!(leap.range.end_ms, expected.end_ms);
    assert_ne!(leap.heatmap_range.start_ms, leap.range.start_ms);
}
#[test]
fn midnight_gaps_are_resolved_but_fully_skipped_dates_never_manufacture_a_day() {
    let gap = selection(
        "America/Sao_Paulo",
        custom("2018-11-04", "2018-11-04"),
        "2026-10-02T12:00:00Z",
    )
    .unwrap();
    let expected = range(
        "2018-11-04T03:00:00Z",
        "2018-11-05T02:00:00Z",
        "America/Sao_Paulo",
    );
    assert_eq!(gap.range.start_ms, expected.start_ms);
    assert_eq!(gap.range.end_ms, expected.end_ms);
    assert_eq!(
        selection(
            "Pacific/Apia",
            custom("2011-12-30", "2011-12-30"),
            "2026-10-02T12:00:00Z"
        )
        .unwrap_err(),
        token_pulse_core::error::ErrorCode::InvalidQuery
    );
}
#[test]
fn date_selections_reject_invalid_dates_order_zones_and_unknown_fields() {
    for (zone, start, end) in [
        ("UTC", "2026-02-29", "2026-03-01"),
        ("UTC", "2026-2-04", "2026-02-05"),
        ("UTC", "2026-10-03", "2026-10-02"),
        ("Unknown/Zone", "2026-10-01", "2026-10-02"),
        ("UTC", "99999-01-01", "99999-01-02"),
    ] {
        assert_eq!(
            selection(zone, custom(start, end), "2026-10-02T12:00:00Z").unwrap_err(),
            token_pulse_core::error::ErrorCode::InvalidQuery
        );
    }
    assert!(
        serde_json::from_value::<token_pulse_core::calendar::CalendarSelection>(
            serde_json::json!({"kind":"today","days":1})
        )
        .is_err()
    );
}
