use token_pulse_store::{Database, ErrorCode, SourceRecord, rusqlite::Connection};

fn source(id: &str) -> SourceRecord {
    SourceRecord {
        source_id: id.into(),
        root_path: "synthetic-read-only-source".into(),
        directory_identity: None,
        kind: "local".into(),
        enabled: true,
        created_at_ms: 1,
    }
}

#[test]
fn migration_reopens_and_readers_cannot_write() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    db.add_source(source("one")).unwrap();
    db.snapshot(|tx, rev| {
        assert_eq!(rev.settings, 1);
        assert!(tx.execute("DELETE FROM sources", []).is_err());
        // Validate the required persisted structures; the old 35-table count
        // predates price caches and source evidence and rejects a valid schema.
        for table in [
            "sources",
            "sessions",
            "usage_events",
            "stream_states",
            "schema_migrations",
            "price_rules",
            "valuation_cache_sets",
            "price_revalue_jobs",
        ] {
            assert!(
                tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                    [table],
                    |r| r.get::<_, bool>(0)
                )?,
                "missing {table}"
            );
        }
        let fk: i64 = tx.pragma_query_value(None, "foreign_keys", |r| r.get(0))?;
        assert_eq!(fk, 1);
        Ok(())
    })
    .unwrap();
    drop(db);
    let reopened = Database::open(dir.path()).unwrap();
    reopened
        .snapshot(|tx, _| {
            assert_eq!(
                tx.query_row("SELECT COUNT(*) FROM sources", [], |r| r.get::<_, i64>(0))?,
                1
            );
            Ok(())
        })
        .unwrap();
}
#[test]
fn real_read_snapshot_survives_a_concurrent_commit() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    db.add_source(source("before")).unwrap();
    db.snapshot(|tx, revision| {
        assert_eq!(revision.settings, 1);
        let writer = db.clone();
        std::thread::spawn(move || writer.add_source(source("after")).unwrap())
            .join()
            .unwrap();
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM sources", [], |r| r.get::<_, i64>(0))?,
            1
        );
        assert_eq!(
            tx.query_row("SELECT settings_revision FROM app_state", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        Ok(())
    })
    .unwrap();
    db.snapshot(|tx, revision| {
        assert_eq!(revision.settings, 2);
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM sources", [], |r| r.get::<_, i64>(0))?,
            2
        );
        Ok(())
    })
    .unwrap();
}
#[test]
fn exact_sql_aggregate_preserves_unknown_and_overflow() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    db.snapshot(|tx, _| {
        assert_eq!(tx.query_row("SELECT sum_token_decimal(v) FROM (SELECT 9223372036854775807 AS v UNION ALL SELECT 9223372036854775807)", [], |r|r.get::<_,String>(0))?, "18446744073709551614");
        assert_eq!(tx.query_row("SELECT sum_token_decimal(NULL)", [], |r|r.get::<_,Option<String>>(0))?, None);
        assert_eq!(tx.query_row("SELECT sum_money_atoms(v) FROM (SELECT '9007199254740993' AS v UNION ALL SELECT '1')", [], |r|r.get::<_,String>(0))?, "9007199254740994");
        assert!(tx.query_row("SELECT sum_money_atoms(v) FROM (SELECT '170141183460469231731687303715884105727' AS v UNION ALL SELECT '1')", [], |r|r.get::<_,String>(0)).is_err());
        Ok(())
    }).unwrap();
}

#[test]
fn combined_vector_aggregate_preserves_exact_zero_null_and_validation() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    db.snapshot(|tx,_| {
        let sums: serde_json::Value = serde_json::from_str(&tx.query_row("SELECT sum_usage_vector(i,c,o,r,t) FROM (SELECT 9223372036854775807 AS i,0 AS c,0 AS o,0 AS r,9223372036854775807 AS t UNION ALL SELECT 9223372036854775807,0,0,0,9223372036854775807)",[],|r|r.get::<_,String>(0))?).unwrap();
        assert_eq!(sums["total"],"18446744073709551614");
        assert_eq!(sums["measures"][0]["value"],"18446744073709551614");
        assert_eq!(sums["measures"][1]["value"],"0");
        assert_eq!(sums["measures"][1]["covered_total_tokens"],"18446744073709551614");
        assert_eq!(sums["measures"][1]["complete"],true);
        let unknown: serde_json::Value = serde_json::from_str(&tx.query_row("SELECT sum_usage_vector(NULL,NULL,NULL,NULL,5)",[],|r|r.get::<_,String>(0))?).unwrap();
        assert_eq!(unknown["total"],"5"); assert_eq!(unknown["measures"][0]["value"],serde_json::Value::Null);
        assert_eq!(unknown["measures"][0]["complete"],false);
        let empty: serde_json::Value = serde_json::from_str(&tx.query_row("SELECT sum_usage_vector(1,0,0,0,1) WHERE 0",[],|r|r.get::<_,String>(0))?).unwrap();
        assert_eq!(empty["total"],"0"); assert_eq!(empty["measures"][0]["value"],serde_json::Value::Null);
        assert!(tx.query_row("SELECT sum_usage_vector(0,1,5,0,5)",[],|r|r.get::<_,String>(0)).is_err());
        assert!(tx.query_row("SELECT sum_usage_vector(1,0,2,0,4)",[],|r|r.get::<_,String>(0)).is_err());
        Ok(())
    }).unwrap();
}

