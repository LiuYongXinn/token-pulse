//! Server-owned notify drafts and actual config/registration operations. No frontend commands,
//! full-config serialization, database facts or authentication access.
use crate::{
    notify_channel::NotifyCapability,
    notify_config::{
        ManagedNotifyCommand, owns_current_notify,
        windows::{
            ConfigFileError, ConfigFilePlan, prepare_enable_file, prepare_restore_file,
            read_optional_config,
        },
    },
    notify_registry::{
        NotifyRegistration, RegistryError,
        windows::{NotifyRegistry, RetireDisposition, RetireError},
    },
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
const MAX_PLANS: usize = 8;
const PLAN_LIFETIME: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotifyOperation {
    Enable,
    Disable,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationError {
    Registry(RegistryError),
    Config(ConfigFileError),
    Retirement(RetireError),
    NoOriginalCommand,
    PlanNotFound,
    PlanExpired,
    PlanLimit,
    ConfigApplyCleanupFailed(ConfigFileError, RetireError),
}
impl From<RegistryError> for OperationError {
    fn from(error: RegistryError) -> Self {
        Self::Registry(error)
    }
}
impl From<ConfigFileError> for OperationError {
    fn from(error: ConfigFileError) -> Self {
        Self::Config(error)
    }
}
impl From<RetireError> for OperationError {
    fn from(error: RetireError) -> Self {
        Self::Retirement(error)
    }
}
impl std::fmt::Display for OperationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Registry(error) => error.fmt(f),
            Self::Config(error) => error.fmt(f),
            Self::Retirement(error) => error.fmt(f),
            Self::NoOriginalCommand => f.write_str("notify_no_original_command"),
            Self::PlanNotFound => f.write_str("notify_plan_not_found"),
            Self::PlanExpired => f.write_str("notify_plan_expired"),
            Self::PlanLimit => f.write_str("notify_plan_limit"),
            Self::ConfigApplyCleanupFailed(_, _) => {
                f.write_str("notify_config_apply_cleanup_failed")
            }
        }
    }
}
impl std::error::Error for OperationError {}

