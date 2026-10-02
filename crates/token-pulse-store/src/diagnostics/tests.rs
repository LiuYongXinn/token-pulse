use super::*;
use crate::{SourceRecord, batch};
use rusqlite::params;

fn request(source: Option<&str>) -> DiagnosticsRequest {
    DiagnosticsRequest {
        source_id: source.map(str::to_owned),
    }
}
fn diagnostic(
    db: &Database,
    id: &str,
    generation: Option<&str>,
    resolved: bool,
    offset: Option<i64>,
) {
    let id = id.to_owned();
    let generation = generation.map(str::to_owned);
    db.write(move |conn| {
        conn.execute("INSERT INTO diagnostics(diagnostic_id,source_id,file_generation_id,byte_offset,code,severity,metadata_json,dedup_key,first_seen_at_ms,last_seen_at_ms,resolved_at_ms) VALUES(?1,'source',?2,?3,'UNSUPPORTED_FORMAT','warning','{\"private\":\"must never leave backend\"}',?1,1,2,?4)", params![id,generation,offset,if resolved {Some(3)} else {None}])?;
        Ok(())
    }).unwrap();
}
#[test]
fn only_unresolved_selected_generations_have_positions_and_move_follows_current_mapping() {
    let (_dir, db) = batch::tests::setup();
    db.commit(batch::tests::fixture()).unwrap();
    db.write(|conn| {
        conn.execute("UPDATE file_generations SET observed_size=9007199254741093 WHERE file_generation_id='generation'",[])?;
        for (id, state) in [("candidate", "candidate"), ("retired", "retired"), ("invalid", "invalid"), ("unselected", "current")] {
            conn.execute("INSERT INTO file_generations SELECT ?1,file_id,?2,identity_json,observed_size,committed_offset,checkpoint_revision,anchor_json,reader_context_json,parser_version,created_at_ms FROM file_generations WHERE file_generation_id='generation'", params![id,state])?;
        }
        Ok(())
    }).unwrap();
    for generation in ["candidate", "retired", "invalid", "unselected"] {
        diagnostic(&db, generation, Some(generation), false, Some(0));
    }
    diagnostic(&db, "resolved", Some("generation"), true, Some(0));
    diagnostic(
        &db,
        "current",
        Some("generation"),
        false,
        Some(9007199254740993),
    );
    diagnostic(&db, "without-position", None, false, None);
    let value = db.diagnostics(&request(None)).unwrap();
    assert_eq!(value.issues.len(), 2);
    let located = value.issues.iter().find(|i| i.path.is_some()).unwrap();
    assert_eq!(located.path.as_deref(), Some("synthetic.jsonl"));
    assert_eq!(
        located.byte_offset.as_ref().unwrap().as_str(),
        "9007199254740993"
    );
    let unknown = value.issues.iter().find(|i| i.path.is_none()).unwrap();
    assert!(unknown.byte_offset.is_none());
    let json = serde_json::to_string(&value).unwrap();
    assert!(!json.contains("private") && !json.contains("must never leave backend"));
    db.write(|conn| {
        conn.execute(
            "UPDATE source_files SET canonical_path='archived_sessions/moved.jsonl'",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    let moved = db.diagnostics(&request(None)).unwrap();
    let moved = moved.issues.iter().find(|i| i.path.is_some()).unwrap();
    assert_eq!(moved.issue_id, located.issue_id);
    assert_eq!(moved.path.as_deref(), Some("archived_sessions/moved.jsonl"));
    db.write(|conn| {
        conn.execute(
            "UPDATE source_files SET current_generation_id='unselected'",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    let current = db.diagnostics(&request(None)).unwrap();
    assert!(
        current
            .issues
            .iter()
            .all(|i| i.issue_id != located.issue_id)
    );
}
#[test]
fn saved_pending_classification_is_active_only_and_never_returns_evidence_or_unknown_as_zero() {
    let (_dir, db) = batch::tests::setup();
    db.commit(batch::tests::fixture()).unwrap();
    db.write(|conn| {
        conn.execute("INSERT INTO ledger_generations SELECT 'unpublished',session_key,'candidate',parser_version,accounting_version,base_data_revision,created_at_ms,NULL,input_manifest_json FROM ledger_generations WHERE ledger_id='ledger'",[])?;
        for (id, ledger, kind) in [("pending","ledger","pending"),("anchor","ledger","unattributed"),("mirror","ledger","duplicate"),("inherit","ledger","inherited"),("hidden","unpublished","pending")] {
            conn.execute("INSERT INTO pending_usage VALUES(?1,?2,'observation',?3,'private_reason',NULL,'{\"private\":\"lineage evidence\"}')",params![id,ledger,kind])?;
        }
        Ok(())
    }).unwrap();
    let value = db.diagnostics(&request(None)).unwrap();
    assert_eq!(value.issues.len(), 2);
    assert!(
        value
            .issues
            .iter()
            .any(|i| i.kind == DiagnosticKind::UnconfirmedUsage)
    );
    assert!(
        value
            .issues
            .iter()
            .any(|i| i.kind == DiagnosticKind::UnattributedUsage)
    );
    assert!(
        value
            .issues
            .iter()
            .all(|i| i.code.is_none() && i.byte_offset.as_ref().unwrap().as_str() == "0")
    );
    let json = serde_json::to_string(&value).unwrap();
    assert!(!json.contains("private") && !json.contains("lineage"));
    db.write(|conn| {
        conn.execute("UPDATE sessions SET active_ledger_id='unpublished'", [])?;
        Ok(())
    })
    .unwrap();
    assert_eq!(db.diagnostics(&request(None)).unwrap().issues.len(), 1);
    db.write(|conn| {
        conn.execute("UPDATE file_generations SET state='retired'", [])?;
        Ok(())
    })
    .unwrap();
    assert!(db.diagnostics(&request(None)).unwrap().issues.is_empty());
}
#[test]
fn scan_and_missing_file_keep_unknown_positions_and_paused_or_wrong_root_scans_are_excluded() {
    let (_dir, db) = batch::tests::setup();
    let scan = db
        .begin_source_scan("source".into(), "synthetic".into(), 2)
        .unwrap();
    db.finish_source_scan(scan, Some(ErrorCode::SourceUnreadable), 3)
        .unwrap();
    db.write(|conn| {
        conn.execute("UPDATE source_files SET status='missing'", [])?;
        Ok(())
    })
    .unwrap();
    let value = db.diagnostics(&request(Some("source"))).unwrap();
    assert_eq!(value.issues.len(), 2);
    assert!(value.issues.iter().all(|i| i.byte_offset.is_none()));
    assert!(
        value
            .issues
            .iter()
            .any(|i| i.kind == DiagnosticKind::DirectoryScan
                && i.path.as_deref() == Some("synthetic")
                && i.code == Some(ErrorCode::SourceUnreadable))
    );
    db.write(|conn| {
        conn.execute("UPDATE sources SET root_path='changed'", [])?;
        Ok(())
    })
    .unwrap();
    assert_eq!(db.diagnostics(&request(None)).unwrap().issues.len(), 1);
    db.write(|conn| {
        conn.execute("UPDATE sources SET root_path='synthetic',enabled=0", [])?;
        Ok(())
    })
    .unwrap();
    assert_eq!(db.diagnostics(&request(None)).unwrap().issues.len(), 1);
}
#[test]
fn repeated_same_file_and_cause_returns_latest_representative_not_record_history() {
    let (_dir, db) = batch::tests::setup();
    for index in 0..25 {
        diagnostic(
            &db,
            &format!("repeated-{index}"),
            Some("generation"),
            false,
            Some(index),
        );
    }
    db.write(|conn| {
        conn.execute(
            "UPDATE diagnostics SET last_seen_at_ms=10 WHERE diagnostic_id='repeated-24'",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    let value = db.diagnostics(&request(None)).unwrap();
    assert_eq!(value.issues.len(), 1);
    assert!(!value.has_more);
    assert_eq!(value.issues[0].byte_offset.as_ref().unwrap().as_str(), "24");
}
#[test]
fn source_filter_limit_and_real_old_transaction_are_consistent() {
    let (_dir, db) = batch::tests::setup();
    db.add_source(SourceRecord {
        source_id: "other".into(),
        root_path: "other-root".into(),
        directory_identity: None,
        kind: "local".into(),
        enabled: true,
        created_at_ms: 1,
    })
    .unwrap();
    for index in 0..21 {
        let generation = format!("generation-{index}");
        db.register_file(crate::FileRegistration {
            file_id: format!("file-{index}"),
            source_id: "source".into(),
            canonical_path: format!("synthetic-{index}.jsonl"),
            file_identity: None,
            file_generation_id: generation.clone(),
            observed_size: 100,
            created_at_ms: 1,
            reader_context: Default::default(),
        })
        .unwrap();
        diagnostic(
            &db,
            &format!("row-{index:02}"),
            Some(&generation),
            false,
            Some(index),
        );
    }
    let value = db.diagnostics(&request(None)).unwrap();
    assert_eq!(value.issues.len(), 20);
    assert!(value.has_more);
    assert_eq!(
        db.diagnostics(&request(None)).unwrap().issues[0].issue_id,
        value.issues[0].issue_id
    );
    let other = db.diagnostics(&request(Some("other"))).unwrap();
    assert!(other.issues.is_empty() && !other.has_more);
    assert_eq!(
        db.diagnostics(&request(Some("unknown"))).unwrap_err().code,
        ErrorCode::InvalidQuery
    );
    assert_eq!(
        db.diagnostics(&request(Some("../path"))).unwrap_err().code,
        ErrorCode::InvalidQuery
    );
    db.snapshot(|tx, rev| {
        let before = read(tx, rev.data, &request(None))?;
        db.write(|conn| {
            conn.execute("UPDATE diagnostics SET resolved_at_ms=4", [])?;
            conn.execute("UPDATE app_state SET data_revision=data_revision+1", [])?;
            Ok(())
        })?;
        let after = read(tx, rev.data, &request(None))?;
        assert_eq!(
            serde_json::to_value(before).unwrap(),
            serde_json::to_value(after).unwrap()
        );
        Ok(())
    })
    .unwrap();
    let newest = db.diagnostics(&request(None)).unwrap();
    assert!(newest.issues.is_empty());
    assert_eq!(
        newest.data_revision.value(),
        value.data_revision.value() + 1
    );
}
