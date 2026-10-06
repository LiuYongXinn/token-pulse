//! Read-only native details from the validated display projection. Never queries a source.
use crate::{
    HostScope, HostSourceStatus, TaskbarView, WireError,
    display::{Tone, percent},
};
use chrono::DateTime;
use chrono_tz::Tz;
use token_pulse_core::{
    numeric::{DecimalInt, EpochMs},
    protocol::{CoverageState, QuotaState},
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
fn measure(value: Option<&DecimalInt>, complete: Option<bool>) -> String {
    if value.is_none() {
        return integer(value);
    }
    match complete {
        Some(true) => integer(value),
        Some(false) => format!("{}（仅已提供部分）", integer(value)),
        None => format!("{}（完整性未提供）", integer(value)),
    }
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
            "可信 Token",
            integer(view.total_tokens.as_ref()),
            Tone::Normal,
        ),
        row(
            "输入总数",
            measure(
                view.input_tokens.as_ref(),
                view.details.as_ref().map(|details| details.input_complete),
            ),
            Tone::Normal,
        ),
        row(
            "缓存输入（包含在输入中）",
            measure(
                view.cached_tokens.as_ref(),
                view.details.as_ref().map(|details| details.cached_complete),
            ),
            Tone::Normal,
        ),
        row(
            "输出总数",
            measure(
                view.output_tokens.as_ref(),
                view.details.as_ref().map(|details| details.output_complete),
            ),
            Tone::Normal,
        ),
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
            "时间范围（左闭右开）",
            format!(
                "{} 至 {}",
                absolute(Some(details.range.start_ms), timezone),
                absolute(Some(details.range.end_ms), timezone)
            ),
            Tone::Muted,
        ));
        if !details.breakdown_complete {
            rows.push(row(
                "分项口径",
                "部分计数未提供，未知分项保留未知",
                Tone::Warning,
            ));
        }
        rows.push(row(
            "来源最近成功核对",
            absolute(details.source_last_success_at_ms, timezone),
            Tone::Muted,
        ));
        for state in &details.source_statuses {
            let text = match state {
                HostSourceStatus::Paused => "来源已暂停，历史统计保留",
                HostSourceStatus::AwaitingDirectory => "等待选择来源目录",
                HostSourceStatus::PartiallyReadable => "部分来源文件不可读",
                HostSourceStatus::Unreadable => "来源不可读，保留可信统计",
                HostSourceStatus::ScanEvidenceMissing => "尚无来源成功核对时间",
                HostSourceStatus::Scanning => "正在核对来源目录",
                HostSourceStatus::ScanPending => "部分文件待采集或重新核对",
                HostSourceStatus::ScanInterrupted => "来源核对已中断，等待补扫",
                HostSourceStatus::ScanChanged => "来源文件发生变化，等待重新核对",
                HostSourceStatus::ScanIncomplete => "来源核对未完成，请查看采集诊断",
                HostSourceStatus::Unknown => "来源状态未识别",
            };
            rows.push(row("来源状态", text, Tone::Warning));
        }
        if details.pending_observations.value() > 0 {
            rows.push(row(
                "待确认观察",
                details.pending_observations.as_str(),
                Tone::Warning,
            ));
        }
        if details.pending_files.value() > 0 {
            rows.push(row(
                "待核对文件",
                details.pending_files.as_str(),
                Tone::Warning,
            ));
        }
    } else {
        rows.push(row("范围详情", "尚未提供", Tone::Muted));
    }
    rows.push(row(
        "用量覆盖",
        match view.usage_status {
            CoverageState::Complete => "完整",
            CoverageState::Partial => "部分数据待确认，仅显示可信统计",
            CoverageState::Unknown => "未知，尚无完整来源证据",
        },
        if matches!(view.usage_status, CoverageState::Complete) {
            Tone::Muted
        } else {
            Tone::Warning
        },
    ));
    rows.push(row(
        "用量快照生成",
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
            "价格覆盖",
            price_coverage(&view.priced_tokens, view.total_tokens.as_ref())?,
            Tone::Muted,
        ));
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
            rows.push(row(
                "计价状态",
                "正在计算，价格版本与 Token 账本分别处理",
                Tone::Warning,
            ));
        }
        rows.push(row(
            "费用口径",
            "等价价格估算；与订阅实付、账户额度分别计算",
            Tone::Muted,
        ));
        match &view.quota {
            None => rows.push(row("账户额度", "未连接，额度未提供", Tone::Muted)),
            Some(quota) => {
                let status = match quota.state {
                    QuotaState::Ready => "已连接",
                    QuotaState::Disconnected => "未连接",
                    QuotaState::Connecting => "正在连接",
                    QuotaState::AuthorizationRequired => "本地登录态不可用",
                    QuotaState::Unsupported => "账户服务不支持",
                    QuotaState::Stale => "旧快照，等待刷新",
                    QuotaState::Error => "读取失败，保留同一账户的旧值",
                };
                rows.push(row(
                    "账户额度（独立范围）",
                    status,
                    if matches!(quota.state, QuotaState::Stale | QuotaState::Error) {
                        Tone::Warning
                    } else {
                        Tone::Muted
                    },
                ));
                if let Some(label) = &quota.limit_label {
                    rows.push(row("额度桶", label.clone(), Tone::Muted));
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
                    "账户成功读取",
                    absolute(quota.fetched_at_ms, timezone),
                    Tone::Muted,
                ));
                rows.push(row(
                    "账户最近尝试",
                    absolute(quota.last_attempt_at_ms, timezone),
                    Tone::Muted,
                ));
                if let Some(code) = quota.error_code {
                    rows.push(row(
                        "账户读取状态",
                        serde_json::to_value(code)
                            .map_err(|_| WireError::InvalidFrame)?
                            .as_str()
                            .ok_or(WireError::InvalidFrame)?
                            .to_owned(),
                        Tone::Warning,
                    ));
                }
            }
        }
    }
    Ok(DetailContent {
        title: "TokenPulse · 用量详情".into(),
        rows,
    })
}