/// Only explicit review information, never capability nonce or unrelated config values.
/// App IPC must apply shared privacy before mapping these internal values to DTOs.
pub struct NotifyPreview {
    pub plan_id: String,
    pub registration_id: String,
    pub home: PathBuf,
    pub operation: NotifyOperation,
    pub before_value: Option<String>,
    pub after_value: Option<String>,
    pub creates_config: bool,
    pub can_chain_original: bool,
    pub chain_original: bool,
}
pub struct NotifyRegistrationStatus {
    pub registration_id: String,
    pub home: Option<PathBuf>,
    pub configured: Option<bool>,
    pub current_executable: Option<bool>,
    pub chain_original: Option<bool>,
    pub error: Option<OperationError>,
}
pub struct ApplyOutcome {
    pub registration_id: String,
    pub configured: bool,
    /// Config was successfully applied. A separate cleanup failure must not pretend it was not.
    pub cleanup_error: Option<OperationError>,
}
struct PendingPlan {
    file: ConfigFilePlan,
    record: NotifyRegistration,
    operation: NotifyOperation,
    deadline: Instant,
}
pub struct NotifyManager {
    registry: NotifyRegistry,
    executable: PathBuf,
    plans: BTreeMap<String, PendingPlan>,
}
impl NotifyManager {
    pub fn new(registry: NotifyRegistry, executable: PathBuf) -> Self {
        Self {
            registry,
            executable,
            plans: BTreeMap::new(),
        }
    }
    /// None preserves an existing original program by default; false is an explicit replacement
    /// choice. In both cases the exact original notify value remains in the restore record.
    pub fn prepare_enable(
        &mut self,
        home: &Path,
        chain_original: Option<bool>,
    ) -> Result<NotifyPreview, OperationError> {
        self.reserve_plan()?;
        let capability = NotifyCapability::new();
        let command = ManagedNotifyCommand::new(&self.executable, capability.registration_id())
            .map_err(|error| OperationError::Config(error.into()))?;
        let file = prepare_enable_file(home, &command)?;
        let available = has_original(file.restore_record())?;
        let chain = chain_original.unwrap_or(available);
        if chain && !available {
            return Err(OperationError::NoOriginalCommand);
        }
        let record = NotifyRegistration::from_prepared(
            file.home(),
            capability,
            file.restore_record().clone(),
            chain,
        )?;
        self.remember(file, record, NotifyOperation::Enable, available)
    }
    pub fn prepare_disable(&mut self, id: &str) -> Result<NotifyPreview, OperationError> {
        self.reserve_plan()?;
        let record = self.registry.get(id)?;
        let file = prepare_restore_file(record.codex_home(), record.restore_record())?;
        let available = has_original(record.restore_record())?;
        self.remember(file, record, NotifyOperation::Disable, available)
    }
    fn reserve_plan(&mut self) -> Result<(), OperationError> {
        let now = Instant::now();
        self.plans.retain(|_, plan| now < plan.deadline);
        if self.plans.len() >= MAX_PLANS {
            return Err(OperationError::PlanLimit);
        }
        Ok(())
    }
    fn remember(
        &mut self,
        file: ConfigFilePlan,
        record: NotifyRegistration,
        operation: NotifyOperation,
        available: bool,
    ) -> Result<NotifyPreview, OperationError> {
        let id = uuid::Uuid::new_v4().simple().to_string();
        let preview = NotifyPreview {
            plan_id: id.clone(),
            registration_id: record.capability().registration_id().to_owned(),
            home: file.home().to_owned(),
            operation,
            before_value: file.before_value().map(str::to_owned),
            after_value: file.after_value().map(str::to_owned),
            creates_config: file.creates_config(),
            can_chain_original: available,
            chain_original: record.chain_original(),
        };
        self.plans.insert(
            id,
            PendingPlan {
                file,
                record,
                operation,
                deadline: Instant::now() + PLAN_LIFETIME,
            },
        );
        Ok(preview)
    }
    /// Applying accepts only an opaque server plan ID. No changed path, nonce, JSON or command.
    /// Busy/stale failures retain the draft until release/expiry; success consumes it.
    pub fn apply(&mut self, plan_id: &str) -> Result<ApplyOutcome, OperationError> {
        let plan = self
            .plans
            .get(plan_id)
            .ok_or(OperationError::PlanNotFound)?;
        if Instant::now() >= plan.deadline {
            self.plans.remove(plan_id);
            return Err(OperationError::PlanExpired);
        }
        let id = plan.record.capability().registration_id().to_owned();
        let outcome = match plan.operation {
            NotifyOperation::Enable => {
                // Earlier failed preparations for this Home are inactive. Their controlled
                // retirement avoids consuming the 16 registration slots across normal retries.
                for other_id in self.registry.registration_ids()? {
                    let Ok(other) = self.registry.get(&other_id) else {
                        continue;
                    };
                    if other.codex_home().canonicalize().ok().as_deref() == Some(plan.file.home()) {
                        self.registry.retire(other.capability())?;
                    }
                }
                // Validate/stage first: an unsupported or stale config must not allocate a record.
                // The restore record must then be durable before the new notify becomes visible.
                let staged = plan.file.stage()?;
                self.registry.create(&plan.record)?;
                if let Err(error) = staged.commit() {
                    // The failed config transaction has been released. Inactive cleanup is safe
                    // to retry on the next enable; retain metadata when it cannot be proven safe.
                    if let Err(cleanup) = self.registry.retire(plan.record.capability()) {
                        return Err(OperationError::ConfigApplyCleanupFailed(error, cleanup));
                    }
                    return Err(error.into());
                }
                ApplyOutcome {
                    registration_id: id,
                    configured: true,
                    cleanup_error: None,
                }
            }
            NotifyOperation::Disable => {
                plan.file.apply()?;
                let cleanup_error = self
                    .registry
                    .retire(plan.record.capability())
                    .err()
                    .map(OperationError::from);
                ApplyOutcome {
                    registration_id: id,
                    configured: false,
                    cleanup_error,
                }
            }
        };
        self.plans.remove(plan_id);
        Ok(outcome)
    }
    pub fn release(&mut self, plan_id: &str) -> bool {
        self.plans.remove(plan_id).is_some()
    }
    pub fn release_all(&mut self) {
        self.plans.clear();
    }
    pub fn retire_inactive(&self, id: &str) -> Result<RetireDisposition, OperationError> {
        match self.registry.get(id) {
            Ok(record) => Ok(self.registry.retire(record.capability())?),
            Err(RegistryError::NotFound) => Ok(RetireDisposition::AlreadyAbsent),
            Err(error) => Err(error.into()),
        }
    }
    pub fn registrations(&self) -> Result<Vec<NotifyRegistrationStatus>, OperationError> {
        self.registry
            .registration_ids()?
            .into_iter()
            .map(|id| {
                let mut status = NotifyRegistrationStatus {
                    registration_id: id.clone(),
                    home: None,
                    configured: None,
                    current_executable: None,
                    chain_original: None,
                    error: None,
                };
                match self.registry.get(&id) {
                    Err(error) => status.error = Some(error.into()),
                    Ok(record) => {
                        status.home = Some(record.codex_home().to_owned());
                        status.chain_original = Some(record.chain_original());
                        status.current_executable = Some(
                            Path::new(&record.restore_record().installed_arguments()[0])
                                == self.executable,
                        );
                        // A missing config is known inactive; unreadable/malformed remains unknown.
                        match read_optional_config(record.codex_home()) {
                            Err(error) => status.error = Some(error.into()),
                            Ok(config) => match config.as_deref() {
                                None => status.configured = Some(false),
                                Some(bytes) => {
                                    match owns_current_notify(bytes, record.restore_record()) {
                                        Ok(value) => status.configured = Some(value),
                                        Err(error) => {
                                            status.error =
                                                Some(OperationError::Config(error.into()))
                                        }
                                    }
                                }
                            },
                        }
                    }
                }
                Ok(status)
            })
            .collect()
    }
}
fn has_original(
    record: &crate::notify_config::NotifyRestoreRecord,
) -> Result<bool, OperationError> {
    Ok(record
        .original_arguments()
        .map_err(|error| OperationError::Config(error.into()))?
        .is_some_and(|arguments| arguments.first().is_some_and(|exe| !exe.is_empty())))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expired_plan_is_rejected_then_freed_and_capacity_releases_without_file_side_effects() {
        let app = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let mut manager = NotifyManager::new(
            NotifyRegistry::open(app.path()).unwrap(),
            std::env::current_exe().unwrap(),
        );
        let preview = manager.prepare_enable(home.path(), None).unwrap();
        manager.plans.get_mut(&preview.plan_id).unwrap().deadline = Instant::now();
        assert_eq!(
            manager.apply(&preview.plan_id).err(),
            Some(OperationError::PlanExpired)
        );
        for _ in 0..MAX_PLANS {
            manager.prepare_enable(home.path(), None).unwrap();
        }
        assert_eq!(
            manager.prepare_enable(home.path(), None).err(),
            Some(OperationError::PlanLimit)
        );
        manager.release_all();
        assert!(manager.prepare_enable(home.path(), None).is_ok());
        assert!(!home.path().join("config.toml").exists());
        assert!(manager.registry.registration_ids().unwrap().is_empty());
    }
}
