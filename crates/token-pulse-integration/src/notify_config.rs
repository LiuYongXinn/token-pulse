//! Pure byte-preserving root `notify` plans. The Windows adapter applies bounded file plans;
//! these pure functions never write files or execute commands.
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use std::{fmt, ops::Range, path::Path};

#[cfg(windows)]
pub mod windows;

pub const MAX_CONFIG_BYTES: usize = 1024 * 1024;
const MAX_ARGUMENTS: usize = 64;
const MAX_ARGUMENT_BYTES: usize = 16 * 1024;
const MAX_COMMAND_BYTES: usize = 64 * 1024;
const NOTIFY_FLAG: &str = "--tokenpulse-notify";
const RECORD_VERSION: u32 = 1;

/// Errors omit configuration text, parser excerpts, paths and arguments.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigError {
    TooLarge,
    InvalidToml,
    InvalidNotify,
    InvalidManagedCommand,
    AlreadyManaged,
    StalePlan,
    OwnershipConflict,
    InvalidRestoreRecord,
}
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooLarge => "notify_config_too_large",
            Self::InvalidToml => "notify_config_invalid_toml",
            Self::InvalidNotify => "notify_config_invalid_command",
            Self::InvalidManagedCommand => "notify_config_invalid_managed_command",
            Self::AlreadyManaged => "notify_config_already_managed",
            Self::StalePlan => "notify_config_stale_plan",
            Self::OwnershipConflict => "notify_config_ownership_conflict",
            Self::InvalidRestoreRecord => "notify_config_invalid_restore_record",
        })
    }
}
impl std::error::Error for ConfigError {}

/// Construct from the application's current executable, never frontend command text.
pub struct ManagedNotifyCommand {
    arguments: Vec<String>,
}
impl ManagedNotifyCommand {
    pub fn new(executable: &Path, registration_id: &str) -> Result<Self, ConfigError> {
        let text = executable
            .to_str()
            .ok_or(ConfigError::InvalidManagedCommand)?;
        if !executable.is_absolute()
            || text.chars().any(char::is_control)
            || text.len() > MAX_ARGUMENT_BYTES
            || registration_id.len() != 32
            || !registration_id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(ConfigError::InvalidManagedCommand);
        }
        #[cfg(windows)]
        if !executable
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
        {
            return Err(ConfigError::InvalidManagedCommand);
        }
        Ok(Self {
            arguments: vec![
                text.to_owned(),
                NOTIFY_FLAG.to_owned(),
                "--integration".to_owned(),
                registration_id.to_owned(),
            ],
        })
    }
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }
}

/// Persists only old notify value and owned command, never a full config snapshot.
/// Deserialization validates syntax and ownership before the record can be used.
#[derive(Clone, Serialize)]
pub struct NotifyRestoreRecord {
    version: u32,
    installed_arguments: Vec<String>,
    original_value: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordWire {
    version: u32,
    installed_arguments: Vec<String>,
    #[serde(deserialize_with = "deserialize_original_value")]
    original_value: Option<String>,
}
fn deserialize_original_value<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer)
}
impl<'de> Deserialize<'de> for NotifyRestoreRecord {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = RecordWire::deserialize(deserializer)?;
        let record = Self {
            version: wire.version,
            installed_arguments: wire.installed_arguments,
            original_value: wire.original_value,
        };
        record
            .validate()
            .map_err(|_| serde::de::Error::custom("notify_config_invalid_restore_record"))?;
        Ok(record)
    }
}
impl NotifyRestoreRecord {
    fn validate(&self) -> Result<(), ConfigError> {
        let invalid = ConfigError::InvalidRestoreRecord;
        if self.version != RECORD_VERSION || self.installed_arguments.len() != 4 {
            return Err(invalid);
        }
        let expected = ManagedNotifyCommand::new(
            Path::new(&self.installed_arguments[0]),
            &self.installed_arguments[3],
        )
        .map_err(|_| invalid)?;
        if self.installed_arguments != expected.arguments {
            return Err(invalid);
        }
        if let Some(raw) = &self.original_value {
            let source = format!("notify = {raw}");
            let parsed = parse(&source).map_err(|_| invalid)?;
            let field = parsed.field.ok_or(invalid)?;
            // Saved values cannot inject other keys, tables or comments outside the value.
            if parsed.root_len != 1 || &source[field.value] != raw || is_managed(&field.arguments) {
                return Err(invalid);
            }
        }
        Ok(())
    }
    pub fn original_value(&self) -> Option<&str> {
        self.original_value.as_deref()
    }
    pub fn installed_arguments(&self) -> &[String] {
        &self.installed_arguments
    }
    /// For an explicitly authorized original-command chain; never executed by this planner.
    pub fn original_arguments(&self) -> Result<Option<Vec<String>>, ConfigError> {
        self.original_value
            .as_ref()
            .map(|raw| {
                parse(&format!("notify = {raw}"))?
                    .field
                    .map(|f| f.arguments)
                    .ok_or(ConfigError::InvalidRestoreRecord)
            })
            .transpose()
    }
}

