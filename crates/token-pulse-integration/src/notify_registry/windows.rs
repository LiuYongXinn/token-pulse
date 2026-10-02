//! Protected application-owned records and coalesced wake markers. Codex Home is never written here.
mod files;
use super::{
    MAX_REGISTRATION_BYTES, MAX_REGISTRATIONS, NotifyRegistration, RegistryError,
    valid_registration_id,
};
use crate::notify_channel::NotifyCapability;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

/// Only this dedicated child directory is written, with a current-user protected owner and DACL.
#[derive(Clone)]
pub struct NotifyRegistry {
    root: PathBuf,
}
impl NotifyRegistry {
    pub fn open(app_directory: &Path) -> Result<Self, RegistryError> {
        if !app_directory.is_absolute() {
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
        let _guard = files::allocation_guard(&self.root.join(".registry.lock"))?;
        if self.registrations()?.len() >= MAX_REGISTRATIONS {
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
            registrations.push(self.get(id)?);
            if registrations.len() > MAX_REGISTRATIONS {
                return Err(RegistryError::LimitReached);
            }
        }
        registrations.sort_by(|a, b| {
            a.capability()
                .registration_id()
                .cmp(b.capability().registration_id())
        });
        Ok(registrations)
    }
    /// A zero-byte dirty bit per registration coalesces all offline hints. No identity/body is saved.
    pub fn mark_pending(
        &self,
        capability: &NotifyCapability,
    ) -> Result<MarkDisposition, RegistryError> {
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
        self.get(id)?;
        let pending = self.named(id, ".wake")?;
        match require_marker(&pending) {
            Ok(()) => {}
            Err(RegistryError::NotFound) => return Ok(None),
            Err(error) => return Err(error),
        }
        let claimed = self
            .root
            .join(format!("{id}.claim.{}", uuid::Uuid::new_v4().simple()));
        match files::move_new(&pending, &claimed) {
            Ok(()) => Ok(Some(PendingWake {
                registry: self.clone(),
                pending,
                claimed,
                completed: false,
            })),
            Err(RegistryError::NotFound) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

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
}
impl PendingWake {
    pub fn complete(mut self) -> Result<(), RegistryError> {
        self.registry.check_root()?;
        require_marker(&self.claimed)?;
        std::fs::remove_file(&self.claimed).map_err(|_| RegistryError::Io)?;
        self.completed = true;
        Ok(())
    }
}
impl Drop for PendingWake {
    fn drop(&mut self) {
        if !self.completed
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
