use super::*;

#[test]
fn schema_eighteen_title_upgrade_preserves_ids_revisions_and_backs_up_old_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("token-pulse.db");
    let mut conn = Connection::open(&path).unwrap();
    migrate(&mut conn, &path).unwrap();
    conn.execute_batch("DROP VIEW session_labels; DROP TABLE session_titles; DELETE FROM schema_migrations WHERE version=19; UPDATE app_state SET schema_version=18,data_revision=19,price_revision=7,settings_revision=77; PRAGMA user_version=18; INSERT INTO sessions(session_key,provider,provider_session_id,identity_status) VALUES('old-session','codex','old-provider','confirmed');").unwrap();
    let before: i64 = conn
        .query_row("SELECT usage_view_revision FROM app_state", [], |r| {
            r.get(0)
        })
        .unwrap();
    migrate(&mut conn, &path).unwrap();
    assert_eq!(
        conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        19
    );
    assert_eq!(
        conn.query_row(
            "SELECT display_name FROM session_labels WHERE session_key='old-session'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "old-provider"
    );
    assert_eq!(conn.query_row("SELECT data_revision,price_revision,settings_revision,usage_view_revision FROM app_state", [], |r| Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?,r.get::<_,i64>(3)?))).unwrap(), (19,7,77,before));
    let backup = fs::read_dir(dir.path().join("migration-backups"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|ext| ext == "db"))
        .unwrap();
    let old = Connection::open_with_flags(backup, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(
        old.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        18
    );
    assert_eq!(
        old.query_row(
            "SELECT provider_session_id FROM sessions WHERE session_key='old-session'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "old-provider"
    );
}
fn legacy(path: &Path) -> Connection {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(MIGRATIONS[0].1).unwrap();
    conn.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,checksum TEXT NOT NULL); PRAGMA user_version=1; UPDATE app_state SET data_revision=19,settings_revision=77;").unwrap();
    conn.execute(
        "INSERT INTO schema_migrations VALUES(1,?1)",
        [checksum(MIGRATIONS[0].1)],
    )
    .unwrap();
    conn.execute_batch("INSERT INTO settings VALUES(1,1,'{\"theme\":\"dark\",\"privacy\":true}',1); INSERT INTO sources(source_id,provider,root_path,kind,enabled,readability,capabilities_json,created_at_ms) VALUES('legacy','codex','synthetic-readonly-source','local',0,'disabled','{}',1);").unwrap();
    conn
}
fn assert_legacy(conn: &Connection) {
    assert_eq!(
        conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row("SELECT data_revision FROM app_state", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        19
    );
    assert!(
        conn.query_row(
            "SELECT json_extract(payload_json,'$.privacy') FROM settings",
            [],
            |r| r.get::<_, bool>(0)
        )
        .unwrap()
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM sources", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='session_aliases'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

fn legacy_v2(path: &Path) -> Connection {
    let conn = legacy(path);
    conn.execute_batch(MIGRATIONS[1].1).unwrap();
    conn.execute(
        "INSERT INTO schema_migrations VALUES(2,?1)",
        [checksum(MIGRATIONS[1].1)],
    )
    .unwrap();
    conn.execute_batch("PRAGMA user_version=2; INSERT INTO sessions(session_key,provider,identity_status) VALUES('old-session','codex','confirmed'); INSERT INTO ledger_generations VALUES('old-ledger','old-session','active','old-parser','old-accounting',19,1,1,'{}'); UPDATE sessions SET active_ledger_id='old-ledger' WHERE session_key='old-session';").unwrap();
    conn
}

#[test]
fn v2_upgrade_initializes_only_rebuildable_versions_and_backs_up_the_actual_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("token-pulse.db");
    drop(legacy_v2(&path));
    let db = crate::Database::open(dir.path()).unwrap();
    db.snapshot(|tx, r| {
        assert_eq!(r.data, 19);
        assert_eq!(r.settings, 77);
        assert_eq!(
            tx.query_row(
                "SELECT revision FROM ledger_usage_versions WHERE ledger_id='old-ledger'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            0
        );
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM usage_rollup_sets", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        assert_eq!(
            tx.query_row(
                "SELECT active_ledger_id FROM sessions WHERE session_key='old-session'",
                [],
                |r| r.get::<_, String>(0)
            )?,
            "old-ledger"
        );
        Ok(())
    })
    .unwrap();
    let copied_path = fs::read_dir(dir.path().join("migration-backups"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|s| s == "db"))
        .unwrap();
    let copied =
        Connection::open_with_flags(&copied_path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(
        copied
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert!(
        copied
            .prepare("SELECT * FROM ledger_usage_versions")
            .is_err()
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(copied_path.with_extension("json")).unwrap()).unwrap();
    assert_eq!(manifest["source_schema_version"], 2);
    assert_eq!(manifest["target_schema_version"], SCHEMA_VERSION);
}

#[test]
fn v2_upgrade_faults_leave_old_pointers_versions_and_backups_intact() {
    for failure in [Stage::AfterBackup, Stage::AfterDdl, Stage::BeforeCommit] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token-pulse.db");
        let mut conn = legacy_v2(&path);
        assert!(
            migrate_with_hook(&mut conn, &path, |stage| {
                if std::mem::discriminant(&stage) == std::mem::discriminant(&failure) {
                    Err(ErrorCode::MigrationFailed.into())
                } else {
                    Ok(())
                }
            })
            .is_err()
        );
        assert_eq!(
            conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            conn.query_row(
                "SELECT active_ledger_id FROM sessions WHERE session_key='old-session'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "old-ledger"
        );
        assert!(conn.prepare("SELECT * FROM ledger_usage_versions").is_err());
        assert_eq!(
            fs::read_dir(dir.path().join("migration-backups"))
                .unwrap()
                .count(),
            2
        );
    }
}
#[test]
fn legacy_upgrade_has_a_verified_consistent_backup_and_preserves_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("token-pulse.db");
    let conn = legacy(&path);
    drop(conn);
    let db = crate::Database::open(dir.path()).unwrap();
    db.snapshot(|tx, r| {
        assert_eq!(r.data, 19);
        assert_eq!(r.settings, 77);
        assert_eq!(
            tx.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))?,
            SCHEMA_VERSION
        );
        assert!(tx.query_row(
            "SELECT json_extract(payload_json,'$.privacy') FROM settings",
            [],
            |r| r.get::<_, bool>(0)
        )?);
        Ok(())
    })
    .unwrap();
    let entries = fs::read_dir(dir.path().join("migration-backups"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 2);
    let backup = entries
        .iter()
        .find(|p| p.extension().is_some_and(|s| s == "db"))
        .unwrap();
    let copied = Connection::open_with_flags(backup, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_legacy(&copied);
    drop(copied);
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(backup.with_extension("json")).unwrap()).unwrap();
    assert_eq!(manifest["source_schema_version"], 1);
    assert_eq!(
        manifest["database_sha256"],
        format!("{:x}", Sha256::digest(fs::read(backup).unwrap()))
    );
    drop(db);
    let db = crate::Database::open(dir.path()).unwrap();
    drop(db);
    assert_eq!(
        fs::read_dir(dir.path().join("migration-backups"))
            .unwrap()
            .count(),
        2
    );
}
#[test]
fn every_upgrade_failure_boundary_preserves_the_legacy_schema_and_backup() {
    for failure in [Stage::AfterBackup, Stage::AfterDdl, Stage::BeforeCommit] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token-pulse.db");
        let mut conn = legacy(&path);
        assert_eq!(
            migrate_with_hook(&mut conn, &path, |stage| {
                if std::mem::discriminant(&stage) == std::mem::discriminant(&failure) {
                    Err(ErrorCode::MigrationFailed.into())
                } else {
                    Ok(())
                }
            })
            .unwrap_err()
            .code,
            ErrorCode::MigrationFailed
        );
        drop(conn);
        let conn = Connection::open(&path).unwrap();
        assert_legacy(&conn);
        let copied_path = fs::read_dir(dir.path().join("migration-backups"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.extension().is_some_and(|s| s == "db"))
            .unwrap();
        assert_legacy(&Connection::open(copied_path).unwrap());
    }
}
#[test]
fn checksum_and_backup_failures_do_not_begin_an_upgrade() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("token-pulse.db");
    let mut conn = legacy(&path);
    conn.execute("UPDATE schema_migrations SET checksum='tampered'", [])
        .unwrap();
    assert_eq!(
        migrate(&mut conn, &path).unwrap_err().code,
        ErrorCode::MigrationFailed
    );
    assert!(!dir.path().join("migration-backups").exists());
    assert_legacy(&conn);
    conn.execute(
        "UPDATE schema_migrations SET checksum=?1",
        [checksum(MIGRATIONS[0].1)],
    )
    .unwrap();
    fs::write(
        dir.path().join("migration-backups"),
        b"directory unavailable",
    )
    .unwrap();
    assert_eq!(
        migrate(&mut conn, &path).unwrap_err().code,
        ErrorCode::MigrationFailed
    );
    assert_legacy(&conn);
}
#[test]
fn legacy_frontier_backfill_uses_proven_observation_order_and_not_episode_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("token-pulse.db");
    let mut conn = legacy(&path);
    conn.execute_batch("INSERT INTO sessions(session_key,provider,identity_status) VALUES('session','codex','confirmed'); INSERT INTO ledger_generations(ledger_id,session_key,state,parser_version,accounting_version,base_data_revision,created_at_ms,input_manifest_json) VALUES('ledger','session','active','v1','v1',19,1,'{}'); UPDATE sessions SET active_ledger_id='ledger'; INSERT INTO source_files(file_id,source_id,canonical_path,status) VALUES('file','legacy','synthetic.jsonl','present'); INSERT INTO file_generations(file_generation_id,file_id,state,identity_json,observed_size,committed_offset,anchor_json,reader_context_json,parser_version,created_at_ms) VALUES('generation','file','current','null',200,200,'[]','{}','v1',1); UPDATE source_files SET current_generation_id='generation';").unwrap();
    let encoded =
        serde_json::to_string(&crate::batch::tests::fixture().observations[0].record).unwrap();
    for (id, offset) in [("old", 0), ("new", 100)] {
        conn.execute("INSERT INTO observations(observation_id,file_generation_id,byte_offset,byte_end,session_key,kind,normalized_json,payload_fingerprint,format_version) VALUES(?1,'generation',?2,?3,'session','usage',?4,'candidate-only','v1')",params![id,offset,offset+100,encoded]).unwrap();
    }
    let baseline =
        serde_json::to_string(&crate::batch::tests::fixture().streams[0].baseline).unwrap();
    for (episode, observation) in [("z-old", "old"), ("a-new", "new")] {
        conn.execute(
            "INSERT INTO stream_states VALUES('ledger','stream',?1,?2,?3,'\"confirmed\"',1)",
            params![episode, baseline, observation],
        )
        .unwrap();
    }
    migrate(&mut conn, &path).unwrap();
    assert_eq!(
        conn.query_row("SELECT episode_id FROM stream_frontiers", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "a-new"
    );
    assert!(
        conn.execute(
            "INSERT INTO stream_frontiers VALUES('ledger','bad','missing')",
            []
        )
        .is_err()
    );
}

#[test]
fn pre_upgrade_backup_includes_committed_wal_pages_without_modifying_the_source_tree() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("token-pulse.db");
    let conn = legacy(&path);
    conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    conn.execute_batch("INSERT INTO sources(source_id,provider,root_path,kind,enabled,readability,capabilities_json,created_at_ms) VALUES('wal-row','codex','synthetic-readonly-source','local',0,'disabled','{}',2);").unwrap();
    assert!(path.with_extension("db-wal").exists());
    let db = crate::Database::open(dir.path()).unwrap();
    let copied_path = fs::read_dir(dir.path().join("migration-backups"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|s| s == "db"))
        .unwrap();
    let copied =
        Connection::open_with_flags(copied_path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(
        copied
            .query_row("SELECT COUNT(*) FROM sources", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        copied
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM sources", [], |r| r.get::<_, i64>(0))?,
            2
        );
        Ok(())
    })
    .unwrap();
}
