//! Application-owned optional notify registration. Never a full Codex config or log backup.
use crate::{notify_channel::NotifyCapability, notify_config::NotifyRestoreRecord};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[cfg(windows)]
pub mod windows;
pub const MAX_REGISTRATION_BYTES: usize = 3 * 1024 * 1024;
pub const MAX_REGISTRATIONS: usize = 16;

/// Restores only notify, carries a local pipe capability and an explicit original-command choice.
/// No Debug implementation: the local capability and original arguments are private state.
#[derive(Clone, Serialize, Deserialize)]
#[serde(try_from = "RegistrationWire")]
pub struct NotifyRegistration {
    version: u32,
    codex_home: String,
    capability: NotifyCapability,
    restore: NotifyRestoreRecord,
    chain_original: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistrationWire {
    version: u32,
    codex_home: String,
    capability: NotifyCapability,
    restore: NotifyRestoreRecord,
    chain_original: bool,
}
impl TryFrom<RegistrationWire> for NotifyRegistration {
    type Error = RegistryError;
    fn try_from(wire: RegistrationWire) -> Result<Self, Self::Error> {
        if wire.version != 1 {
            return Err(RegistryError::InvalidRecord);
        }
        Self::from_prepared(
            Path::new(&wire.codex_home),
            wire.capability,
            wire.restore,
            wire.chain_original,
        )
    }
}
impl NotifyRegistration {
    pub fn from_prepared(
        home: &Path,
        capability: NotifyCapability,
        restore: NotifyRestoreRecord,
        chain_original: bool,
    ) -> Result<Self, RegistryError> {
        let home_text = home.to_str().ok_or(RegistryError::InvalidRecord)?;
        if !home.is_absolute()
            || home_text.len() > 4096
            || home_text.chars().any(char::is_control)
            || restore.installed_arguments().get(3).map(String::as_str)
                != Some(capability.registration_id())
        {
            return Err(RegistryError::InvalidRecord);
        }
        if chain_original
            && !restore
                .original_arguments()
                .map_err(|_| RegistryError::InvalidRecord)?
                .is_some_and(|args| args.first().is_some_and(|exe| !exe.is_empty()))
        {
            return Err(RegistryError::InvalidRecord);
        }
        Ok(Self {
            version: 1,
            codex_home: home_text.to_owned(),
            capability,
            restore,
            chain_original,
        })
    }
    pub fn codex_home(&self) -> &Path {
        Path::new(&self.codex_home)
    }
    pub fn capability(&self) -> &NotifyCapability {
        &self.capability
    }
    pub fn restore_record(&self) -> &NotifyRestoreRecord {
        &self.restore
    }
    pub fn chain_original(&self) -> bool {
        self.chain_original
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistryError {
    Io,
    UnsafePath,
    UnsafePermissions,
    InvalidRecord,
    InvalidMarker,
    TooLarge,
    LimitReached,
    AlreadyExists,
    NotFound,
    Unauthorized,
}
impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Io => "notify_registry_io",
            Self::UnsafePath => "notify_registry_unsafe_path",
            Self::UnsafePermissions => "notify_registry_unsafe_permissions",
            Self::InvalidRecord => "notify_registry_invalid_record",
            Self::InvalidMarker => "notify_registry_invalid_marker",
            Self::TooLarge => "notify_registry_too_large",
            Self::LimitReached => "notify_registry_limit",
            Self::AlreadyExists => "notify_registry_exists",
            Self::NotFound => "notify_registry_not_found",
            Self::Unauthorized => "notify_registry_unauthorized",
        })
    }
}
impl std::error::Error for RegistryError {}
pub(crate) fn valid_registration_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
