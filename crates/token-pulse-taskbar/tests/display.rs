use token_pulse_core::{
    numeric::{DecimalInt, DecimalMoney},
    protocol::{CoverageState, QuotaState},
};
use token_pulse_taskbar::{TaskbarView, display::*};
fn fixture() -> TaskbarView {
    serde_json::from_str(include_str!("../../../fixtures/taskbar-display.json")).unwrap()
}
fn texts(rows: Vec<Vec<Span>>) -> Vec<Vec<String>> {
    rows.into_iter()
        .map(|r| r.into_iter().map(|s| s.text).collect())
        .collect()
}
#[test]
fn exact_numbers_and_half_up_money_do_not_cross_floating_point_or_overflow() {
    for (value, expected) in [
        ("0", "0"),
        ("999", "999"),
        ("999999", "1000.0K"),
        ("9007199254740993", "9007199.3B"),
        (
            "170141183460469231731687303715884105727",
            "170141183460469231731687303715.9B",
        ),
    ] {
        assert_eq!(
            compact_tokens(Some(&DecimalInt::parse(value).unwrap())),
            expected
        );
    }
    assert_eq!(compact_tokens(None), "—");
    for (value, expected) in [
        ("0.000000000000001", "0.00"),
        ("0.005", "0.01"),
        ("9007199254740993.995", "9007199254740994.00"),
        (
            "170141183460469231731687.303715884105727",
            "170141183460469231731687.30",
        ),
    ] {
        assert_eq!(money(Some(&DecimalMoney::parse(value).unwrap())), expected);
    }
    assert_eq!(money(None), "—");
    assert_eq!(percent(Some(0.0)), "0%");
    assert_eq!(percent(None), "—");
}
#[test]
fn actual_periods_estimate_and_timezone_match_independent_expected_text() {
    let view = fixture();
    assert_eq!(
        texts(
            rows(
                &view,
                DisplayPreferences::default(),
                Density::Full,
                1790899200000
            )
            .unwrap()
        ),
        vec![
            vec!["Token 683.1K", "$0.57"],
            vec!["5h 72%", "周 38%", "周重置 10/04 10:25"]
        ]
    );
    let compact = texts(
        rows(
            &view,
            DisplayPreferences::default(),
            Density::Compact,
            1790899200000,
        )
        .unwrap(),
    );
    assert_eq!(compact, vec![vec!["Token 683.1K", "$0.57"], vec!["周 38%"]]);
    let mut invalid = view.clone();
    invalid.timezone = "INVALID/TIMEZONE".into();
    assert_eq!(
        texts(rows(&invalid, DisplayPreferences::default(), Density::Full, 0).unwrap())[1][2],
        "周重置 时区无效"
    );
    assert_eq!(
        texts(
            rows(
                &view,
                DisplayPreferences::default(),
                Density::Full,
                1791080700000
            )
            .unwrap()
        )[1][2],
        "周重置待更新"
    );
}
#[test]
fn null_zero_stale_and_ambiguous_roles_remain_distinct() {
    let mut view = fixture();
    view.total_tokens = None;
    view.costs.clear();
    view.usage_status = CoverageState::Unknown;
    let quota = view.quota.as_mut().unwrap();
    quota.state = QuotaState::Stale;
    quota.windows[1].remaining_percent = Some(0.0);
    let output = rows(&view, DisplayPreferences::default(), Density::Full, 0).unwrap();
    assert_eq!(output[0][0].text, "Token —");
    assert_eq!(output[0][1].text, "—");
    assert_eq!(output[1][1].text, "周 0%");
    assert_eq!(output[1][1].tone, Tone::Warning);
    assert_eq!(output[1][2].text, "更新失败");
    let duplicate = view.quota.as_ref().unwrap().windows[1].clone();
    let mut duplicate = duplicate;
    duplicate.window_id = "another_actual_week".into();
    view.quota.as_mut().unwrap().windows.push(duplicate);
    assert_eq!(
        texts(rows(&view, DisplayPreferences::default(), Density::Full, 0).unwrap())[1][1],
        "周 —"
    );
    view.quota = None;
    assert_eq!(
        texts(rows(&view, DisplayPreferences::default(), Density::Full, 0).unwrap())[1][0],
        "额度未连接"
    );
    view.unpriced_tokens = DecimalInt::parse("3").unwrap();
    assert_eq!(
        texts(rows(&view, DisplayPreferences::default(), Density::Full, 0).unwrap())[0][1],
        "—"
    );
}
#[test]
fn privacy_and_each_valid_selection_do_not_leak_private_text_or_drop_only_selected_item() {
    let mut view = fixture();
    view.privacy = true;
    view.scope_label = None;
    view.costs.clear();
    view.quota = None;
    for mask in 1..16 {
        let prefs = DisplayPreferences {
            layout: DisplayLayout::TwoRows,
            show_tokens: mask & 1 != 0,
            show_costs: mask & 2 != 0,
            show_quota: mask & 4 != 0,
            show_weekly_reset: mask & 8 != 0,
        };
        for density in [Density::Full, Density::Compact, Density::Minimal] {
            let output = rows(&view, prefs, density, 0).unwrap();
            assert!(!output.is_empty());
            let output = format!("{output:?}");
            assert!(!output.contains("SYNTHETIC"));
            assert!(!output.contains("0.57"));
            assert!(!output.contains("38%"));
        }
    }
    let invalid = DisplayPreferences {
        layout: DisplayLayout::SingleRow,
        show_tokens: false,
        show_costs: false,
        show_quota: false,
        show_weekly_reset: false,
    };
    assert!(rows(&view, invalid, Density::Full, 0).is_err());
    assert!(!accessible_text(&view).unwrap().contains("SYNTHETIC"));
    assert!(!accessible_text(&view).unwrap().contains("38%"));
    let mut original = fixture();
    let original_name = accessible_text(&original).unwrap();
    assert!(original_name.contains("683100"));
    assert!(original_name.contains("0.565000000000001"));
    original.costs.push(original.costs[0].clone());
    assert!(rows(&original, DisplayPreferences::default(), Density::Full, 0).is_err());
}
#[test]
fn measured_compaction_fallback_and_single_row_use_width_and_height_not_fixed_dips() {
    let view = fixture();
    let prefs = DisplayPreferences::default();
    let bound = |width, height| MeasureBounds {
        dpi: 96,
        font_height: 10,
        width,
        height,
    };
    let measure_text = |text: &str| Ok(text.chars().count() as i32 * 5);
    let full = measure(&view, prefs, 0, bound(1000, 48), measure_text)
        .unwrap()
        .unwrap();
    assert_eq!(full.density, Density::Full);
    // This independent 5px glyph metric gives full width 164 and compact width 126.
    let compact = measure(&view, prefs, 0, bound(150, 48), measure_text)
        .unwrap()
        .unwrap();
    assert_eq!(compact.density, Density::Compact);
    let minimal = measure(&view, prefs, 0, bound(110, 48), measure_text)
        .unwrap()
        .unwrap();
    assert_eq!(minimal.density, Density::Minimal);
    assert_eq!(minimal.spans.len(), 1);
    assert!(
        measure(&view, prefs, 0, bound(10, 48), measure_text)
            .unwrap()
            .is_none()
    );
    let one = measure(&view, prefs, 0, bound(1000, 23), measure_text)
        .unwrap()
        .unwrap();
    assert!(one.spans.iter().all(|s| s.y == one.spans[0].y));
    for plan in [full, compact, minimal, one] {
        for item in plan.spans {
            assert!(item.x >= 0 && item.x + item.width <= plan.width);
            assert!(item.y >= 0 && item.y + 10 <= plan.height);
        }
    }
    assert!(
        measure(&view, prefs, 0, bound(1000, i32::MAX), measure_text)
            .unwrap()
            .is_none()
    );
}
