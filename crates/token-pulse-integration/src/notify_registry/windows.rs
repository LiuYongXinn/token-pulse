//! Protected application-owned records and coalesced wake markers. Codex Home is never written here.
mod files;
use super::{
    MAX_REGISTRATION_BYTES, MAX_REGISTRATIONS, NotifyRegistration, RegistryError,
    local_absolute_path, valid_registration_id,
};
use crate::{
    notify_channel::NotifyCapability,
    notify_config::{
        owns_current_notify,
        windows::{ConfigFileError, lock_current_config},
    },
};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// Only this dedicated child directory is written, with a current-user protected owner and DACL.
#[derive(Clone)]
pub struct NotifyRegistry {
    root: PathBuf,
}
impl NotifyRegistry {
    pub fn open(app_directory: &Path) -> Result<Self, RegistryError> {
        if !local_absolute_path(app_directory) {
            return Err(RegistryError::UnsafePath);
        }
        files::require_plain_directory(app_directory)?;
        let app_directory = std::fs::canonicalize(app_directory).map_err(|_| RegistryError::Io)?;
        let root = app_directory.join("notify");
        files::create_private_directory(&root)?;
        let registry = Self { root };
        registry.check_root()?;
        Ok(registry)
    }
    fn check_root(&self) -> Result<(), RegistryError> {
        files::open_verified(&self.root, true).map(|_| ())
    }
    fn mutation_guard(&self) -> Result<std::fs::File, RegistryError> {
        self.check_root()?;
        let until = Instant::now() + Duration::from_millis(250);
        loop {
            match files::allocation_guard(&self.root.join(".registry.lock")) {
                Err(RegistryError::Io) if Instant::now() < until => {
                    std::thread::sleep(Duration::from_millis(2))
                }
                result => return result,
            }
        }
    }
    fn named(&self, id: &str, suffix: &str) -> Result<PathBuf, RegistryError> {
        if !valid_registration_id(id) {
            return Err(RegistryError::InvalidRecord);
        }
        self.check_root()?;
        Ok(self.root.join(format!("{id}{suffix}")))
    }
    pub fn create(&self, registration: &NotifyRegistration) -> Result<(), RegistryError> {
        self.check_root()?;
        // Deny sharing while allocating a registration, so separate processes cannot exceed the limit.
        let _guard = self.mutation_guard()?;
        if self.registration_ids()?.len() >= MAX_REGISTRATIONS {
            return Err(RegistryError::LimitReached);
        }
        let bytes = serde_json::to_vec(registration).map_err(|_| RegistryError::InvalidRecord)?;
        if bytes.len() > MAX_REGISTRATION_BYTES {
            return Err(RegistryError::TooLarge);
        }
        let id = registration.capability().registration_id();
        let destination = self.named(id, ".registration.json")?;
        let temporary = self
            .root
            .join(format!(".tmp.{}", uuid::Uuid::new_v4().simple()));
        let result = (|| {
            let mut file = files::create_private_file(&temporary)?;
            file.write_all(&bytes).map_err(|_| RegistryError::Io)?;
            file.sync_all().map_err(|_| RegistryError::Io)?;
            drop(file);
            // Immutable identity, never replace an existing registration or restore record.
            files::move_new(&temporary, &destination)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    }
    pub fn get(&self, id: &str) -> Result<NotifyRegistration, RegistryError> {
        let path = self.named(id, ".registration.json")?;
        let file = files::open_verified(&path, false)?;
        if file.metadata().map_err(|_| RegistryError::Io)?.len() > MAX_REGISTRATION_BYTES as u64 {
            return Err(RegistryError::TooLarge);
        }
        let mut bytes = Vec::new();
        file.take(MAX_REGISTRATION_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| RegistryError::Io)?;
        if bytes.len() > MAX_REGISTRATION_BYTES {
            return Err(RegistryError::TooLarge);
        }
        let registration: NotifyRegistration =
            serde_json::from_slice(&bytes).map_err(|_| RegistryError::InvalidRecord)?;
        if registration.capability().registration_id() != id {
            return Err(RegistryError::InvalidRecord);
        }
        Ok(registration)
    }
    pub fn registrations(&self) -> Result<Vec<NotifyRegistration>, RegistryError> {
        self.registration_ids()?
            .iter()
            .map(|id| self.get(id))
            .collect()
    }
    /// Bounded identities only; one corrupt record does not prevent other profiles from loading.
    pub fn registration_ids(&self) -> Result<Vec<String>, RegistryError> {
        self.check_root()?;
        let mut registrations = Vec::new();
        for (count, entry) in std::fs::read_dir(&self.root)
            .map_err(|_| RegistryError::Io)?
            .enumerate()
        {
            if count >= 128 {
                return Err(RegistryError::LimitReached);
            }
            let name = entry.map_err(|_| RegistryError::Io)?.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(id) = name.strip_suffix(".registration.json") else {
                continue;
            };
            if !valid_registration_id(id) {
                return Err(RegistryError::InvalidRecord);
            }
            registrations.push(id.to_owned());
            if registrations.len() > MAX_REGISTRATIONS {
                return Err(RegistryError::LimitReached);
            }
        }
        registrations.sort();
        Ok(registrations)
    }
    /// A zero-byte dirty bit per registration coalesces all offline hints. No identity/body is saved.
    pub fn mark_pending(
        &self,
        capability: &NotifyCapability,
    ) -> Result<MarkDisposition, RegistryError> {
        let _guard = self.mutation_guard()?;
        let registered = self.get(capability.registration_id())?;
        if !registered.capability().matches(capability) {
            return Err(RegistryError::Unauthorized);
        }
        let path = self.named(capability.registration_id(), ".wake")?;
        match files::create_private_file(&path) {
            Ok(file) => {
                file.sync_all().map_err(|_| RegistryError::Io)?;
                Ok(MarkDisposition::Created)
            }
            Err(RegistryError::AlreadyExists) => {
                require_marker(&path)?;
                Ok(MarkDisposition::AlreadyPending)
            }
            Err(error) => Err(error),
        }
    }
    /// Rename before acknowledging, so completion cannot remove a newly arrived wake marker.
    pub fn claim_pending(&self, id: &str) -> Result<Option<PendingWake>, RegistryError> {
        let _guard = self.mutation_guard()?;
        let pending = self.named(id, ".wake")?;
        match require_marker(&pending) {
            Ok(()) => {}
            Err(RegistryError::NotFound) => return Ok(None),
            Err(error) => return Err(error),
        }
        let capability = self.get(id)?.capability().clone();
        let claimed = self
            .root
            .join(format!("{id}.claim.{}", uuid::Uuid::new_v4().simple()));
        match files::move_new(&pending, &claimed) {
            Ok(()) => Ok(Some(PendingWake {
                registry: self.clone(),
                pending,
                claimed,
                completed: false,
                capability,
            })),
            Err(RegistryError::NotFound) => Ok(None),
            Err(error) => Err(error),
        }
    }
    pub fn has_pending(&self, id: &str) -> Result<bool, RegistryError> {
        match require_marker(&self.named(id, ".wake")?) {
            Ok(()) => Ok(true),
            Err(RegistryError::NotFound) => Ok(false),
            Err(error) => Err(error),
        }
    }
    pub(crate) fn service_guard(&self) -> Result<std::fs::File, RegistryError> {
        self.check_root()?;
        files::allocation_guard(&self.root.join(".service.lock"))
    }
    /// Called only by the single service owner on startup, never by headless marker writers.
    pub(crate) fn recover_claims(&self) -> Result<(), RegistryError> {
        let _guard = self.mutation_guard()?;
        self.check_root()?;
        for (count, entry) in std::fs::read_dir(&self.root)
            .map_err(|_| RegistryError::Io)?
            .enumerate()
        {
            if count >= 128 {
                return Err(RegistryError::LimitReached);
            }
            let entry = entry.map_err(|_| RegistryError::Io)?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some((id, claim_id)) = name.split_once(".claim.") else {
                continue;
            };
            if !valid_registration_id(id) || !valid_registration_id(claim_id) {
                return Err(RegistryError::InvalidMarker);
            }
            self.get(id)?;
            let claimed = entry.path();
            require_marker(&claimed)?;
            let pending = self.named(id, ".wake")?;
            match files::move_new(&claimed, &pending) {
                Ok(()) => {}
                Err(RegistryError::AlreadyExists) => {
                    require_marker(&pending)?;
                    std::fs::remove_file(claimed).map_err(|_| RegistryError::Io)?;
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
    /// Retire only when the latest config is known not to contain this exact managed command.
    /// Keep config/missing-name and private file guards until all controlled removals complete.
    pub fn retire(&self, capability: &NotifyCapability) -> Result<RetireDisposition, RetireError> {
        let _guard = self.mutation_guard()?;
        let id = capability.registration_id();
        let record = match self.get(id) {
            Ok(record) => record,
            Err(RegistryError::NotFound) => return Ok(RetireDisposition::AlreadyAbsent),
            Err(error) => return Err(error.into()),
        };
        if !record.capability().matches(capability) {
            return Err(RegistryError::Unauthorized.into());
        }
        let config = lock_current_config(record.codex_home()).map_err(RetireError::Config)?;
        if let Some(bytes) = config.bytes() {
            if owns_current_notify(bytes, record.restore_record())
                .map_err(|error| RetireError::Config(error.into()))?
            {
                return Err(RetireError::ActiveConfiguration);
            }
        }
        let mut markers = Vec::new();
        for (count, entry) in std::fs::read_dir(&self.root)
            .map_err(|_| RegistryError::Io)?
            .enumerate()
        {
            if count >= 128 {
                return Err(RegistryError::LimitReached.into());
            }
            let entry = entry.map_err(|_| RegistryError::Io)?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let claim = name.strip_prefix(&format!("{id}.claim."));
            if name != format!("{id}.wake") && claim.is_none() {
                continue;
            }
            if claim.is_some_and(|suffix| !valid_registration_id(suffix)) {
                return Err(RegistryError::InvalidMarker.into());
            }
            let file = files::open_for_delete(&entry.path())?;
            if file.metadata().map_err(|_| RegistryError::Io)?.len() != 0 {
                return Err(RegistryError::InvalidMarker.into());
            }
            markers.push(file);
        }
        let mut registered = files::open_for_delete(&self.named(id, ".registration.json")?)?;
        if registered.metadata().map_err(|_| RegistryError::Io)?.len()
            > MAX_REGISTRATION_BYTES as u64
        {
            return Err(RegistryError::TooLarge.into());
        }
        let mut bytes = Vec::new();
        (&mut registered)
            .take(MAX_REGISTRATION_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| RegistryError::Io)?;
        let current: NotifyRegistration =
            serde_json::from_slice(&bytes).map_err(|_| RegistryError::InvalidRecord)?;
        if serde_json::to_vec(&current).map_err(|_| RegistryError::InvalidRecord)?
            != serde_json::to_vec(&record).map_err(|_| RegistryError::InvalidRecord)?
        {
            return Err(RegistryError::Unauthorized.into());
        }
        for file in markers {
            files::delete_owned(file)?;
        }
        files::delete_owned(registered)?;
        Ok(RetireDisposition::Retired)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetireDisposition {
    Retired,
    AlreadyAbsent,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetireError {
    Registry(RegistryError),
    Config(ConfigFileError),
    ActiveConfiguration,
}
impl From<RegistryError> for RetireError {
    fn from(error: RegistryError) -> Self {
        Self::Registry(error)
    }
}
impl std::fmt::Display for RetireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Registry(error) => error.fmt(f),
            Self::Config(error) => error.fmt(f),
            Self::ActiveConfiguration => f.write_str("notify_registration_still_active"),
        }
    }
}
impl std::error::Error for RetireError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkDisposition {
    Created,
    AlreadyPending,
}
/// Dropping without completion restores the dirty bit. Concurrent new markers remain untouched.
pub struct PendingWake {
    registry: NotifyRegistry,
    pending: PathBuf,
    claimed: PathBuf,
    completed: bool,
    capability: NotifyCapability,
}
impl PendingWake {
    pub fn complete(mut self) -> Result<(), RegistryError> {
        let _guard = self.registry.mutation_guard()?;
        self.registry.check_root()?;
        if matches!(
            self.registry.get(self.capability.registration_id()),
            Err(RegistryError::NotFound)
        ) {
            self.completed = true;
            return Ok(());
        }
        require_marker(&self.claimed)?;
        std::fs::remove_file(&self.claimed).map_err(|_| RegistryError::Io)?;
        self.completed = true;
        Ok(())
    }
}
impl Drop for PendingWake {
    fn drop(&mut self) {
        if self.completed {
            return;
        }
        let Ok(_guard) = self.registry.mutation_guard() else {
            return;
        };
        let current = self.registry.get(self.capability.registration_id());
        if matches!(current, Err(RegistryError::NotFound))
            || current
                .as_ref()
                .is_ok_and(|record| !record.capability().matches(&self.capability))
        {
            if let Ok(file) = files::open_for_delete(&self.claimed) {
                if file.metadata().is_ok_and(|metadata| metadata.len() == 0) {
                    let _ = files::delete_owned(file);
                }
            }
            return;
        }
        if current.is_ok()
            && self.registry.check_root().is_ok()
            && require_marker(&self.claimed).is_ok()
        {
            match files::move_new(&self.claimed, &self.pending) {
                Ok(()) => {}
                Err(RegistryError::AlreadyExists) if require_marker(&self.pending).is_ok() => {
                    let _ = std::fs::remove_file(&self.claimed);
                }
                _ => {} // Leave the owned claim on IO failure; startup scanning remains authoritative.
            }
        }
    }
}
fn require_marker(path: &Path) -> Result<(), RegistryError> {
    let file = files::open_marker(path)?;
    if file.metadata().map_err(|_| RegistryError::Io)?.len() != 0 {
        return Err(RegistryError::InvalidMarker);
    }
    Ok(())
}
