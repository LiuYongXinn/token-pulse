use token_pulse_core::{
    mini::*,
    numeric::EpochMs,
    protocol::{MiniScope, ScopeStart},
};
fn instant(value: &str) -> EpochMs {
    EpochMs::new(
        chrono::DateTime::parse_from_rfc3339(value)
            .unwrap()
            .timestamp_millis(),
    )
    .unwrap()
}
#[test]
fn today_scope_uses_configured_dst_midnight_and_advances_next_day_without_changing_fixed_start() {
    for time in ["2026-11-01T05:30:00Z", "2026-11-01T06:30:00Z"] {
        let at = instant(time);
        let f = usage_filter(&MiniScope::TodayAllSources {}, "America/New_York", at).unwrap();
        assert_eq!(f.range.start_ms, instant("2026-11-01T04:00:00Z"));
        assert_eq!(f.range.end_ms.value(), at.value() + 1);
    }
    let at = instant("2026-11-02T05:00:00Z");
    assert_eq!(
        usage_filter(&MiniScope::TodayAllSources {}, "America/New_York", at)
            .unwrap()
            .range
            .start_ms,
        at
    );
    let fixed = MiniScope::Session {
        session_key: "stable-session".into(),
        start: ScopeStart::Fixed {
            start_ms: instant("2026-11-01T04:00:00Z"),
        },
    };
    assert_eq!(
        usage_filter(&fixed, "America/New_York", at)
            .unwrap()
            .range
            .start_ms,
        instant("2026-11-01T04:00:00Z")
    );
}
#[test]
fn invalid_fixed_start_and_unknown_timezone_are_rejected_without_fallback() {
    let at = EpochMs::new(0).unwrap();
    let future = MiniScope::Session {
        session_key: "session".into(),
        start: ScopeStart::Fixed {
            start_ms: EpochMs::new(1).unwrap(),
        },
    };
    assert!(usage_filter(&future, "UTC", at).is_err());
    assert!(usage_filter(&MiniScope::TodayAllSources {}, "Invalid/Zone", at).is_err());
    let now = MiniScope::Session {
        session_key: "session".into(),
        start: ScopeStart::Fixed { start_ms: at },
    };
    let f = usage_filter(&now, "UTC", at).unwrap();
    assert_eq!(f.range.start_ms, at);
    assert_eq!(f.range.end_ms.value(), 1);
}
