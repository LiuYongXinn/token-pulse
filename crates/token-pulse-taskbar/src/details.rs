//! Read-only native details from the validated display projection. Never queries a source.
use crate::{
    HostScope, TaskbarView, WireError,
    display::{Tone, percent},
};
use chrono::DateTime;
use chrono_tz::Tz;
use token_pulse_core::{
    numeric::{DecimalInt, EpochMs},
    protocol::QuotaState,
};

#[derive(Debug, Clone, PartialEq)]
pub struct DetailRow {
    pub label: String,
    pub value: String,
    pub tone: Tone,
    /// The actual account remaining percentage. None is an unknown bar, never a zero bar.
    pub remaining_percent: Option<f64>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct DetailContent {
    pub title: String,
    pub rows: Vec<DetailRow>,
}
impl DetailContent {
    pub fn accessible_text(&self) -> String {
        std::iter::once(self.title.clone())
            .chain(
                self.rows
                    .iter()
                    .map(|row| format!("{}：{}", row.label, row.value)),
            )
            .collect::<Vec<_>>()
            .join("；")
    }
}
fn row(label: &str, value: impl Into<String>, tone: Tone) -> DetailRow {
    DetailRow {
        label: label.into(),
        value: value.into(),
        tone,
        remaining_percent: None,
    }
}
fn integer(value: Option<&DecimalInt>) -> String {
    value
        .map(|value| value.as_str())
        .unwrap_or("—（未提供）")
        .into()
}
fn absolute(value: Option<EpochMs>, timezone: Tz) -> String {
    match value {
        None => "—（未提供）".into(),
        Some(value) => DateTime::from_timestamp_millis(value.value())
            .map(|date| {
                date.with_timezone(&timezone)
                    .format("%Y-%m-%d %H:%M:%S%.3f")
                    .to_string()
            })
            .unwrap_or_else(|| "时间超出显示范围".into()),
    }
}
/// Rounded two-decimal percentage without multiplying a large Token count or using f64.
pub fn price_coverage(
    priced: &DecimalInt,
    total: Option<&DecimalInt>,
) -> Result<String, WireError> {
    let Some(total) = total else {
        return Ok("—（用量未知）".into());
    };
    let denominator = total.value();
    if priced.value() > denominator {
        return Err(WireError::InvalidFrame);
    }
    if denominator == 0 {
        return Ok("—（无用量）".into());
    }
    let mut remainder = priced.value() % denominator;
    let mut basis_points = (priced.value() / denominator) * 10_000;
    for _ in 0..4 {
        let mut next = 0;
        let mut digit = 0;
        // Modular addition stays below denominator even when it equals i128::MAX.
        for _ in 0..10 {
            if next >= denominator - remainder {
                next -= denominator - remainder;
                digit += 1;
            } else {
                next += remainder;
            }
        }
        basis_points = if basis_points == 10_000 {
            10_000
        } else {
            basis_points * 10 + digit
        };
        remainder = next;
    }
    if remainder >= denominator - remainder {
        basis_points += 1;
    }
    Ok(format!("{}.{:02}%", basis_points / 100, basis_points % 100))
}
fn countdown(reset: Option<EpochMs>, now: i64) -> String {
    match reset {
        None => "重置时间未提供".into(),
        Some(reset) if reset.value() <= now => "重置时间已到，等待账户服务更新".into(),
        Some(reset) => {
            let total_minutes = (reset.value() - now + 59_999) / 60_000;
            let days = total_minutes / 1440;
            let hours = (total_minutes % 1440) / 60;
            let minutes = total_minutes % 60;
            format!("距重置 {days} 天 {hours} 小时 {minutes} 分钟")
        }
    }
}
fn period(minutes: Option<u32>) -> String {
    match minutes {
        Some(10080) => "周额度（7 天）".into(),
        Some(value) if value % 1440 == 0 => format!("{} 天额度", value / 1440),
        Some(value) if value % 60 == 0 => format!("{} 小时额度", value / 60),
        Some(value) => format!("{value} 分钟额度"),
        None => "周期未提供".into(),
    }
}
pub fn content(view: &TaskbarView, now: i64) -> Result<DetailContent, WireError> {
    view.validate()?;
    EpochMs::new(now).map_err(|_| WireError::InvalidFrame)?;
    let timezone = view
        .timezone
        .parse::<Tz>()
        .map_err(|_| WireError::InvalidFrame)?;
    let mut rows = vec![
        row(
            "Token 用量",
            integer(view.total_tokens.as_ref()),
            Tone::Normal,
        ),
        row("输入", integer(view.input_tokens.as_ref()), Tone::Normal),
        row(
            "缓存输入（包含在输入中）",
            integer(view.cached_tokens.as_ref()),
            Tone::Normal,
        ),
        row("输出", integer(view.output_tokens.as_ref()), Tone::Normal),
    ];
    if let Some(details) = &view.details {
        let scope = match details.scope {
            HostScope::TodayAllSources => "全部来源 · 今日",
            HostScope::TodaySession => "固定会话 · 今日",
            HostScope::FixedSession => "固定会话 · 指定起点",
        };
        rows.push(row(
            "统计范围",
            if view.privacy {
                scope.into()
            } else {
                view.scope_label.clone().unwrap_or_else(|| scope.into())
            },
            Tone::Muted,
        ));
        rows.push(row(
            "时间范围",
            format!(
                "{} 至 {}",
                absolute(Some(details.range.start_ms), timezone),
                absolute(Some(details.range.end_ms), timezone)
            ),
            Tone::Muted,
        ));
    } else {
        rows.push(row("范围详情", "尚未提供", Tone::Muted));
    }
    rows.push(row(
        "用量更新时间",
        absolute(Some(view.generated_at_ms), timezone),
        Tone::Muted,
    ));
    rows.push(row("统计时区", view.timezone.clone(), Tone::Muted));
    if view.privacy {
        rows.push(row(
            "隐私模式",
            "费用、会话名称和账户额度已隐藏",
            Tone::Muted,
        ));
    } else {
        if view.costs.is_empty() {
            rows.push(row(
                "已计价部分估算",
                if view.unpriced_tokens.value() > 0 {
                    "未计价"
                } else {
                    "—（未提供）"
                },
                Tone::Muted,
            ));
        }
        for cost in &view.costs {
            rows.push(row(
                "已计价部分估算",
                format!(
                    "{} {}",
                    cost.currency,
                    cost.estimated_cost
                        .as_ref()
                        .map(|money| money.as_str())
                        .unwrap_or("—（未提供）")
                ),
                Tone::Cost,
            ));
        }
        rows.push(row(
            "未计价 Token",
            view.unpriced_tokens.as_str(),
            if view.unpriced_tokens.value() > 0 {
                Tone::Warning
            } else {
                Tone::Muted
            },
        ));
        if view
            .details
            .as_ref()
            .is_some_and(|details| details.pricing_calculating)
        {
            rows.push(row("计价状态", "正在计算", Tone::Warning));
        }
        match &view.quota {
            None => rows.push(row("账户额度", "未连接，额度未提供", Tone::Muted)),
            Some(quota) => {
                let status = match quota.state {
                    QuotaState::Ready => "已连接",
                    QuotaState::Disconnected => "未连接",
                    QuotaState::Connecting => "正在连接",
                    QuotaState::AuthorizationRequired => "本地登录态不可用",
                    QuotaState::Unsupported => "账户服务不支持",
                    QuotaState::Stale => "更新失败，可重试",
                    QuotaState::Error => "更新失败，显示上次结果",
                };
                rows.push(row(
                    "账户额度",
                    status,
                    if matches!(quota.state, QuotaState::Stale | QuotaState::Error) {
                        Tone::Warning
                    } else {
                        Tone::Muted
                    },
                ));
                if let Some(label) = &quota.limit_label {
                    rows.push(row("账户方案", label.clone(), Tone::Muted));
                }
                // A disconnected/unsupported account must not display stale windows as current.
                if matches!(
                    quota.state,
                    QuotaState::Ready | QuotaState::Stale | QuotaState::Error
                ) {
                    if quota.windows.is_empty() {
                        rows.push(row("额度窗口", "—（服务未提供）", Tone::Muted));
                    }
                    for (index, window) in quota.windows.iter().enumerate() {
                        let mut remaining = row(
                            &format!("窗口 {} · {}", index + 1, period(window.duration_mins)),
                            format!("剩余 {}", percent(window.remaining_percent)),
                            if window.remaining_percent.is_some_and(|n| n <= 20.0) {
                                Tone::Warning
                            } else {
                                Tone::Normal
                            },
                        );
                        remaining.remaining_percent = window.remaining_percent;
                        rows.push(remaining);
                        rows.push(row(
                            "重置时间",
                            format!(
                                "{} · {}",
                                absolute(window.resets_at_ms, timezone),
                                countdown(window.resets_at_ms, now)
                            ),
                            Tone::Muted,
                        ));
                    }
                }
                rows.push(row(
                    "额度更新时间",
                    absolute(quota.fetched_at_ms, timezone),
                    Tone::Muted,
                ));
            }
        }
    }
    Ok(DetailContent {
        title: "TokenPulse · 用量详情".into(),
        rows,
    })
}
