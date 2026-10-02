//! Main-process ownership. Provider hints enqueue ordinary reconciliation, never usage facts.
use std::{path::Path, sync::Arc};
use token_pulse_integration::{
    notify_registry::{RegistryError, windows::NotifyRegistry},
    notify_service::NotifyService,
};
pub(super) fn start(
    directory: &Path,
    collector: &token_pulse_store::StoreResult<
        Arc<token_pulse_collector::service::CollectorService>,
    >,
) -> Result<NotifyService, RegistryError> {
    let collector = collector.as_ref().map_err(|_| RegistryError::Io)?.clone();
    let registry = NotifyRegistry::open(directory)?;
    let executable = std::env::current_exe().map_err(|_| RegistryError::Io)?;
    NotifyService::start(
        registry,
        executable,
        Arc::new(move || collector.reconcile()),
    )
}
