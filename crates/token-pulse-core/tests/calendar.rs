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
