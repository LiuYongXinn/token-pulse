//! Update contracts and operation ordering. Network, signatures and installers belong to
//! the native provider; the renderer cannot supply an artifact, key, URL or verification result.
use crate::{
    error::ErrorCode,
    numeric::{DecimalInt, EpochMs},
    privacy::PrivacyRedact,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePhase {
    Unavailable,
    Idle,
    Checking,
    Current,
    Available,
    Downloading,
    Verifying,
    ReadyToInstall,
    Installing,
    Error,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum UpdateIssue {
    PublicationNotConfigured,
    UnsupportedPlatform,
    Network,
    InvalidRelease,
    DownloadFailed,
    SignatureInvalid,
    InstallerUnavailable,
    InstallFailed,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct UpdateRelease {
    pub version: String,
    pub notes: Option<String>,
    pub published_at_ms: Option<EpochMs>,
}
impl UpdateRelease {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validate_version(&self.version)?;
        if self.notes.as_ref().is_some_and(|notes| {
            notes.len() > 32768
                || notes
                    .chars()
                    .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
        }) {
            return Err(ErrorCode::InvalidQuery);
        }
        Ok(())
    }
}
// The provider performs SemVer comparison. This boundary additionally limits public text;
// no filenames or command arguments are ever constructed from the displayed version.
fn validate_version(version: &str) -> Result<(), ErrorCode> {
    if version.is_empty()
        || version.len() > 128
        || !version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+'))
    {
        return Err(ErrorCode::InvalidQuery);
    }
    Ok(())
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct UpdateSnapshot {
    pub update_revision: DecimalInt,
    pub phase: UpdatePhase,
    pub current_version: String,
    pub release: Option<UpdateRelease>,
    pub last_checked_at_ms: Option<EpochMs>,
    pub downloaded_bytes: Option<DecimalInt>,
    pub total_bytes: Option<DecimalInt>,
    pub issue: Option<UpdateIssue>,
}
// Only bounded public release metadata; no local paths, account values, keys or raw errors.
impl PrivacyRedact for UpdateSnapshot {
    fn redact(&mut self) {}
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct UpdateActionRequest {
    pub expected_update_revision: DecimalInt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OperationKind {
    Check,
    Download,
    Install,
}
/// Native-only capability. Cannot be deserialized or constructed by the frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateOperation {
    generation: i128,
    kind: OperationKind,
}
pub struct UpdateWorkflow {
    snapshot: UpdateSnapshot,
    operation: Option<UpdateOperation>,
}
impl UpdateWorkflow {
    pub fn new(
        current_version: String,
        unavailable: Option<UpdateIssue>,
    ) -> Result<Self, ErrorCode> {
        validate_version(&current_version)?;
        if unavailable.is_some_and(|issue| {
            !matches!(
                issue,
                UpdateIssue::PublicationNotConfigured | UpdateIssue::UnsupportedPlatform
            )
        }) {
            return Err(ErrorCode::InvalidQuery);
        }
        Ok(Self {
            snapshot: UpdateSnapshot {
                update_revision: DecimalInt::parse("0")?,
                phase: if unavailable.is_some() {
                    UpdatePhase::Unavailable
                } else {
                    UpdatePhase::Idle
                },
                current_version,
                release: None,
                last_checked_at_ms: None,
                downloaded_bytes: None,
                total_bytes: None,
                issue: unavailable,
            },
            operation: None,
        })
    }
    pub fn snapshot(&self) -> UpdateSnapshot {
        self.snapshot.clone()
    }
    fn next_revision(&self) -> Result<DecimalInt, ErrorCode> {
        DecimalInt::from_nonnegative(
            self.snapshot
                .update_revision
                .value()
                .checked_add(1)
                .ok_or(ErrorCode::NumericOverflow)?,
        )
    }
    fn check_available(&self) -> Result<(), ErrorCode> {
        if self.snapshot.phase == UpdatePhase::Unavailable {
            return Err(ErrorCode::UpdateUnavailable);
        }
        if self.operation.is_some() {
            return Err(ErrorCode::UpdateBusy);
        }
        Ok(())
    }
    fn begin(
        &mut self,
        kind: OperationKind,
        phase: UpdatePhase,
    ) -> Result<UpdateOperation, ErrorCode> {
        let revision = self.next_revision()?;
        let operation = UpdateOperation {
            generation: revision.value(),
            kind,
        };
        self.snapshot.update_revision = revision;
        self.snapshot.phase = phase;
        self.snapshot.issue = None;
        self.operation = Some(operation);
        Ok(operation)
    }
    fn require(&self, operation: UpdateOperation, kind: OperationKind) -> Result<(), ErrorCode> {
        if self.operation != Some(operation) || operation.kind != kind {
            return Err(ErrorCode::CandidateObsolete);
        }
        Ok(())
    }
    fn compare_revision(&self, expected: &DecimalInt) -> Result<(), ErrorCode> {
        if &self.snapshot.update_revision != expected {
            return Err(ErrorCode::RevisionConflict);
        }
        Ok(())
    }
    pub fn begin_check(&mut self) -> Result<UpdateOperation, ErrorCode> {
        self.check_available()?;
        let operation = self.begin(OperationKind::Check, UpdatePhase::Checking)?;
        // A new check invalidates the old offer and any verified download in the owner.
        self.snapshot.release = None;
        self.snapshot.downloaded_bytes = None;
        self.snapshot.total_bytes = None;
        Ok(operation)
    }
    pub fn checked(
        &mut self,
        operation: UpdateOperation,
        release: Option<UpdateRelease>,
        at: EpochMs,
    ) -> Result<(), ErrorCode> {
        self.require(operation, OperationKind::Check)?;
        if let Some(release) = &release {
            release.validate()?;
        }
        let revision = self.next_revision()?;
        self.snapshot.phase = if release.is_some() {
            UpdatePhase::Available
        } else {
            UpdatePhase::Current
        };
        self.snapshot.release = release;
        self.snapshot.last_checked_at_ms = Some(at);
        self.snapshot.update_revision = revision;
        self.operation = None;
        Ok(())
    }
    pub fn begin_download(&mut self, expected: &DecimalInt) -> Result<UpdateOperation, ErrorCode> {
        self.compare_revision(expected)?;
        self.check_available()?;
        if self.snapshot.phase != UpdatePhase::Available || self.snapshot.release.is_none() {
            return Err(ErrorCode::InvalidQuery);
        }
        let operation = self.begin(OperationKind::Download, UpdatePhase::Downloading)?;
        self.snapshot.downloaded_bytes = Some(DecimalInt::parse("0")?);
        self.snapshot.total_bytes = None;
        Ok(operation)
    }
    pub fn progress(
        &mut self,
        operation: UpdateOperation,
        chunk_bytes: u64,
        total_bytes: Option<u64>,
    ) -> Result<(), ErrorCode> {
        self.require(operation, OperationKind::Download)?;
        if self.snapshot.phase != UpdatePhase::Downloading {
            return Err(ErrorCode::CandidateObsolete);
        }
        let next = self
            .snapshot
            .downloaded_bytes
            .as_ref()
            .ok_or(ErrorCode::InvalidUsage)?
            .value()
            .checked_add(i128::from(chunk_bytes))
            .ok_or(ErrorCode::NumericOverflow)?;
        let total = total_bytes
            .map(i128::from)
            .or_else(|| self.snapshot.total_bytes.as_ref().map(DecimalInt::value));
        if self
            .snapshot
            .total_bytes
            .as_ref()
            .is_some_and(|known| total.is_some_and(|total| total != known.value()))
            || total.is_some_and(|total| next > total)
        {
            return Err(ErrorCode::InvalidUsage);
        }
        let revision = self.next_revision()?;
        self.snapshot.downloaded_bytes = Some(DecimalInt::from_nonnegative(next)?);
        self.snapshot.total_bytes = total.map(DecimalInt::from_nonnegative).transpose()?;
        self.snapshot.update_revision = revision;
        Ok(())
    }
    pub fn begin_verification(&mut self, operation: UpdateOperation) -> Result<(), ErrorCode> {
        self.require(operation, OperationKind::Download)?;
        if self.snapshot.phase != UpdatePhase::Downloading {
            return Err(ErrorCode::CandidateObsolete);
        }
        let revision = self.next_revision()?;
        self.snapshot.phase = UpdatePhase::Verifying;
        self.snapshot.update_revision = revision;
        Ok(())
    }
    /// Only call after the native provider has returned successfully from signature and
    /// signed-version verification; network EOF alone does not authorize installation.
    pub fn verified(
        &mut self,
        operation: UpdateOperation,
        artifact_bytes: u64,
    ) -> Result<(), ErrorCode> {
        self.require(operation, OperationKind::Download)?;
        if self.snapshot.phase != UpdatePhase::Verifying {
            return Err(ErrorCode::CandidateObsolete);
        }
        let bytes = i128::from(artifact_bytes);
        if bytes == 0
            || self
                .snapshot
                .downloaded_bytes
                .as_ref()
                .map(DecimalInt::value)
                != Some(bytes)
            || self
                .snapshot
                .total_bytes
                .as_ref()
                .is_some_and(|total| total.value() != bytes)
        {
            return Err(ErrorCode::InvalidUsage);
        }
        let revision = self.next_revision()?;
        self.snapshot.phase = UpdatePhase::ReadyToInstall;
        self.snapshot.update_revision = revision;
        self.operation = None;
        Ok(())
    }
    pub fn begin_install(&mut self, expected: &DecimalInt) -> Result<UpdateOperation, ErrorCode> {
        self.compare_revision(expected)?;
        self.check_available()?;
        if self.snapshot.phase != UpdatePhase::ReadyToInstall {
            return Err(ErrorCode::InvalidQuery);
        }
        self.begin(OperationKind::Install, UpdatePhase::Installing)
    }
    pub fn failed(
        &mut self,
        operation: UpdateOperation,
        issue: UpdateIssue,
    ) -> Result<(), ErrorCode> {
        if self.operation != Some(operation) {
            return Err(ErrorCode::CandidateObsolete);
        }
        if matches!(
            issue,
            UpdateIssue::PublicationNotConfigured | UpdateIssue::UnsupportedPlatform
        ) {
            return Err(ErrorCode::InvalidQuery);
        }
        let revision = self.next_revision()?;
        self.snapshot.phase = UpdatePhase::Error;
        self.snapshot.issue = Some(issue);
        self.snapshot.update_revision = revision;
        self.operation = None;
        Ok(())
    }
}
