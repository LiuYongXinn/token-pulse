//! Bounded basic diagnostics; no raw samples, history or lineage evidence in public DTOs.
use crate::{error::ErrorCode, numeric::DecimalInt, protocol::validate_request_id};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticsRequest {
    pub source_id: Option<String>,
}
impl DiagnosticsRequest {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if let Some(id) = &self.source_id {
            validate_request_id(id)?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticKind {
    LogRecord,
    UnconfirmedUsage,
    UnattributedUsage,
    MissingFile,
    DirectoryScan,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticIssue {
    pub issue_id: String,
    pub source_id: Option<String>,
    pub kind: DiagnosticKind,
    pub code: Option<ErrorCode>,
    pub path: Option<String>,
    pub byte_offset: Option<DecimalInt>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticsSnapshot {
    pub data_revision: DecimalInt,
    pub issues: Vec<DiagnosticIssue>,
    pub has_more: bool,
}

impl crate::privacy::PrivacyRedact for DiagnosticsSnapshot {
    fn redact(&mut self) {
        for issue in &mut self.issues {
            if issue.path.is_some() {
                issue.path = Some("位置已隐藏".into());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privacy::{DisplayPolicyStamp, PrivacyState, PrivateResponse};
    #[test]
    fn late_privacy_hides_known_paths_and_keeps_unknown_positions_and_exact_offsets() {
        let stamp = |revision, privacy| DisplayPolicyStamp {
            settings_revision: DecimalInt::from_nonnegative(revision).unwrap(),
            privacy,
        };
        let policy = PrivacyState::new(stamp(1, false));
        let value = DiagnosticsSnapshot {
            data_revision: DecimalInt::from_nonnegative(9007199254740993).unwrap(),
            issues: vec![
                DiagnosticIssue {
                    issue_id: "opaque".into(),
                    source_id: None,
                    kind: DiagnosticKind::LogRecord,
                    code: Some(ErrorCode::UnsupportedFormat),
                    path: Some("private-source.jsonl".into()),
                    byte_offset: Some(DecimalInt::from_nonnegative(9007199254740993).unwrap()),
                },
                DiagnosticIssue {
                    issue_id: "unknown".into(),
                    source_id: None,
                    kind: DiagnosticKind::DirectoryScan,
                    code: None,
                    path: None,
                    byte_offset: None,
                },
            ],
            has_more: false,
        };
        let response = PrivateResponse::new("query".into(), value, policy.clone());
        policy.publish(stamp(2, true)).unwrap();
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["data"]["issues"][0]["path"], "位置已隐藏");
        assert_eq!(json["data"]["issues"][0]["byte_offset"], "9007199254740993");
        assert!(
            json["data"]["issues"][1]["path"].is_null()
                && json["data"]["issues"][1]["byte_offset"].is_null()
        );
        assert!(!json.to_string().contains("private-source"));
        policy.publish(stamp(3, false)).unwrap();
        assert_eq!(
            serde_json::to_value(&response).unwrap()["data"]["issues"][0]["path"],
            "private-source.jsonl"
        );
    }
    #[test]
    fn request_ids_are_bounded_and_raw_samples_are_not_dto_fields() {
        assert!(
            DiagnosticsRequest {
                source_id: Some("\0".into())
            }
            .validate()
            .is_err()
        );
        assert!(
            serde_json::from_str::<DiagnosticsRequest>(r#"{"source_id":null,"sample":true}"#)
                .is_err()
        );
        assert!(serde_json::from_str::<DiagnosticIssue>(r#"{"issue_id":"id","source_id":null,"kind":"log_record","code":null,"path":null,"byte_offset":null,"raw":"body"}"#).is_err());
    }
}
