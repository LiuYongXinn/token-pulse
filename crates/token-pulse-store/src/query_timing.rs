//! Bounded anonymous monotonic process timings, without query bodies or identifiers.
use std::{collections::VecDeque, sync::Mutex, time::Instant};
#[derive(Debug, Clone, serde::Serialize)]
pub struct QueryTiming {
    pub stage: &'static str,
    pub milliseconds: f64,
}
static TIMINGS: Mutex<VecDeque<QueryTiming>> = Mutex::new(VecDeque::new());
pub(crate) fn record(stage: &'static str, started: Instant) {
    record_duration(stage, started.elapsed());
}
pub(crate) fn record_duration(stage: &'static str, duration: std::time::Duration) {
    if let Ok(mut timings) = TIMINGS.lock() {
        if timings.len() >= 2048 {
            timings.pop_front();
        }
        timings.push_back(QueryTiming {
            stage,
            milliseconds: duration.as_secs_f64() * 1000.0,
        });
    }
}
pub fn take() -> Vec<QueryTiming> {
    TIMINGS
        .lock()
        .map(|mut timings| timings.drain(..).collect())
        .unwrap_or_default()
}