/// Full edited bytes exist only in memory. Intentionally no Debug or Serialize implementation.
pub struct NotifyEditPlan {
    expected_digest: [u8; 32],
    edited: Vec<u8>,
    record: NotifyRestoreRecord,
    next_value: String,
}
impl NotifyEditPlan {
    pub fn restore_record(&self) -> &NotifyRestoreRecord {
        &self.record
    }
    /// Preview only notify values; unrelated settings never reach diff text.
    pub fn next_value(&self) -> &str {
        &self.next_value
    }
    /// Revalidate exact bytes immediately before a future controlled filesystem replacement.
    pub fn apply_to(&self, current: &[u8]) -> Result<Vec<u8>, ConfigError> {
        if digest(current) != self.expected_digest {
            return Err(ConfigError::StalePlan);
        }
        Ok(self.edited.clone())
    }
}

pub fn prepare_enable(
    current: &[u8],
    command: &ManagedNotifyCommand,
) -> Result<NotifyEditPlan, ConfigError> {
    let source = config_text(current)?;
    let parsed = parse(source)?;
    let next_value = render_arguments(command.arguments());
    let (edited, original_value) = match parsed.field {
        Some(field) => {
            if is_managed(&field.arguments) {
                return Err(ConfigError::AlreadyManaged);
            }
            let original = source[field.value.clone()].to_owned();
            (replace(source, field.value, &next_value), Some(original))
        }
        None => {
            // Insert before tables; nested profile notify values are never changed.
            let newline = if source.contains("\r\n") {
                "\r\n"
            } else {
                "\n"
            };
            let assignment = format!("notify = {next_value}{newline}");
            (
                replace(source, parsed.bom_len..parsed.bom_len, &assignment),
                None,
            )
        }
    };
    if edited.len() > MAX_CONFIG_BYTES {
        return Err(ConfigError::TooLarge);
    }
    Ok(NotifyEditPlan {
        expected_digest: digest(current),
        edited: edited.into_bytes(),
        record: NotifyRestoreRecord {
            version: RECORD_VERSION,
            installed_arguments: command.arguments.clone(),
            original_value,
        },
        next_value,
    })
}

/// Undo only while the latest notify command still exactly belongs to this registration.
/// Unrelated edits are retained; altered or removed commands cause a conflict.
pub fn restore(current: &[u8], record: &NotifyRestoreRecord) -> Result<Vec<u8>, ConfigError> {
    record.validate()?;
    let source = config_text(current)?;
    let parsed = parse(source)?;
    let field = parsed.field.ok_or(ConfigError::OwnershipConflict)?;
    if field.arguments != record.installed_arguments {
        return Err(ConfigError::OwnershipConflict);
    }
    let restored = if let Some(original) = &record.original_value {
        replace(source, field.value, original)
    } else {
        let range = removable_assignment(source, field.key.start..field.value.end, parsed.bom_len);
        replace(source, range, "")
    };
    if restored.len() > MAX_CONFIG_BYTES {
        return Err(ConfigError::TooLarge);
    }
    Ok(restored.into_bytes())
}

