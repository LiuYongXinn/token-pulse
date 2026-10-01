//! Latest display policy is applied at serialization, outside frozen usage snapshots.
use crate::{numeric::DecimalInt, pricing::PriceOutcome, protocol::*, query::*, sources::*};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct DisplayPolicyStamp {
    pub settings_revision: DecimalInt,
    pub privacy: bool,
}

/// Only the owner of committed display settings publishes a replacement stamp.
#[derive(Debug, Clone)]
pub struct PrivacyState(Arc<Mutex<Option<DisplayPolicyStamp>>>);
impl PrivacyState {
    pub fn new(stamp: DisplayPolicyStamp) -> Self {
        Self(Arc::new(Mutex::new(Some(stamp))))
    }
    pub fn current(&self) -> Result<DisplayPolicyStamp, crate::error::ErrorCode> {
        self.0
            .lock()
            .map_err(|_| crate::error::ErrorCode::DbCorrupt)?
            .clone()
            .ok_or(crate::error::ErrorCode::DbCorrupt)
    }
    pub fn publish(&self, stamp: DisplayPolicyStamp) -> Result<(), crate::error::ErrorCode> {
        let mut current = self
            .0
            .lock()
            .map_err(|_| crate::error::ErrorCode::DbCorrupt)?;
        let previous = current.as_ref().ok_or(crate::error::ErrorCode::DbCorrupt)?;
        if stamp.settings_revision.value() < previous.settings_revision.value() {
            return Err(crate::error::ErrorCode::RevisionConflict);
        }
        if stamp.settings_revision.value() == previous.settings_revision.value()
            && stamp.privacy != previous.privacy
        {
            return Err(crate::error::ErrorCode::RevisionConflict);
        }
        *current = Some(stamp);
        Ok(())
    }
    /// Coordinate the actual settings transaction with responses; no response can serialize an old policy after commit.
    pub fn commit_update<T, E: From<crate::error::ErrorCode>>(
        &self,
        change: impl FnOnce() -> Result<(T, DisplayPolicyStamp), E>,
    ) -> Result<T, E> {
        let mut current = self
            .0
            .lock()
            .map_err(|_| crate::error::ErrorCode::DbCorrupt)?;
        let previous = current
            .as_ref()
            .ok_or(crate::error::ErrorCode::DbCorrupt)?
            .clone();
        let (result, stamp) = change()?;
        if stamp.settings_revision.value() < previous.settings_revision.value()
            || (stamp.settings_revision.value() == previous.settings_revision.value()
                && stamp.privacy != previous.privacy)
        {
            // The caller claims its write committed but returned an impossible stamp. Stop all display publication.
            *current = None;
            return Err(crate::error::ErrorCode::RevisionConflict.into());
        }
        *current = Some(stamp);
        Ok(result)
    }
}

/// Each public data DTO explicitly implements this trait; no permissive blanket implementation.
pub trait PrivacyRedact {
    fn redact(&mut self);
    fn redact_group(&mut self, _dimension: GroupDimension) {
        self.redact();
    }
}

/// Constructing this response does not freeze policy. The lock covers both data and stamp serialization.
pub struct PrivateResponse<T> {
    request_id: String,
    data: T,
    policy: PrivacyState,
    group_dimension: Option<GroupDimension>,
}
impl<T> PrivateResponse<T> {
    pub fn new(request_id: String, data: T, policy: PrivacyState) -> Self {
        Self {
            request_id,
            data,
            policy,
            group_dimension: None,
        }
    }
}
impl PrivateResponse<GroupedUsageBundle> {
    pub fn groups(
        request_id: String,
        data: GroupedUsageBundle,
        policy: PrivacyState,
        dimension: GroupDimension,
    ) -> Self {
        Self {
            request_id,
            data,
            policy,
            group_dimension: Some(dimension),
        }
    }
}
impl<T: Clone + Serialize + PrivacyRedact> Serialize for PrivateResponse<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let guard = self
            .policy
            .0
            .lock()
            .map_err(|_| serde::ser::Error::custom("DISPLAY_POLICY_UNAVAILABLE"))?;
        let policy = guard
            .as_ref()
            .ok_or_else(|| serde::ser::Error::custom("DISPLAY_POLICY_UNAVAILABLE"))?;
        let mut data = self.data.clone();
        if policy.privacy {
            match self.group_dimension {
                Some(dimension) => data.redact_group(dimension),
                None => data.redact(),
            }
        }
        let mut response = Response::new(self.request_id.clone(), data);
        response.display_policy = Some(policy.clone());
        response.serialize(serializer)
    }
}