#[test]
fn projected_vectors_merge_without_turning_unknown_fields_into_zero() {
    use token_pulse_store::rusqlite::params;
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    db.snapshot(|tx,_| {
        let known:String=tx.query_row("SELECT sum_usage_vector(100,60,10,2,110)",[],|r|r.get(0))?;
        let unknown:String=tx.query_row("SELECT sum_usage_vector(NULL,NULL,NULL,NULL,7)",[],|r|r.get(0))?;
        let empty:String=tx.query_row("SELECT sum_usage_vector(1,0,0,0,1) WHERE 0",[],|r|r.get(0))?;
        let value:String=tx.query_row("SELECT sum_usage_projection(s,n) FROM (SELECT ?1 AS s,1 AS n UNION ALL SELECT ?2,1 UNION ALL SELECT ?3,0)",params![known,unknown,empty],|r|r.get(0))?;
        let value:serde_json::Value=serde_json::from_str(&value)?;
        assert_eq!(value["total"],"117");
        for (index,expected) in ["100","60","40","10","2"].into_iter().enumerate() {
            assert_eq!(value["measures"][index]["value"],expected);
            assert_eq!(value["measures"][index]["covered_total_tokens"],"110");
            assert_eq!(value["measures"][index]["complete"],false);
        }
        let value:String=tx.query_row("SELECT sum_usage_projection(?1,1)",[unknown],|r|r.get(0))?;
        let value:serde_json::Value=serde_json::from_str(&value)?;
        assert_eq!(value["measures"][0]["value"],serde_json::Value::Null);
        let value:String=tx.query_row("SELECT sum_usage_projection(?1,1)",[known],|r|r.get(0))?;
        let value:serde_json::Value=serde_json::from_str(&value)?;
        assert_eq!(value["measures"][0]["complete"],true);
        Ok(())
    }).unwrap();
}

#[test]
fn projected_vectors_preserve_large_sums_and_reject_corrupt_coverage() {
    use token_pulse_store::rusqlite::params;
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    db.snapshot(|tx,_| {
        let huge:String=tx.query_row("SELECT sum_usage_vector(9223372036854775807,0,0,0,9223372036854775807)",[],|r|r.get(0))?;
        let value:String=tx.query_row("SELECT sum_usage_projection(s,n) FROM (SELECT ?1 AS s,1 AS n UNION ALL SELECT ?1,1)",[&huge],|r|r.get(0))?;
        let value:serde_json::Value=serde_json::from_str(&value)?;
        assert_eq!(value["total"],"18446744073709551614");
        assert_eq!(value["measures"][1]["value"],"0");
        assert_eq!(value["measures"][1]["complete"],true);
        let mut invalid:serde_json::Value=serde_json::from_str(&huge)?;
        invalid["measures"][0]["covered_total_tokens"]=serde_json::json!("9223372036854775808");
        assert!(tx.query_row("SELECT sum_usage_projection(?1,1)",[invalid.to_string()],|r|r.get::<_,String>(0)).is_err());
        invalid=serde_json::from_str(&huge)?;invalid["measures"][0]["value"]=serde_json::Value::Null;
        assert!(tx.query_row("SELECT sum_usage_projection(?1,1)",[invalid.to_string()],|r|r.get::<_,String>(0)).is_err());
        assert!(tx.query_row("SELECT sum_usage_projection(?1,0)",[&huge],|r|r.get::<_,String>(0)).is_err());
        assert!(tx.query_row("SELECT sum_usage_projection(?1,-1)",[&huge],|r|r.get::<_,String>(0)).is_err());
        assert!(tx.query_row("SELECT sum_usage_projection(?1,1)",["{}"],|r|r.get::<_,String>(0)).is_err());
        invalid=serde_json::from_str(&huge)?;
        invalid["total"]=serde_json::json!("170141183460469231731687303715884105727");
        for measure in invalid["measures"].as_array_mut().unwrap() {
            measure["value"]=serde_json::Value::Null;
            measure["covered_total_tokens"]=serde_json::json!("0");
            measure["complete"]=serde_json::json!(false);
        }
        let encoded=invalid.to_string();
        let overflow=tx.query_row("SELECT sum_usage_projection(s,n) FROM (SELECT ?1 AS s,1 AS n UNION ALL SELECT ?1,1)",params![encoded],|r|r.get::<_,String>(0)).unwrap_err();
        assert_eq!(token_pulse_store::StoreError::from(overflow).code,ErrorCode::NumericOverflow);
        let ordinary=tx.prepare("SELECT missing_column FROM nonexistent_table").unwrap_err();
        assert_eq!(token_pulse_store::StoreError::from(ordinary).code,ErrorCode::DbWriteFailed);
        Ok(())
    }).unwrap();
}
#[test]
fn unknown_newer_schema_and_corruption_do_not_create_fresh_history() {
    let dir = tempfile::tempdir().unwrap();
    let connection = Connection::open(dir.path().join("token-pulse.db")).unwrap();
    connection.execute_batch("PRAGMA user_version=99").unwrap();
    drop(connection);
    assert_eq!(
        Database::open(dir.path()).err().unwrap().code,
        ErrorCode::MigrationFailed
    );
    let corrupt = tempfile::tempdir().unwrap();
    let path = corrupt.path().join("token-pulse.db");
    std::fs::write(&path, b"corrupt database preserved").unwrap();
    assert_eq!(
        Database::open(corrupt.path()).err().unwrap().code,
        ErrorCode::DbCorrupt
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"corrupt database preserved");
}
