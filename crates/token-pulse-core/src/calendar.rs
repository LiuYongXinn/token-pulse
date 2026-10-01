//! Calendar-aligned, half-open UTC buckets. Labels never identify a bucket.
use crate::{error::ErrorCode, numeric::EpochMs, protocol::DateRange};
use chrono::{
    DateTime, Datelike, Duration, LocalResult, NaiveDate, NaiveDateTime, Offset, TimeZone,
    Timelike, Utc,
};
use chrono_tz::{GapInfo, Tz};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use ts_rs::TS;

pub const MAX_BUCKETS: usize = 2000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum Grain {
    Hour,
    Day,
    Month,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct CalendarBucket {
    pub start_ms: EpochMs,
    pub end_ms: EpochMs,
    pub display_label: String,
    pub utc_offset: String,
}

fn invalid<T>() -> Result<T, ErrorCode> {
    Err(ErrorCode::InvalidQuery)
}
fn utc(ms: i64) -> Result<DateTime<Utc>, ErrorCode> {
    DateTime::from_timestamp_millis(ms).ok_or(ErrorCode::InvalidQuery)
}
fn local_boundaries(
    local: NaiveDateTime,
    tz: Tz,
    all: bool,
    points: &mut BTreeSet<i64>,
) -> Result<(), ErrorCode> {
    match tz.from_local_datetime(&local) {
        LocalResult::Single(dt) => {
            points.insert(dt.timestamp_millis());
        }
        LocalResult::Ambiguous(a, b) => {
            points.insert(a.timestamp_millis().min(b.timestamp_millis()));
            if all {
                points.insert(a.timestamp_millis().max(b.timestamp_millis()));
            }
        }
        LocalResult::None => {
            let end = GapInfo::new(&local, &tz)
                .and_then(|g| g.end)
                .ok_or(ErrorCode::InvalidQuery)?;
            points.insert(end.timestamp_millis());
        }
    }
    Ok(())
}
fn midnight(date: NaiveDate) -> Result<NaiveDateTime, ErrorCode> {
    date.and_hms_opt(0, 0, 0).ok_or(ErrorCode::InvalidQuery)
}

/// Every offset change is also an hour boundary, including half-hour DST and
/// historical second-based offsets. IANA transitions are separated by more
/// than a minute; minute probes locate changes, then binary search to the ms.
fn hour_transitions(
    tz: Tz,
    start: i64,
    end: i64,
    points: &mut BTreeSet<i64>,
) -> Result<(), ErrorCode> {
    let offset = |ms| -> Result<i32, ErrorCode> {
        Ok(utc(ms)?.with_timezone(&tz).offset().fix().local_minus_utc())
    };
    let mut left = start;
    let mut old = offset(left)?;
    while left < end {
        let right = (left + 60_000).min(end);
        let new = offset(right)?;
        if new != old {
            let (mut lo, mut hi) = (left, right);
            while hi - lo > 1 {
                let mid = lo + (hi - lo) / 2;
                if offset(mid)? == old {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            points.insert(hi);
        }
        left = right;
        old = new;
    }
    Ok(())
}

pub fn buckets(range: &DateRange, grain: Grain) -> Result<Vec<CalendarBucket>, ErrorCode> {
    range.validate()?;
    let tz: Tz = range
        .timezone
        .parse()
        .map_err(|_| ErrorCode::InvalidQuery)?;
    let start = utc(range.start_ms.value())?;
    let end = utc(range.end_ms.value())?;
    // Reject pathological ranges before allocation or transition probing.
    let max_span_days = match grain {
        Grain::Hour => 84,
        Grain::Day => 2200,
        Grain::Month => 64_100,
    };
    if end.signed_duration_since(start) > Duration::days(max_span_days) {
        return invalid();
    }
    let local_start = start.with_timezone(&tz).naive_local();
    let local_end = end.with_timezone(&tz).naive_local();
    let mut points = BTreeSet::new();
    match grain {
        Grain::Hour => {
            let mut local = local_start
                .with_minute(0)
                .and_then(|d| d.with_second(0))
                .and_then(|d| d.with_nanosecond(0))
                .and_then(|d| d.checked_sub_signed(Duration::days(2)))
                .ok_or(ErrorCode::InvalidQuery)?;
            let stop = local_end
                .checked_add_signed(Duration::days(2))
                .ok_or(ErrorCode::InvalidQuery)?;
            while local <= stop {
                local_boundaries(local, tz, true, &mut points)?;
                local = local
                    .checked_add_signed(Duration::hours(1))
                    .ok_or(ErrorCode::InvalidQuery)?;
            }
            let pad = Duration::days(2).num_milliseconds();
            hour_transitions(
                tz,
                start
                    .timestamp_millis()
                    .checked_sub(pad)
                    .ok_or(ErrorCode::InvalidQuery)?,
                end.timestamp_millis()
                    .checked_add(pad)
                    .ok_or(ErrorCode::InvalidQuery)?,
                &mut points,
            )?;
        }
        Grain::Day => {
            let mut date = local_start
                .date()
                .checked_sub_signed(Duration::days(2))
                .ok_or(ErrorCode::InvalidQuery)?;
            let stop = local_end
                .date()
                .checked_add_signed(Duration::days(2))
                .ok_or(ErrorCode::InvalidQuery)?;
            while date <= stop {
                local_boundaries(midnight(date)?, tz, false, &mut points)?;
                date = date.succ_opt().ok_or(ErrorCode::InvalidQuery)?;
            }
        }
        Grain::Month => {
            let mut date = NaiveDate::from_ymd_opt(local_start.year(), local_start.month(), 1)
                .ok_or(ErrorCode::InvalidQuery)?;
            let stop = NaiveDate::from_ymd_opt(local_end.year(), local_end.month(), 1)
                .ok_or(ErrorCode::InvalidQuery)?;
            loop {
                local_boundaries(midnight(date)?, tz, false, &mut points)?;
                let next = if date.month() == 12 {
                    NaiveDate::from_ymd_opt(date.year() + 1, 1, 1)
                } else {
                    NaiveDate::from_ymd_opt(date.year(), date.month() + 1, 1)
                }
                .ok_or(ErrorCode::InvalidQuery)?;
                if date > stop {
                    break;
                }
                date = next;
            }
        }
    }
    let mut result = Vec::new();
    let mut previous: Option<i64> = None;
    for point in points {
        if let Some(left) = previous {
            let clipped_start = left.max(range.start_ms.value());
            let clipped_end = point.min(range.end_ms.value());
            if clipped_start < clipped_end {
                if result.len() == MAX_BUCKETS {
                    return invalid();
                }
                let label_time = utc(left)?.with_timezone(&tz);
                let format = match grain {
                    Grain::Hour => "%Y-%m-%d %H:%M",
                    Grain::Day => "%Y-%m-%d",
                    Grain::Month => "%Y-%m",
                };
                result.push(CalendarBucket {
                    start_ms: EpochMs::new(clipped_start)?,
                    end_ms: EpochMs::new(clipped_end)?,
                    display_label: label_time.format(format).to_string(),
                    utc_offset: {
                        let seconds = label_time.offset().fix().local_minus_utc();
                        let sign = if seconds < 0 { '-' } else { '+' };
                        let absolute = seconds.unsigned_abs();
                        let base =
                            format!("{sign}{:02}:{:02}", absolute / 3600, absolute / 60 % 60);
                        if absolute % 60 == 0 {
                            base
                        } else {
                            format!("{base}:{:02}", absolute % 60)
                        }
                    },
                });
            }
        }
        previous = Some(point);
    }
    if result.first().map(|b| b.start_ms) != Some(range.start_ms)
        || result.last().map(|b| b.end_ms) != Some(range.end_ms)
    {
        return invalid();
    }
    Ok(result)
}
