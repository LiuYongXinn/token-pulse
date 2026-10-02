//! Shared native-host wire contract and fail-closed receiver. No Tauri, database or log access.
pub mod display;
#[cfg(windows)]
pub mod windows;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use token_pulse_core::{
    mini::MiniUsageSnapshot,
    numeric::{DecimalInt, DecimalMoney, EpochMs},
    protocol::{CoverageState, QuotaSnapshot, QuotaState, QuotaWindow, validate_request_id},
};

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 65_536;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HostConfiguration {
    pub settings_revision: DecimalInt,
    pub enabled: bool,
    pub display: display::DisplayPreferences,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HostDisplayState {
    Disabled,
    WaitingSnapshot,
    Embedded,
    Unavailable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HostFailure {
    Os,
    UnsupportedVersion,
    MissingTaskbar,
    UnexpectedStructure,
    UnsafeGeometry,
    InsufficientSpace,
    BackgroundUnavailable,
    CleanupTimeout,
    GuardianUnavailable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HostRestore {
    NoRecord,
    Restored,
    AlreadyRestored,
    ExternalChange,
    IdentityLost,
    Failed,
    Uncertain,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HostStatus {
    pub settings_revision: Option<DecimalInt>,
    pub system_revision: DecimalInt,
    pub state: HostDisplayState,
    pub failure: Option<HostFailure>,
    pub density: Option<display::Density>,
    pub last_restore: Option<HostRestore>,
}
impl HostStatus {
    pub fn validate(&self) -> Result<(), WireError> {
        if (self.state == HostDisplayState::Unavailable) != self.failure.is_some()
            || (self.state == HostDisplayState::Embedded) != self.density.is_some()
            || (self.settings_revision.is_none() && self.state != HostDisplayState::Disabled)
        {
            return Err(WireError::InvalidFrame);
        }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Envelope<T> {
    #[schemars(range(min = 1, max = 1))]
    pub protocol_version: u32,
    #[schemars(length(min = 1, max = 128))]
    pub host_instance_id: String,
    #[schemars(length(min = 64, max = 64))]
    pub nonce: String,
    pub sequence: DecimalInt,
    pub body: T,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HostMessage {
    Hello {},
    Snapshot {
        view: Box<TaskbarView>,
    },
    Privacy {
        settings_revision: DecimalInt,
        enabled: bool,
    },
    Heartbeat {},
    Configure {
        configuration: HostConfiguration,
    },
    GetStatus {},
    Shutdown {},
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HostAction {
    OpenFloat {},
    OpenStats {},
    OpenTaskbarSettings {},
    SetPrivacy { enabled: bool },
    DisableTaskbar {},
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HostReply {
    Ready {},
    Heartbeat {},
    PrivacyApplied { enabled: bool },
    Stopped {},
    Action { action: HostAction },
    Configured { settings_revision: DecimalInt },
    Status { status: HostStatus },
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HostCost {
    pub currency: String,
    pub estimated_cost: Option<DecimalMoney>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HostQuota {
    pub connection_epoch: String,
    pub revision: DecimalInt,
    pub state: QuotaState,
    pub limit_label: Option<String>,
    pub fetched_at_ms: Option<EpochMs>,
    pub windows: Vec<QuotaWindow>,
}
/// Display-only projection: no paths, SQL, credentials, conversation text or executable arguments.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskbarView {
    pub settings_revision: DecimalInt,
    pub usage_revision: DecimalInt,
    pub price_revision: DecimalInt,
    pub generated_at_ms: EpochMs,
    pub privacy: bool,
    pub scope_label: Option<String>,
    pub timezone: String,
    pub total_tokens: Option<DecimalInt>,
    pub input_tokens: Option<DecimalInt>,
    pub cached_tokens: Option<DecimalInt>,
    pub output_tokens: Option<DecimalInt>,
    pub usage_status: CoverageState,
    pub costs: Vec<HostCost>,
    pub priced_tokens: DecimalInt,
    pub unpriced_tokens: DecimalInt,
    pub quota: Option<HostQuota>,
}
impl TaskbarView {
    pub fn from_snapshots(usage: &MiniUsageSnapshot, quota: &QuotaSnapshot, privacy: bool) -> Self {
        let known = usage.usage.usage_event_count.as_str() != "0"
            || matches!(usage.coverage.state, CoverageState::Complete);
        let label = quota
            .available_limits
            .iter()
            .find(|q| Some(&q.limit_id) == quota.selected_limit_id.as_ref())
            .map(|q| q.display_name.as_ref().unwrap_or(&q.limit_id).clone());
        Self {
            settings_revision: usage.settings_revision.clone(),
            usage_revision: usage.meta.data_revision.clone(),
            price_revision: usage.meta.price_revision.clone(),
            generated_at_ms: usage.meta.generated_at_ms,
            privacy,
            scope_label: (!privacy)
                .then(|| usage.scope_display_name.clone())
                .flatten(),
            timezone: usage.range.timezone.clone(),
            total_tokens: known.then(|| usage.usage.total_tokens.clone()),
            input_tokens: usage.usage.input_total.value.clone(),
            cached_tokens: usage.usage.cached_input.value.clone(),
            output_tokens: usage.usage.output_total.value.clone(),
            usage_status: usage.coverage.state,
            priced_tokens: usage.pricing.priced_total_tokens.clone(),
            unpriced_tokens: usage.pricing.unpriced_total_tokens.clone(),
            costs: if privacy {
                vec![]
            } else {
                usage
                    .pricing
                    .currencies
                    .iter()
                    .map(|c| HostCost {
                        currency: c.currency.clone(),
                        estimated_cost: c.estimated_cost.clone(),
                    })
                    .collect()
            },
            quota: (!privacy).then(|| HostQuota {
                connection_epoch: quota.connection_epoch.clone(),
                revision: quota.quota_revision.clone(),
                state: quota.state,
                limit_label: label,
                fetched_at_ms: quota.fetched_at_ms,
                windows: quota.windows.clone(),
            }),
        }
    }
    pub fn validate(&self) -> Result<(), WireError> {
        if !text(&self.timezone, 128)
            || self.scope_label.as_ref().is_some_and(|s| !text(s, 256))
            || self.costs.len() > 16
        {
            return Err(WireError::InvalidFrame);
        }
        if self.privacy
            && (!self.costs.is_empty() || self.quota.is_some() || self.scope_label.is_some())
        {
            return Err(WireError::PrivacyViolation);
        }
        for (index, cost) in self.costs.iter().enumerate() {
            if cost.currency.len() != 3
                || !cost.currency.bytes().all(|c| c.is_ascii_uppercase())
                || self.costs[..index]
                    .iter()
                    .any(|earlier| earlier.currency == cost.currency)
            {
                return Err(WireError::InvalidFrame);
            }
        }
        if let Some(quota) = &self.quota {
            if validate_request_id(&quota.connection_epoch).is_err()
                || quota.windows.len() > 16
                || quota.limit_label.as_ref().is_some_and(|s| !text(s, 256))
            {
                return Err(WireError::InvalidFrame);
            }
            for (index, window) in quota.windows.iter().enumerate() {
                if !text(&window.window_id, 128)
                    || quota.windows[..index]
                        .iter()
                        .any(|w| w.window_id == window.window_id)
                    || window.duration_mins == Some(0)
                    || window.used_percent.is_some_and(|v| !v.is_finite())
                    || window
                        .remaining_percent
                        .is_some_and(|v| !v.is_finite() || !(0.0..=100.0).contains(&v))
                {
                    return Err(WireError::InvalidFrame);
                }
            }
        }
        Ok(())
    }
}
fn text(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.chars().count() <= maximum && !value.chars().any(char::is_control)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireError {
    Io(std::io::ErrorKind),
    InvalidFrame,
    TooLarge,
    WrongChannel,
    OutOfOrder,
    InvalidState,
    PrivacyViolation,
    Closed,
}
pub fn read_frame<R: Read, T: for<'de> Deserialize<'de>>(
    reader: &mut R,
) -> Result<Option<Envelope<T>>, WireError> {
    let mut prefix = [0; 4];
    // Distinguish a clean channel close from a truncated prefix/body.
    loop {
        match reader.read(&mut prefix[..1]) {
            Ok(0) => return Ok(None),
            Ok(_) => break,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(WireError::Io(e.kind())),
        }
    }
    reader
        .read_exact(&mut prefix[1..])
        .map_err(|e| WireError::Io(e.kind()))?;
    let length = u32::from_le_bytes(prefix) as usize;
    if length == 0 {
        return Err(WireError::InvalidFrame);
    }
    if length > MAX_FRAME_BYTES {
        return Err(WireError::TooLarge);
    }
    let mut body = vec![0; length];
    reader
        .read_exact(&mut body)
        .map_err(|e| WireError::Io(e.kind()))?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|_| WireError::InvalidFrame)
}
pub fn write_frame<W: Write, T: Serialize>(
    writer: &mut W,
    frame: &Envelope<T>,
) -> Result<(), WireError> {
    // Reject while serializing, before allocating an unbounded JSON buffer or writing a prefix.
    let mut encoded = BoundedBuffer {
        bytes: Vec::new(),
        exceeded: false,
    };
    serde_json::to_writer(&mut encoded, frame).map_err(|_| {
        if encoded.exceeded {
            WireError::TooLarge
        } else {
            WireError::InvalidFrame
        }
    })?;
    let body = encoded.bytes;
    writer
        .write_all(&(body.len() as u32).to_le_bytes())
        .and_then(|_| writer.write_all(&body))
        .and_then(|_| writer.flush())
        .map_err(|e| WireError::Io(e.kind()))
}
struct BoundedBuffer {
    bytes: Vec<u8>,
    exceeded: bool,
}
impl Write for BoundedBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_FRAME_BYTES - self.bytes.len() {
            self.exceeded = true;
            return Err(std::io::Error::other("host frame exceeds maximum"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Receiver owned by the native host. Transport authentication/ACL is a separate mandatory layer.
pub struct HostSession {
    instance: String,
    nonce: String,
    sequence: i128,
    ready: bool,
    closed: bool,
    privacy: bool,
    policy_revision: i128,
    privacy_revision: i128,
    view: Option<TaskbarView>,
    configuration: Option<HostConfiguration>,
}
impl HostSession {
    pub fn new(instance: String, nonce: String) -> Result<Self, WireError> {
        if validate_request_id(&instance).is_err()
            || nonce.len() != 64
            || !nonce.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err(WireError::InvalidFrame);
        }
        Ok(Self {
            instance,
            nonce,
            sequence: 0,
            ready: false,
            closed: false,
            privacy: true,
            policy_revision: 0,
            privacy_revision: 0,
            view: None,
            configuration: None,
        })
    }
    pub fn view(&self) -> Option<&TaskbarView> {
        self.view.as_ref()
    }
    pub fn configuration(&self) -> Option<&HostConfiguration> {
        self.configuration.as_ref()
    }
    pub fn close(&mut self) {
        self.closed = true;
        self.view = None;
        self.configuration = None;
    }
    pub fn apply(&mut self, frame: Envelope<HostMessage>) -> Result<HostReply, WireError> {
        let result = self.apply_checked(frame);
        if result.is_err() {
            self.close();
        }
        result
    }
    fn apply_checked(&mut self, frame: Envelope<HostMessage>) -> Result<HostReply, WireError> {
        if self.closed {
            return Err(WireError::Closed);
        }
        if frame.protocol_version != PROTOCOL_VERSION
            || frame.host_instance_id != self.instance
            || frame.nonce != self.nonce
        {
            return Err(WireError::WrongChannel);
        }
        if frame.sequence.value() <= self.sequence {
            return Err(WireError::OutOfOrder);
        }
        self.sequence = frame.sequence.value();
        if !self.ready {
            if !matches!(frame.body, HostMessage::Hello {}) {
                return Err(WireError::InvalidState);
            }
            self.ready = true;
            return Ok(HostReply::Ready {});
        }
        match frame.body {
            HostMessage::Hello {} => Err(WireError::InvalidState),
            HostMessage::Heartbeat {} => Ok(HostReply::Heartbeat {}),
            HostMessage::GetStatus {} => Ok(HostReply::Heartbeat {}),
            // Session validates the request; transport must obtain an actual UI receipt for status.
            HostMessage::Configure { configuration } => {
                configuration.display.validate()?;
                if configuration.settings_revision.value() < self.policy_revision
                    || self.configuration.as_ref().is_some_and(|old| {
                        configuration.settings_revision.value() < old.settings_revision.value()
                            || (configuration.settings_revision == old.settings_revision
                                && configuration != *old)
                    })
                {
                    return Err(WireError::OutOfOrder);
                }
                self.policy_revision = configuration.settings_revision.value();
                if self
                    .view
                    .as_ref()
                    .is_some_and(|v| v.settings_revision.value() < self.policy_revision)
                {
                    self.view = None;
                }
                let revision = configuration.settings_revision.clone();
                self.configuration = Some(configuration);
                Ok(HostReply::Configured {
                    settings_revision: revision,
                })
            }
            HostMessage::Shutdown {} => {
                self.close();
                Ok(HostReply::Stopped {})
            }
            HostMessage::Privacy {
                settings_revision,
                enabled,
            } => {
                if settings_revision.value() < self.policy_revision
                    || (settings_revision.value() == self.privacy_revision
                        && self.privacy != enabled)
                {
                    return Err(WireError::OutOfOrder);
                }
                self.policy_revision = settings_revision.value();
                self.privacy_revision = settings_revision.value();
                self.privacy = enabled;
                self.view = None;
                Ok(HostReply::PrivacyApplied { enabled })
            }
            HostMessage::Snapshot { view } => {
                view.validate()?;
                if view.settings_revision.value() < self.policy_revision
                    || view.privacy != self.privacy
                {
                    return Err(WireError::PrivacyViolation);
                }
                self.policy_revision = view.settings_revision.value();
                self.privacy_revision = view.settings_revision.value();
                self.view = Some(*view);
                // Receiving display data alone never claims Explorer embedding succeeded.
                Ok(HostReply::Heartbeat {})
            }
        }
    }
}
