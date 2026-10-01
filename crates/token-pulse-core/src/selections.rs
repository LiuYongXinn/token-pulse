//! Native-selected targets cannot be replaced with renderer-supplied paths or reused for other purposes.
use crate::{error::ErrorCode, sources::SourceOrigin};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    time::{Duration, Instant},
};
pub struct SelectedDirectory {
    pub path: PathBuf,
    pub origin: SourceOrigin,
}
struct Lease {
    window: String,
    target: SelectedDirectory,
    expires: Instant,
}
#[derive(Default)]
pub struct DirectorySelections {
    leases: BTreeMap<String, Lease>,
}
impl DirectorySelections {
    pub fn insert(
        &mut self,
        handle: String,
        window: String,
        target: SelectedDirectory,
        now: Instant,
    ) -> Result<(), ErrorCode> {
        self.leases.retain(|_, lease| lease.expires > now);
        if self.leases.len() >= 16
            || handle.is_empty()
            || self.leases.contains_key(&handle)
            || window != "main"
        {
            return Err(ErrorCode::InvalidQuery);
        }
        self.leases.insert(
            handle,
            Lease {
                window,
                target,
                expires: now + Duration::from_secs(300),
            },
        );
        Ok(())
    }
    pub fn take(
        &mut self,
        handle: &str,
        window: &str,
        now: Instant,
    ) -> Result<SelectedDirectory, ErrorCode> {
        let lease = self.leases.get(handle).ok_or(ErrorCode::InvalidQuery)?;
        if lease.window != window {
            return Err(ErrorCode::PermissionDenied);
        }
        if lease.expires <= now {
            self.leases.remove(handle);
            return Err(ErrorCode::StaleConfirmation);
        }
        Ok(self.leases.remove(handle).expect("checked lease").target)
    }
}
