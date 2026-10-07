//! Independent display expectations. Values are synthetic and never use developer logs/accounts.
use token_pulse_core::{
    numeric::{DecimalInt, EpochMs},
    protocol::{DateRange, QuotaState},
    settings::AppTheme,
};
use token_pulse_taskbar::{
    HostDetails, HostScope, HostSourceStatus, TaskbarView, WireError,
    details::{content, price_coverage},
};
fn number(text: &str) -> DecimalInt {
    DecimalInt::parse(text).unwrap()
}
fn fixture() -> TaskbarView {
    let mut view: TaskbarView =
        serde_json::from_str(include_str!("../../../fixtures/taskbar-display.json")).unwrap();
    view.details = Some(HostDetails {
        theme: AppTheme::Light,
        range: DateRange {
            start_ms: EpochMs::new(0).unwrap(),
            end_ms: EpochMs::new(1001).unwrap(),
            timezone: view.timezone.clone(),
        },
        scope: HostScope::FixedSession,
        source_last_success_at_ms: Some(EpochMs::new(500).unwrap()),
        source_statuses: vec![HostSourceStatus::Unreadable],
        pending_observations: number("9007199254740993"),
        pending_files: number("1"),
        breakdown_complete: false,
        input_complete: false,
        cached_complete: true,
        output_complete: false,
        pricing_calculating: true,
    });
    view
}
#[test]
fn price_coverage_has_independent_exact_rounding_and_i128_boundary_expectations() {
    for (priced, total, expected) in [
        ("0", "1", "0.00%"),
        ("1", "1", "100.00%"),
        ("1", "3", "33.33%"),
        ("2", "3", "66.67%"),
        ("1", "8", "12.50%"),
        ("1", "20000", "0.01%"),
        ("1", "20001", "0.00%"),
        (
            "170141183460469231731687303715884105726",
            "170141183460469231731687303715884105727",
            "100.00%",
        ),
        (
            "56713727820156410577229101238628035242",
            "170141183460469231731687303715884105727",
            "33.33%",
        ),
    ] {
        assert_eq!(
            price_coverage(&number(priced), Some(&number(total))).unwrap(),
            expected
        );
    }
    assert_eq!(
        price_coverage(&number("0"), Some(&number("0"))).unwrap(),
        "—（无用量）"
    );
    assert_eq!(price_coverage(&number("0"), None).unwrap(), "—（用量未知）");
    assert_eq!(
        price_coverage(&number("2"), Some(&number("1"))),
        Err(WireError::InvalidFrame)
    );
}
#[test]
fn precise_tokens_costs_scope_timezone_and_independent_timestamps_are_all_readable() {
    let mut view = fixture();
    view.total_tokens = Some(number("9007199254740993"));
    view.priced_tokens = number("9007199254740992");
    view.unpriced_tokens = number("1");
    let details = content(&view, view.generated_at_ms.value()).unwrap();
    let text = details.accessible_text();
    for expected in [
        "Token 用量：9007199254740993",
        "输入：—（未提供）",
        "缓存输入（包含在输入中）：0",
        "USD 0.565000000000001",
        "未计价 Token：1",
        "SYNTHETIC DEVELOPMENT FIXTURE",
        "1970-01-01 08:00:00.000",
        "1970-01-01 08:00:01.001",
        "Asia/Shanghai",
        "额度更新时间",
        "用量更新时间",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    let remaining: Vec<_> = details
        .rows
        .iter()
        .filter_map(|row| row.remaining_percent)
        .collect();
    assert_eq!(remaining, vec![72.0, 38.0]);
}
#[test]
fn visible_numbers_use_shared_ui_units_and_currency_rules_without_losing_exact_values() {
    use token_pulse_core::numeric::DecimalMoney;
    let mut view = fixture();
    view.total_tokens = Some(number("23849873"));
    view.input_tokens = Some(number("23733285"));
    view.cached_tokens = Some(number("22926848"));
    view.output_tokens = Some(number("1163588"));
    view.priced_tokens = number("23849873");
    view.unpriced_tokens = number("0");
    view.costs[0].estimated_cost = Some(DecimalMoney::parse("50.690338000000000").unwrap());
    let details = content(&view, view.generated_at_ms.value()).unwrap();
    for (label, expected) in [
        ("Token 用量", "23.8M Token"),
        ("输入", "23.7M Token"),
        ("缓存输入（包含在输入中）", "22.9M Token"),
        ("输出", "1.2M Token"),
        ("未计价 Token", "0 Token"),
        ("已计价部分估算", "$50.69"),
    ] {
        assert_eq!(
            details
                .rows
                .iter()
                .find(|row| row.label == label)
                .unwrap()
                .value,
            expected
        );
    }
    let exact = details.accessible_text();
    assert!(exact.contains("Token 用量：23849873"));
    assert!(exact.contains("USD 50.690338000000000"));
    for (count, expected) in [
        ("999", "999 Token"),
        ("1000", "1.0K Token"),
        ("1000500", "1.0M Token"),
        ("1000000000", "1.0B Token"),
    ] {
        view.total_tokens = Some(number(count));
        view.priced_tokens = number(count);
        let details = content(&view, view.generated_at_ms.value()).unwrap();
        assert_eq!(details.rows[0].value, expected);
    }
    view.costs[0].estimated_cost = Some(DecimalMoney::parse("0.005").unwrap());
    let mut other = view.costs[0].clone();
    other.currency = "CNY".into();
    other.estimated_cost = Some(DecimalMoney::parse("9.995").unwrap());
    view.costs.push(other);
    let details = content(&view, view.generated_at_ms.value()).unwrap();
    let amounts: Vec<_> = details
        .rows
        .iter()
        .filter(|row| row.label == "已计价部分估算")
        .map(|row| row.value.as_str())
        .collect();
    assert_eq!(amounts, ["USD 0.01", "CNY 10.00"]);
    view.costs[0].estimated_cost = None;
    view.total_tokens = None;
    let details = content(&view, view.generated_at_ms.value()).unwrap();
    assert_eq!(details.rows[0].value, "—");
    assert_eq!(
        details
            .rows
            .iter()
            .find(|row| row.label == "已计价部分估算")
            .unwrap()
            .value,
        "—"
    );
}
#[test]
fn privacy_removes_cost_account_bucket_and_original_scope_from_all_detail_text() {
    let mut view = fixture();
    view.privacy = true;
    view.scope_label = None;
    view.costs.clear();
    view.quota = None;
    let details = content(&view, view.generated_at_ms.value()).unwrap();
    let text = details.accessible_text();
    for private in [
        "SYNTHETIC DEVELOPMENT",
        "USD",
        "0.565",
        "已计价比例",
        "额度桶",
        "剩余 72%",
    ] {
        assert!(!text.contains(private), "leaked {private}");
    }
    assert!(text.contains("固定会话 · 指定起点"));
    assert!(text.contains("683100"));
    assert!(
        details
            .rows
            .iter()
            .all(|row| row.remaining_percent.is_none())
    );
    assert!(text.contains("费用、会话名称和账户额度已隐藏"));
}
#[test]
fn measurements_show_values_without_internal_completeness_labels() {
    let mut view = fixture();
    view.input_tokens = Some(number("4"));
    view.output_tokens = Some(number("7"));
    let text = content(&view, view.generated_at_ms.value())
        .unwrap()
        .accessible_text();
    assert!(text.contains("输入：4；"));
    assert!(text.contains("输出：7；"));
    assert!(text.contains("缓存输入（包含在输入中）：0；"));
    view.details = None;
    assert!(
        content(&view, 0)
            .unwrap()
            .accessible_text()
            .contains("输入：4；")
    );
}
#[test]
fn actual_account_periods_null_zero_duplicates_and_expired_reset_do_not_invent_values() {
    let mut view = fixture();
    let now = view.generated_at_ms.value();
    let quota = view.quota.as_mut().unwrap();
    quota.windows[0].duration_mins = Some(120);
    quota.windows[0].remaining_percent = None;
    quota.windows[1].remaining_percent = Some(0.0);
    quota.windows[1].resets_at_ms = Some(EpochMs::new(now).unwrap());
    quota.windows.push(token_pulse_core::protocol::QuotaWindow {
        window_id: "another-week".into(),
        duration_mins: Some(10080),
        used_percent: None,
        remaining_percent: Some(12.5),
        resets_at_ms: None,
    });
    let details = content(&view, now).unwrap();
    let text = details.accessible_text();
    assert!(text.contains("窗口 1 · 2 小时额度：剩余 —"));
    assert!(text.contains("窗口 2 · 周额度（7 天）：剩余 0%"));
    assert!(text.contains("窗口 3 · 周额度（7 天）：剩余 12.5%"));
    assert!(text.contains("重置时间已到，等待账户服务更新"));
    assert!(!text.contains("100%"));
    assert_eq!(
        details
            .rows
            .iter()
            .filter(|row| row.label.starts_with("窗口"))
            .map(|row| row.remaining_percent)
            .collect::<Vec<_>>(),
        vec![None, Some(0.0), Some(12.5)]
    );
}
#[test]
fn account_failure_preserves_old_values_and_attempt_time_but_disconnection_hides_them() {
    let mut view = fixture();
    let quota = view.quota.as_mut().unwrap();
    quota.state = QuotaState::Error;
    quota.error_code = Some(token_pulse_core::error::ErrorCode::QuotaTimeout);
    quota.fetched_at_ms = Some(EpochMs::new(500).unwrap());
    quota.last_attempt_at_ms = Some(EpochMs::new(1000).unwrap());
    let text = content(&view, view.generated_at_ms.value())
        .unwrap()
        .accessible_text();
    assert!(text.contains("更新失败，显示上次结果"));
    assert!(text.contains("剩余 72%"));
    assert!(text.contains("额度更新时间：1970-01-01 08:00:00.500"));
    view.quota.as_mut().unwrap().state = QuotaState::Disconnected;
    assert!(
        !content(&view, view.generated_at_ms.value())
            .unwrap()
            .accessible_text()
            .contains("剩余 72%")
    );
}
#[test]
fn native_details_reject_cross_timezone_bad_ranges_and_duplicate_states() {
    let mut view = fixture();
    view.details.as_mut().unwrap().range.timezone = "UTC".into();
    assert_eq!(content(&view, 0), Err(WireError::InvalidFrame));
    view.details.as_mut().unwrap().range.timezone = view.timezone.clone();
    view.details.as_mut().unwrap().range.end_ms = EpochMs::new(0).unwrap();
    assert_eq!(content(&view, 0), Err(WireError::InvalidFrame));
    view.details.as_mut().unwrap().range.end_ms = EpochMs::new(1).unwrap();
    view.details
        .as_mut()
        .unwrap()
        .source_statuses
        .push(HostSourceStatus::Unreadable);
    assert_eq!(content(&view, 0), Err(WireError::InvalidFrame));
}
#[test]
fn extreme_timestamps_and_countdown_carry_are_bounded_and_null_details_stay_unknown() {
    let mut view = fixture();
    view.quota.as_mut().unwrap().windows[0].resets_at_ms =
        Some(EpochMs::new(view.generated_at_ms.value() + 3_599_999).unwrap());
    assert!(
        content(&view, view.generated_at_ms.value())
            .unwrap()
            .accessible_text()
            .contains("距重置 0 天 1 小时 0 分钟")
    );
    view.details = None;
    view.quota = None;
    assert!(
        content(&view, -8_640_000_000_000_000)
            .unwrap()
            .accessible_text()
            .contains("范围详情：尚未提供")
    );
    assert_eq!(content(&view, i64::MIN), Err(WireError::InvalidFrame));
}

#[test]
fn collection_diagnostics_do_not_appear_in_usage_details() {
    let mut view = fixture();
    for status in [
        HostSourceStatus::Scanning,
        HostSourceStatus::ScanPending,
        HostSourceStatus::ScanInterrupted,
        HostSourceStatus::ScanChanged,
        HostSourceStatus::ScanIncomplete,
        HostSourceStatus::Unknown,
    ] {
        view.details.as_mut().unwrap().source_statuses = vec![status];
        let text = content(&view, view.generated_at_ms.value())
            .unwrap()
            .accessible_text();
        assert!(text.contains("Token 用量：683100"));
        for internal in [
            "来源状态",
            "待核对",
            "待确认观察",
            "用量覆盖",
            "来源最近成功核对",
        ] {
            assert!(!text.contains(internal), "{text}");
        }
    }
}