/// Labels are deterministic by stable identity; names and paths never enter the alias.
pub fn alias(kind: &str, key: Option<&str>) -> String {
    match key {
        Some(key) => {
            let digest = format!("{:x}", Sha256::digest(key.as_bytes()));
            format!("{kind} #{}", &digest[..12])
        }
        None => format!("{kind}（已隐藏）"),
    }
}
fn replace_known(value: &mut Option<String>, kind: &str, key: Option<&str>) {
    if value.is_some() {
        *value = Some(alias(kind, key));
    }
}
impl PrivacyRedact for PricingSummary {
    fn redact(&mut self) {
        self.redacted = true;
        for currency in &mut self.currencies {
            currency.estimated_cost = None;
        }
        self.reasons.clear();
    }
}
impl PrivacyRedact for PriceOutcome {
    fn redact(&mut self) {
        *self = Self::Redacted {};
    }
}
impl PrivacyRedact for RecentSession {
    fn redact(&mut self) {
        self.display_name = alias("会话", Some(&self.session_key));
        replace_known(
            &mut self.latest_project_name,
            "项目",
            self.latest_project_id.as_deref(),
        );
        self.pricing.redact();
    }
}
impl PrivacyRedact for DashboardBundle {
    fn redact(&mut self) {
        self.pricing.redact();
        for row in &mut self.recent_sessions {
            row.redact();
        }
    }
}
impl PrivacyRedact for GroupedUsageBundle {
    fn redact(&mut self) {
        self.redact_group(GroupDimension::Projects);
    }
    fn redact_group(&mut self, dimension: GroupDimension) {
        self.pricing.redact();
        for row in &mut self.groups {
            row.pricing.redact();
        }
        redact_group_labels(self, dimension);
    }
}
// The group DTO deliberately has no dimension, so its caller must pass that explicit context.
pub fn redact_group_labels(bundle: &mut GroupedUsageBundle, dimension: GroupDimension) {
    if matches!(dimension, GroupDimension::Projects) {
        for row in &mut bundle.groups {
            if row.key.is_some() {
                row.display_name = alias("项目", row.key.as_deref());
            }
        }
    }
}
impl PrivacyRedact for FilterOptionsPage {
    fn redact(&mut self) {
        let kind = match self.dimension {
            FacetDimension::Models => return,
            FacetDimension::Projects => "项目",
            FacetDimension::Sessions => "会话",
            FacetDimension::Sources => "来源",
        };
        for row in &mut self.options {
            if row.key.is_some() {
                row.display_name = alias(kind, row.key.as_deref());
            }
        }
    }
}
impl PrivacyRedact for SessionIdentity {
    fn redact(&mut self) {
        self.display_name = alias("会话", Some(&self.session_key));
        replace_known(
            &mut self.parent_display_name,
            "会话",
            self.parent_key.as_deref(),
        );
        replace_known(&mut self.parent_provider_id, "父会话", None);
    }
}
impl PrivacyRedact for SessionRow {
    fn redact(&mut self) {
        self.display_name = alias("会话", Some(&self.session_key));
        replace_known(
            &mut self.latest_project_name,
            "项目",
            self.latest_project_id.as_deref(),
        );
        replace_known(
            &mut self.parent_display_name,
            "会话",
            self.parent_key.as_deref(),
        );
        replace_known(&mut self.parent_provider_id, "父会话", None);
        self.pricing.redact();
    }
}
impl PrivacyRedact for SessionsPage {
    fn redact(&mut self) {
        self.pricing.redact();
        for row in &mut self.sessions {
            row.redact();
        }
    }
}
impl PrivacyRedact for SessionBundle {
    fn redact(&mut self) {
        self.identity.redact();
        self.pricing.redact();
        for row in &mut self.children {
            row.redact();
        }
        if let Some(activity) = &mut self.latest_selected_activity {
            replace_known(
                &mut activity.project_display_name,
                "项目",
                activity.project_id.as_deref(),
            );
        }
    }
}
impl PrivacyRedact for TurnsPage {
    fn redact(&mut self) {
        self.pricing.redact();
        for row in &mut self.turns {
            row.pricing.redact();
        }
    }
}
impl PrivacyRedact for UsageEventsPage {
    fn redact(&mut self) {
        self.pricing.redact();
        for row in &mut self.events {
            row.session_display_name = alias("会话", Some(&row.session_key));
            replace_known(
                &mut row.project_display_name,
                "项目",
                row.project_id.as_deref(),
            );
            row.price.redact();
        }
    }
}
impl PrivacyRedact for SourceSummary {
    fn redact(&mut self) {
        self.root_path = alias("来源", Some(&self.source_id));
    }
}
impl PrivacyRedact for SourcesSnapshot {
    fn redact(&mut self) {
        for row in &mut self.sources {
            row.redact();
        }
    }
}
impl PrivacyRedact for SourceDirectorySelection {
    fn redact(&mut self) {
        self.root_path = "所选目录（已隐藏）".into();
    }
}
impl PrivacyRedact for AppStatus {
    fn redact(&mut self) {
        self.data_directory = "应用数据目录（已隐藏）".into();
    }
}
impl PrivacyRedact for MiniSnapshot {
    fn redact(&mut self) {
        self.privacy = true;
        self.pricing.redact();
        let key = match &self.mini_scope {
            MiniScope::Session { session_key, .. } => Some(session_key.as_str()),
            MiniScope::TodayAllSources {} => None,
        };
        // All-source scope is a public label, only a fixed session's label is sensitive.
        if key.is_some() {
            replace_known(&mut self.scope_display_name, "会话", key);
        } else if self.scope_display_name.is_some() {
            self.scope_display_name = Some("全部来源 · 今日".into());
        }
        self.quota.redact();
    }
}
impl PrivacyRedact for crate::pricing::PriceRulesSnapshot {
    fn redact(&mut self) {
        self.rules.clear();
        self.aliases.clear();
    }
}
impl PrivacyRedact for ContextSnapshot {
    fn redact(&mut self) {}
}
impl PrivacyRedact for CalendarSelectionResult {
    fn redact(&mut self) {}
}
impl PrivacyRedact for crate::settings::DisplaySettingsSnapshot {
    fn redact(&mut self) {}
}
impl PrivacyRedact for QuotaSnapshot {
    fn redact(&mut self) {
        for limit in &mut self.available_limits {
            replace_known(&mut limit.display_name, "额度类型", Some(&limit.limit_id));
        }
    }
}
impl PrivacyRedact for Job {
    fn redact(&mut self) {
        if let Some(error) = &mut self.error {
            error.details.clear();
        }
    }
}
impl PrivacyRedact for crate::jobs::CancelJobResult {
    fn redact(&mut self) {}
}
impl PrivacyRedact for () {
    fn redact(&mut self) {}
}
impl<T: PrivacyRedact> PrivacyRedact for Option<T> {
    fn redact(&mut self) {
        if let Some(value) = self {
            value.redact();
        }
    }
}
impl<T: PrivacyRedact> PrivacyRedact for Vec<T> {
    fn redact(&mut self) {
        for value in self {
            value.redact();
        }
    }
}

