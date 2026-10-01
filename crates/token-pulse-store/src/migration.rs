use crate::{ErrorCode, StoreResult};
use rusqlite::{Connection, TransactionBehavior, params};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: i64 = 1;
const INITIAL: &str = include_str!("../migrations/0001_initial.sql");

pub fn migrate(connection: &mut Connection) -> StoreResult<()> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if !(0..=SCHEMA_VERSION).contains(&version) {
        return Err(ErrorCode::MigrationFailed.into());
    }
    let integrity: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(ErrorCode::DbCorrupt.into());
    }
    let checksum = format!("{:x}", Sha256::digest(INITIAL.as_bytes()));
    if version == 0 {
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
            [],
            |r| r.get(0),
        )?;
        if count != 0 {
            return Err(ErrorCode::MigrationFailed.into());
        }
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch(INITIAL)?;
        transaction.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, checksum TEXT NOT NULL); PRAGMA user_version=1;")?;
        transaction.execute("INSERT INTO schema_migrations VALUES (1, ?1)", [&checksum])?;
        let initialized_at = i64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| ErrorCode::InvalidQuery)?
                .as_millis(),
        )
        .map_err(|_| ErrorCode::NumericOverflow)?;
        transaction.execute("INSERT INTO settings(singleton,settings_version,payload_json,updated_at_ms) VALUES(1,1,?1,?2)", params![r#"{"theme":"dark","privacy":false,"mini_scope":{"kind":"today_all_sources"},"taskbar_enabled":false,"startup_enabled":false}"#,initialized_at])?;
        transaction.commit()?;
    } else {
        let stored: String = connection.query_row(
            "SELECT checksum FROM schema_migrations WHERE version=?1",
            params![SCHEMA_VERSION],
            |r| r.get(0),
        )?;
        let app_version: i64 = connection.query_row(
            "SELECT schema_version FROM app_state WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        if checksum != stored || app_version != SCHEMA_VERSION {
            return Err(ErrorCode::MigrationFailed.into());
        }
    }
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    Ok(())
}
