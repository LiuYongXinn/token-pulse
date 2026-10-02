use crate::{ErrorCode, StoreResult};
use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Read,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
pub const SCHEMA_VERSION: i64 = 7;
const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../migrations/0001_initial.sql")),
    (
        2,
        include_str!("../migrations/0002_canonical_frontiers.sql"),
    ),
    (3, include_str!("../migrations/0003_usage_rollups.sql")),
    (
        4,
        include_str!("../migrations/0004_offline_price_catalogs.sql"),
    ),
    (
        5,
        include_str!("../migrations/0005_event_valuation_cache.sql"),
    ),
    (6, include_str!("../migrations/0006_price_revalue_jobs.sql")),
    (
        7,
        include_str!("../migrations/0007_source_scan_evidence.sql"),
    ),
];
static BACKUP_SERIAL: AtomicU64 = AtomicU64::new(0);
fn checksum(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
#[derive(Clone, Copy)]
enum Stage {
    AfterBackup,
    AfterDdl,
    BeforeCommit,
}
pub fn migrate(connection: &mut Connection, path: &Path) -> StoreResult<()> {
    migrate_with_hook(connection, path, |_| Ok(()))
}
fn verify_version(connection: &Connection, version: i64) -> StoreResult<()> {
    for (number, sql) in MIGRATIONS.iter().filter(|(number, _)| *number <= version) {
        let stored: String = connection
            .query_row(
                "SELECT checksum FROM schema_migrations WHERE version=?1",
                [number],
                |r| r.get(0),
            )
            .map_err(|_| ErrorCode::MigrationFailed)?;
        if stored != checksum(sql) {
            return Err(ErrorCode::MigrationFailed.into());
        }
    }
    let app: i64 = connection
        .query_row(
            "SELECT schema_version FROM app_state WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .map_err(|_| ErrorCode::MigrationFailed)?;
    if app != version {
        return Err(ErrorCode::MigrationFailed.into());
    }
    Ok(())
}
fn backup(connection: &Connection, path: &Path, version: i64) -> StoreResult<()> {
    let directory = path
        .parent()
        .ok_or(ErrorCode::MigrationFailed)?
        .join("migration-backups");
    fs::create_dir_all(&directory).map_err(|_| ErrorCode::MigrationFailed)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ErrorCode::InvalidQuery)?
        .as_nanos();
    let filename = format!(
        "before-v{version}-to-v{SCHEMA_VERSION}-{stamp}-{}-{}.db",
        std::process::id(),
        BACKUP_SERIAL.fetch_add(1, Ordering::Relaxed)
    );
    let target = directory.join(&filename);
    let reserve = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(|_| ErrorCode::MigrationFailed)?;
    drop(reserve);
    let mut destination = Connection::open(&target)?;
    destination.pragma_update(None, "synchronous", "FULL")?;
    {
        let copy = rusqlite::backup::Backup::new(connection, &mut destination)?;
        let mut last_progress = Instant::now();
        loop {
            match copy.step(128)? {
                rusqlite::backup::StepResult::Done => break,
                rusqlite::backup::StepResult::More => last_progress = Instant::now(),
                rusqlite::backup::StepResult::Busy | rusqlite::backup::StepResult::Locked => {
                    if last_progress.elapsed() > Duration::from_secs(5) {
                        return Err(ErrorCode::MigrationFailed.into());
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                _ => return Err(ErrorCode::MigrationFailed.into()),
            }
        }
    }
    let integrity: String = destination.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if integrity != "ok" {
        return Err(ErrorCode::MigrationFailed.into());
    }
    verify_version(&destination, version)?;
    destination.close().map_err(|(_, e)| e)?;
    let verified = Connection::open_with_flags(&target, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    verify_version(&verified, version)?;
    drop(verified);
    let mut file = fs::File::open(&target).map_err(|_| ErrorCode::MigrationFailed)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|_| ErrorCode::MigrationFailed)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    let manifest = serde_json::json!({"format":"token-pulse-migration-backup-v1","source_schema_version":version,"target_schema_version":SCHEMA_VERSION,"database_file":filename,"database_sha256":format!("{:x}",hash.finalize())});
    let mut manifest_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target.with_extension("json"))
        .map_err(|_| ErrorCode::MigrationFailed)?;
    serde_json::to_writer(&mut manifest_file, &manifest)?;
    manifest_file
        .sync_all()
        .map_err(|_| ErrorCode::MigrationFailed)?;
    Ok(())
}
fn migrate_with_hook(
    connection: &mut Connection,
    path: &Path,
    mut at: impl FnMut(Stage) -> StoreResult<()>,
) -> StoreResult<()> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if !(0..=SCHEMA_VERSION).contains(&version) {
        return Err(ErrorCode::MigrationFailed.into());
    }
    let integrity: String = connection.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if integrity != "ok" {
        return Err(ErrorCode::DbCorrupt.into());
    }
    if version == 0 {
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
            [],
            |r| r.get(0),
        )?;
        if count != 0 {
            return Err(ErrorCode::MigrationFailed.into());
        }
    } else {
        verify_version(connection, version)?;
        if version < SCHEMA_VERSION {
            backup(connection, path, version)?;
            at(Stage::AfterBackup)?;
        }
    }
    if version < SCHEMA_VERSION {
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: i64 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if current != version {
            return Err(ErrorCode::MigrationFailed.into());
        }
        for (number, sql) in MIGRATIONS.iter().filter(|(number, _)| *number > version) {
            tx.execute_batch(sql)?;
            if *number == 1 {
                tx.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,checksum TEXT NOT NULL)")?;
                let initialized = i64::try_from(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_err(|_| ErrorCode::InvalidQuery)?
                        .as_millis(),
                )
                .map_err(|_| ErrorCode::NumericOverflow)?;
                tx.execute("INSERT INTO settings(singleton,settings_version,payload_json,updated_at_ms) VALUES(1,1,?1,?2)",params![r#"{"theme":"dark","privacy":false,"mini_scope":{"kind":"today_all_sources"},"taskbar_enabled":false,"startup_enabled":false}"#,initialized])?;
            }
            tx.execute(
                "INSERT INTO schema_migrations(version,checksum) VALUES(?1,?2)",
                params![number, checksum(sql)],
            )?;
            tx.pragma_update(None, "user_version", number)?;
        }
        at(Stage::AfterDdl)?;
        let violations: i64 =
            tx.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
                r.get(0)
            })?;
        if violations != 0 {
            return Err(ErrorCode::MigrationFailed.into());
        }
        verify_version(&tx, SCHEMA_VERSION)?;
        at(Stage::BeforeCommit)?;
        tx.commit()?;
    }
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    Ok(())
}

#[cfg(test)]
mod tests;
