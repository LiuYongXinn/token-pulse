//! Retained main-window navigation; one monotonic domain prevents competing late requests.
use crate::{error::ErrorCode, mini::MiniStatsRequest, numeric::DecimalInt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MainNavigationIntent {
    MiniStats { request: Box<MiniStatsRequest> },
    TaskbarSettings {},
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn navigation_revision_is_exact_monotonic_and_overflow_preserves_previous_intent() {
        let mut snapshot = MainNavigationSnapshot::default();
        assert_eq!(snapshot.revision.as_str(), "0");
        assert!(snapshot.intent.is_none());
        snapshot.revision = DecimalInt::parse("9007199254740993").unwrap();
        snapshot
            .publish(MainNavigationIntent::TaskbarSettings {})
            .unwrap();
        assert_eq!(snapshot.revision.as_str(), "9007199254740994");
        assert!(snapshot.mini_stats().is_none());
        snapshot.revision = DecimalInt::parse(&i128::MAX.to_string()).unwrap();
        assert_eq!(
            snapshot
                .publish(MainNavigationIntent::TaskbarSettings {})
                .unwrap_err(),
            ErrorCode::NumericOverflow
        );
        assert_eq!(snapshot.revision.value(), i128::MAX);
        assert!(matches!(
            snapshot.intent,
            Some(MainNavigationIntent::TaskbarSettings {})
        ));
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MainNavigationSnapshot {
    pub revision: DecimalInt,
    pub intent: Option<MainNavigationIntent>,
}
impl Default for MainNavigationSnapshot {
    fn default() -> Self {
        Self {
            revision: DecimalInt::parse("0").expect("literal"),
            intent: None,
        }
    }
}
impl MainNavigationSnapshot {
    pub fn publish(&mut self, intent: MainNavigationIntent) -> Result<(), ErrorCode> {
        let next = self
            .revision
            .value()
            .checked_add(1)
            .ok_or(ErrorCode::NumericOverflow)?;
        self.revision = DecimalInt::from_nonnegative(next)?;
        self.intent = Some(intent);
        Ok(())
    }
    pub fn mini_stats(&self) -> Option<MiniStatsRequest> {
        match &self.intent {
            Some(MainNavigationIntent::MiniStats { request }) => Some((**request).clone()),
            _ => None,
        }
    }
}
