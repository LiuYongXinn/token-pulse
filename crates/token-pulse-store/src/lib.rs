//! Application-owned storage. Source log directories are never write targets.

use std::{io, path::Path};

pub mod aggregate;
pub mod batch;
pub mod collection;
mod database;
mod migration;
mod registration;
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