use crate::calendar::CalendarSelectionResult;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::TryLockError;
    fn stamp(revision: i128, privacy: bool) -> DisplayPolicyStamp {
        DisplayPolicyStamp {
            settings_revision: DecimalInt::from_nonnegative(revision).unwrap(),
            privacy,
        }
    }
    #[derive(Clone)]
    struct LockProbe {
        policy: PrivacyState,
        redacted: bool,
    }
    impl PrivacyRedact for LockProbe {
        fn redact(&mut self) {
            self.redacted = true;
        }
    }
    impl Serialize for LockProbe {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            assert!(matches!(
                self.policy.0.try_lock(),
                Err(TryLockError::WouldBlock)
            ));
            self.redacted.serialize(serializer)
        }
    }
    #[test]
    fn payload_and_stamp_are_serialized_while_policy_publication_is_locked() {
        let policy = PrivacyState::new(stamp(1, true));
        let response = PrivateResponse::new(
            "r".into(),
            LockProbe {
                policy: policy.clone(),
                redacted: false,
            },
            policy,
        );
        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value["data"], true);
        assert_eq!(value["display_policy"]["privacy"], true);
    }
    #[test]
    fn commit_guard_preserves_failed_transaction_and_stops_impossible_committed_policy() {
        let policy = PrivacyState::new(stamp(1, false));
        assert_eq!(
            policy.commit_update(|| Err::<((), DisplayPolicyStamp), _>(
                crate::error::ErrorCode::RevisionConflict
            )),
            Err(crate::error::ErrorCode::RevisionConflict)
        );
        assert!(!policy.current().unwrap().privacy);
        policy
            .commit_update(|| Ok::<_, crate::error::ErrorCode>(((), stamp(2, true))))
            .unwrap();
        assert!(policy.current().unwrap().privacy);
        assert_eq!(
            policy.commit_update(|| Ok::<_, crate::error::ErrorCode>(((), stamp(1, false)))),
            Err(crate::error::ErrorCode::RevisionConflict)
        );
        assert_eq!(
            policy.current().unwrap_err(),
            crate::error::ErrorCode::DbCorrupt
        );
        let response = PrivateResponse::new(
            "r".into(),
            Some(SourceDirectorySelection {
                selection_handle: "opaque".into(),
                root_path: "SECRET".into(),
                origin: SourceOrigin::Custom,
            }),
            policy,
        );
        assert_eq!(
            serde_json::to_string(&response).unwrap_err().to_string(),
            "DISPLAY_POLICY_UNAVAILABLE"
        );
    }
}
