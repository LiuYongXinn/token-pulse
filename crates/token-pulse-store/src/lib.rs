//! Application-owned storage. Source log directories are never write targets.

use std::{io, path::Path};

pub mod aggregate;
pub mod batch;
mod canonical_progress;
pub mod collection;
mod database;
pub mod jobs;
pub mod leases;
pub mod maintenance;
mod migration;
pub mod mini;
pub mod pricing;
mod proof_jobs;
pub mod query;
pub mod rebuild;
mod registration;
pub mod revalue_jobs;
pub mod revalue_service;
pub mod rollup;
pub mod rollup_service;
pub mod settings;
pub mod source_management;
pub mod valuation;
pub use database::{Database, Revision, SourceRecord};
pub use registration::{FileRegistration, SessionRegistration};
pub use rusqlite;
pub use token_pulse_core::error::ErrorCode;

#[derive(Debug)]
pub struct StoreError {
    pub code: ErrorCode,
}
pub type StoreResult<T> = Result<T, StoreError>;
impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.code.fmt(f)
    }
}
impl std::error::Error for StoreError {}
impl From<ErrorCode> for StoreError {
    fn from(code: ErrorCode) -> Self {
        Self { code }
    }
}
impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        use rusqlite::ErrorCode as SqliteCode;
        let code = match error {
            rusqlite::Error::SqliteFailure(e, _) if e.code == SqliteCode::DiskFull => {
                ErrorCode::DiskFull
            }
            rusqlite::Error::SqliteFailure(e, _)
                if matches!(
                    e.code,
                    SqliteCode::DatabaseCorrupt | SqliteCode::NotADatabase
                ) =>
            {
                ErrorCode::DbCorrupt
            }
            rusqlite::Error::UserFunctionError(e) if e.downcast_ref::<ErrorCode>().is_some() => {
                *e.downcast_ref::<ErrorCode>().unwrap()
            }
            // SQLite callbacks serialize user-function errors through sqlite3_result_error;
            // the outer statement receives SQLITE_ERROR and an exact controlled code.
            rusqlite::Error::SqliteFailure(e, Some(message))
                if e.code == SqliteCode::Unknown && e.extended_code == 1 =>
            {
                serde_json::from_value::<ErrorCode>(serde_json::Value::String(message))
                    .unwrap_or(ErrorCode::DbWriteFailed)
            }
            _ => ErrorCode::DbWriteFailed,
        };
        Self { code }
    }
}
impl From<serde_json::Error> for StoreError {
    fn from(_: serde_json::Error) -> Self {
        ErrorCode::InvalidQuery.into()
    }
}

pub fn prepare_data_directory(path: &Path) -> io::Result<()> {
    std::fs::create_dir_all(path)
}
