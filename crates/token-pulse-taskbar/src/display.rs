//! Lossless presentation and measured density selection shared by native rendering and checks.
use crate::{TaskbarView, WireError};
use chrono::DateTime;
use chrono_tz::Tz;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use token_pulse_core::{
    numeric::{DecimalInt, DecimalMoney},
    protocol::{CoverageState, QuotaState, QuotaWindow},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DisplayLayout {
    TwoRows,
    SingleRow,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DisplayPreferences {
    pub layout: DisplayLayout,
    pub show_tokens: bool,
    pub show_costs: bool,
    pub show_quota: bool,
    pub show_weekly_reset: bool,
}
impl Default for DisplayPreferences {
    fn default() -> Self {
        Self {
            layout: DisplayLayout::TwoRows,
            show_tokens: true,
            show_costs: true,
            show_quota: true,
            show_weekly_reset: true,
        }
    }
}
impl DisplayPreferences {
    pub fn validate(self) -> Result<(), WireError> {
        if !(self.show_tokens || self.show_costs || self.show_quota || self.show_weekly_reset) {
            Err(WireError::InvalidFrame)
        } else {
            Ok(())
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Normal,
    Cost,
    Warning,
    Muted,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub tone: Tone,
}
fn span(text: impl Into<String>, tone: Tone) -> Span {
    Span {
        text: text.into(),
        tone,
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Density {
    Full,
    Compact,
    Minimal,
}
pub fn compact_tokens(value: Option<&DecimalInt>) -> String {
    let Some(value) = value else {
        return "—".into();
    };
    let n = value.value();
    for (scale, label) in [(1_000_000_000_i128, "B"), (1_000_000, "M"), (1_000, "K")] {
        if n >= scale {
            // Avoid n*10 overflow even at the allowed i128 maximum.
            let tenths = (n / scale) * 10 + ((n % scale) * 10 + scale / 2) / scale;
            return format!(
                "{}.{label_digit}{label}",
                tenths / 10,
                label_digit = tenths % 10
            );
        }
    }
    value.as_str().into()
}
pub fn money(value: Option<&DecimalMoney>) -> String {
    let Some(value) = value else {
        return "—".into();
    };
    let (whole, fraction) = value
        .as_str()
        .split_once('.')
        .unwrap_or((value.as_str(), ""));
    let atoms = format!("{whole}{fraction:0<15}")
        .parse::<i128>()
        .expect("validated money");
    let unit = 10_000_000_000_000_i128;
    let cents = atoms / unit + i128::from(atoms % unit >= unit / 2);
    format!("{}.{:02}", cents / 100, cents % 100)
}
pub fn percent(value: Option<f64>) -> String {
    value
        .map(|v| {
            format!("{v:.2}")
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_owned()
                + "%"
        })
        .unwrap_or_else(|| "—".into())
}
/// Full precision accessible name. Private fields are omitted at the same native boundary.
pub fn accessible_text(view: &TaskbarView) -> Result<String, WireError> {
    view.validate()?;
    let value = |n: Option<&DecimalInt>| n.map(|v| v.as_str()).unwrap_or("—").to_owned();
    let mut fields = vec![
        format!("Token {}", value(view.total_tokens.as_ref())),
        format!("输入 {}", value(view.input_tokens.as_ref())),
        format!("缓存 {}", value(view.cached_tokens.as_ref())),
        format!("输出 {}", value(view.output_tokens.as_ref())),
    ];
    if view.privacy {
        fields.push("费用和账户已隐藏".into());
    } else {
        if let Some(scope) = &view.scope_label {
            fields.push(format!("范围 {scope}"));
        }
        for cost in &view.costs {
            fields.push(format!(
                "{} {} 估算",
                cost.currency,
                cost.estimated_cost
                    .as_ref()
                    .map(|v| v.as_str())
                    .unwrap_or("—")
            ));
        }
        if view.unpriced_tokens.value() > 0 {
            fields.push(format!("未计价 Token {}", view.unpriced_tokens.as_str()));
        }
        if let Some(quota) = &view.quota.as_ref().filter(|q| {
            matches!(
                q.state,
                QuotaState::Ready | QuotaState::Stale | QuotaState::Error
            )
        }) {
            if let Some(label) = &quota.limit_label {
                fields.push(format!("账户额度桶 {label}"));
            }
            for window in &quota.windows {
                fields.push(format!(
                    "{} 分钟周期，剩余 {}",
                    window
                        .duration_mins
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "未知".into()),
                    percent(window.remaining_percent)
                ));
            }
        }
    }
    Ok(fields.join("；"))
}
fn roles(windows: &[QuotaWindow]) -> (Option<&QuotaWindow>, Option<&QuotaWindow>) {
    let weeks: Vec<_> = windows
        .iter()
        .filter(|w| w.duration_mins == Some(10080))
        .collect();
    let duration = windows
        .iter()
        .filter_map(|w| w.duration_mins.filter(|d| *d > 0 && *d < 10080))
        .min();
    let shorts: Vec<_> = windows
        .iter()
        .filter(|w| duration.is_some() && w.duration_mins == duration)
        .collect();
    (
        (shorts.len() == 1).then(|| shorts[0]),
        (weeks.len() == 1).then(|| weeks[0]),
    )
}
fn percent_span(label: &str, window: Option<&QuotaWindow>) -> Span {
    let value = window.and_then(|w| w.remaining_percent);
    span(
        format!("{label} {}", percent(value)),
        if value.is_some_and(|v| v <= 20.0) {
            Tone::Warning
        } else {
            Tone::Normal
        },
    )
}
fn date(window: Option<&QuotaWindow>, view: &TaskbarView, now: i64) -> String {
    let Some(reset) = window.and_then(|w| w.resets_at_ms) else {
        return "周重置 —".into();
    };
    if reset.value() <= now {
        return "周重置待更新".into();
    }
    let Ok(timezone) = view.timezone.parse::<Tz>() else {
        return "周重置 时区无效".into();
    };
    DateTime::from_timestamp_millis(reset.value())
        .map(|t| {
            format!(
                "周重置 {}",
                t.with_timezone(&timezone).format("%m/%d %H:%M")
            )
        })
        .unwrap_or_else(|| "周重置 时间无效".into())
}
fn costs(view: &TaskbarView, full: bool) -> Span {
    if view.privacy {
        return span("••••", Tone::Muted);
    }
    if view.costs.is_empty() {
        return span(
            if view.unpriced_tokens.value() > 0 {
                "未计价"
            } else {
                "费用 —"
            },
            Tone::Muted,
        );
    }
    let text = view
        .costs
        .iter()
        .map(|cost| format!("{} {}", cost.currency, money(cost.estimated_cost.as_ref())))
        .collect::<Vec<_>>()
        .join(" / ");
    let partial = view.unpriced_tokens.value() > 0;
    span(
        if full {
            format!("{text} 估算{}", if partial { " · 部分未计价" } else { "" })
        } else {
            format!("{text}{}", if partial { "*" } else { "" })
        },
        Tone::Cost,
    )
}
/// Unknown and ambiguous quota windows stay unknown; ids/order do not define a weekly role.
pub fn rows(
    view: &TaskbarView,
    prefs: DisplayPreferences,
    density: Density,
    now: i64,
) -> Result<Vec<Vec<Span>>, WireError> {
    view.validate()?;
    prefs.validate()?;
    let mut first = vec![];
    let mut second = vec![];
    if prefs.show_tokens {
        first.push(span(
            format!("Token {}", compact_tokens(view.total_tokens.as_ref())),
            Tone::Normal,
        ));
        if density == Density::Full && !matches!(view.usage_status, CoverageState::Complete) {
            first.push(span("待核对", Tone::Muted));
        }
    }
    if prefs.show_costs {
        first.push(costs(view, density == Density::Full));
    }
    if prefs.show_quota {
        if view.privacy {
            second.push(span("额度已隐藏", Tone::Muted));
        } else if let Some(quota) = view.quota.as_ref().filter(|q| {
            matches!(
                q.state,
                QuotaState::Ready | QuotaState::Stale | QuotaState::Error
            ) && !q.windows.is_empty()
        }) {
            let (short, weekly) = roles(&quota.windows);
            if density == Density::Full {
                if let Some(short) = short {
                    let length = short.duration_mins.expect("known actual period");
                    let label = if length % 60 == 0 {
                        format!("{}h", length / 60)
                    } else {
                        format!("{length}m")
                    };
                    second.push(percent_span(&label, Some(short)));
                }
            }
            second.push(percent_span("周", weekly));
            if matches!(quota.state, QuotaState::Stale | QuotaState::Error) {
                second.push(span("旧快照", Tone::Warning));
            }
        } else {
            let text = match view.quota.as_ref().map(|q| q.state) {
                Some(QuotaState::Connecting) => "额度读取中",
                Some(QuotaState::AuthorizationRequired) => "本地登录态不可用",
                Some(QuotaState::Unsupported) => "额度不支持",
                Some(QuotaState::Error) => "额度读取失败",
                Some(QuotaState::Stale) => "额度旧快照",
                Some(QuotaState::Ready) => "额度未提供",
                _ => "额度未连接",
            };
            second.push(span(text, Tone::Muted));
        }
    }
    if prefs.show_weekly_reset
        && (density == Density::Full
            || (!prefs.show_tokens && !prefs.show_costs && !prefs.show_quota))
    {
        let weekly = view
            .quota
            .as_ref()
            .filter(|q| {
                matches!(
                    q.state,
                    QuotaState::Ready | QuotaState::Stale | QuotaState::Error
                )
            })
            .and_then(|q| roles(&q.windows).1);
        second.push(span(
            if view.privacy {
                "周重置已隐藏".into()
            } else {
                date(weekly, view, now)
            },
            Tone::Muted,
        ));
    }
    if density == Density::Minimal {
        let chosen = if prefs.show_tokens || prefs.show_costs {
            first.into_iter().next()
        } else {
            second.into_iter().next()
        };
        return Ok(vec![vec![chosen.expect("at least one selected item")]]);
    }
    let mut output = vec![];
    if !first.is_empty() {
        output.push(first);
    }
    if !second.is_empty() {
        output.push(second);
    }
    if prefs.layout == DisplayLayout::SingleRow {
        output = vec![output.into_iter().flatten().collect()];
    }
    Ok(output)
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedSpan {
    pub span: Span,
    pub x: i32,
    pub y: i32,
    pub width: i32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeasuredPlan {
    pub width: i32,
    pub height: i32,
    pub density: Density,
    pub spans: Vec<PlacedSpan>,
}
/// Measure every string with the renderer's selected font; refuse unreadable clipping.
#[derive(Debug, Clone, Copy)]
pub struct MeasureBounds {
    pub dpi: u32,
    pub font_height: i32,
    pub width: i32,
    pub height: i32,
}
pub fn measure(
    view: &TaskbarView,
    mut prefs: DisplayPreferences,
    now: i64,
    bounds: MeasureBounds,
    mut width: impl FnMut(&str) -> Result<i32, WireError>,
) -> Result<Option<MeasuredPlan>, WireError> {
    let MeasureBounds {
        dpi,
        font_height,
        width: available_width,
        height: available_height,
    } = bounds;
    if !(96..=768).contains(&dpi)
        || !(1..=2048).contains(&font_height)
        || !(1..=32768).contains(&available_width)
        || !(1..=32768).contains(&available_height)
    {
        return Ok(None);
    }
    let pad = (6 * dpi / 96) as i32;
    let left = (12 * dpi / 96) as i32;
    let gap = (8 * dpi / 96) as i32;
    let line_gap = (2 * dpi / 96) as i32;
    if font_height * 2 + line_gap + pad * 2 > available_height {
        prefs.layout = DisplayLayout::SingleRow;
    }
    for density in [Density::Full, Density::Compact, Density::Minimal] {
        let rows = rows(view, prefs, density, now)?;
        let height = font_height * rows.len() as i32 + line_gap * (rows.len() as i32 - 1);
        if height + pad * 2 > available_height {
            continue;
        }
        let mut widest = 0i32;
        let mut spans = vec![];
        for (row, items) in rows.into_iter().enumerate() {
            let mut x = left;
            for item in items {
                let measured = width(&item.text)?;
                if !(0..=32_768).contains(&measured) {
                    return Err(WireError::InvalidFrame);
                }
                spans.push(PlacedSpan {
                    span: item,
                    x,
                    y: (available_height - height) / 2 + row as i32 * (font_height + line_gap),
                    width: measured,
                });
                x = x
                    .checked_add(measured + gap)
                    .ok_or(WireError::InvalidFrame)?;
            }
            widest = widest.max(x - gap + pad);
        }
        if widest <= available_width {
            return Ok(Some(MeasuredPlan {
                width: widest,
                height: available_height,
                density,
                spans,
            }));
        }
    }
    Ok(None)
}
