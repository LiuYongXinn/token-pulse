use super::AccountServiceTarget;
use crate::{error::ErrorCode, numeric::DecimalInt};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
#[derive(Clone)]
pub struct AccountServiceCandidate {
    pub target: AccountServiceTarget,
    pub settings_revision: DecimalInt,
}
struct Lease {
    candidate: AccountServiceCandidate,
    expires: Instant,
}
#[derive(Default)]
pub struct AccountServiceSelections {
    leases: BTreeMap<String, Lease>,
}
impl AccountServiceSelections {
    pub fn insert(
        &mut self,
        handle: String,
        candidate: AccountServiceCandidate,
        now: Instant,
    ) -> Result<(), ErrorCode> {
        candidate.target.validate()?;
        self.leases.retain(|_, lease| lease.expires > now);
        if handle.is_empty()
            || handle.len() > 128
            || self.leases.contains_key(&handle)
            || self.leases.len() >= 16
        {
            return Err(ErrorCode::InvalidQuery);
        }
        self.leases.insert(
            handle,
            Lease {
                candidate,
                expires: now + Duration::from_secs(300),
            },
        );
        Ok(())
    }
    /// These capabilities are main-only and never shared with source / export selections.
    pub fn get(
        &self,
        handle: &str,
        owner: &str,
        now: Instant,
    ) -> Result<AccountServiceCandidate, ErrorCode> {
        if owner != "main" {
            return Err(ErrorCode::PermissionDenied);
        }
        let lease = self
            .leases
            .get(handle)
            .ok_or(ErrorCode::StaleConfirmation)?;
        if lease.expires <= now {
            return Err(ErrorCode::StaleConfirmation);
        }
        Ok(lease.candidate.clone())
    }
    pub fn remove(&mut self, handle: &str) {
        self.leases.remove(handle);
    }
}