/// A prepared registry entry is not active until the current config contains its exact command.
pub fn owns_current_notify(
    current: &[u8],
    record: &NotifyRestoreRecord,
) -> Result<bool, ConfigError> {
    record.validate()?;
    let parsed = parse(config_text(current)?)?;
    Ok(parsed
        .field
        .is_some_and(|field| field.arguments == record.installed_arguments))
}

struct ParsedConfig {
    bom_len: usize,
    root_len: usize,
    field: Option<NotifyField>,
}
struct NotifyField {
    key: Range<usize>,
    value: Range<usize>,
    arguments: Vec<String>,
}
fn config_text(bytes: &[u8]) -> Result<&str, ConfigError> {
    if bytes.len() > MAX_CONFIG_BYTES {
        return Err(ConfigError::TooLarge);
    }
    std::str::from_utf8(bytes).map_err(|_| ConfigError::InvalidToml)
}
fn parse(source: &str) -> Result<ParsedConfig, ConfigError> {
    if source.len() > MAX_CONFIG_BYTES {
        return Err(ConfigError::TooLarge);
    }
    let bom_len = if source.starts_with('\u{feff}') { 3 } else { 0 };
    let doc =
        toml_edit::Document::parse(&source[bom_len..]).map_err(|_| ConfigError::InvalidToml)?;
    let field = match doc.as_table().get_key_value("notify") {
        None => None,
        Some((key, item)) => {
            let value = item.as_value().ok_or(ConfigError::InvalidNotify)?;
            let array = value.as_array().ok_or(ConfigError::InvalidNotify)?;
            if array.len() > MAX_ARGUMENTS {
                return Err(ConfigError::InvalidNotify);
            }
            let mut arguments = Vec::with_capacity(array.len());
            let mut total = 0;
            for arg in array.iter() {
                let arg = arg.as_str().ok_or(ConfigError::InvalidNotify)?;
                total += arg.len();
                if arg.len() > MAX_ARGUMENT_BYTES || total > MAX_COMMAND_BYTES || arg.contains('\0')
                {
                    return Err(ConfigError::InvalidNotify);
                }
                arguments.push(arg.to_owned());
            }
            let shifted = |r: Range<usize>| (r.start + bom_len)..(r.end + bom_len);
            Some(NotifyField {
                key: shifted(key.span().ok_or(ConfigError::InvalidNotify)?),
                value: shifted(value.span().ok_or(ConfigError::InvalidNotify)?),
                arguments,
            })
        }
    };
    Ok(ParsedConfig {
        bom_len,
        root_len: doc.as_table().len(),
        field,
    })
}
fn is_managed(arguments: &[String]) -> bool {
    arguments.get(1).is_some_and(|arg| arg == NOTIFY_FLAG)
}
fn render_arguments(arguments: &[String]) -> String {
    let mut array = toml_edit::Array::new();
    for argument in arguments {
        array.push(argument.as_str());
    }
    array.to_string()
}
fn replace(source: &str, range: Range<usize>, replacement: &str) -> String {
    let mut result = String::with_capacity(source.len() - range.len() + replacement.len());
    result.push_str(&source[..range.start]);
    result.push_str(replacement);
    result.push_str(&source[range.end..]);
    result
}
fn removable_assignment(source: &str, range: Range<usize>, bom_len: usize) -> Range<usize> {
    let start = source[..range.start].rfind('\n').map_or(bom_len, |i| i + 1);
    let end = source[range.end..]
        .find('\n')
        .map_or(source.len(), |i| range.end + i + 1);
    let whitespace = |s: &str| s.bytes().all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'));
    if whitespace(&source[start..range.start]) && whitespace(&source[range.end..end]) {
        start..end
    } else {
        range
    } // Keep user-added surrounding comments even when the assignment is removed.
}
fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
