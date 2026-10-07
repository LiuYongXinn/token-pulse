use crate::{Database, ErrorCode};
use std::sync::{Arc, Mutex};

#[test]
fn view_only_changes_commit_notify_without_tokens_and_rollback_is_silent() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    let notices = Arc::new(Mutex::new(Vec::new()));
    let captured = notices.clone();
    db.on_usage_changed(Arc::new(move |version| captured.lock().unwrap().push(version)));
    let before = db.usage_revision().unwrap();
    db.write(|conn| {
        let tx = conn.transaction()?;
        tx.execute("INSERT INTO projects(project_id,canonical_cwd,display_name,normalization_version) VALUES('p','synthetic','before',1)", [])?;
        tx.commit()?;
        Ok(())
    }).unwrap();
    let inserted = db.usage_revision().unwrap();
    assert_eq!(before.data_revision, inserted.data_revision);
    assert!(inserted.usage_view_revision.value() > before.usage_view_revision.value());
    let failure: crate::StoreResult<()> = db.write(|conn| {
        let tx = conn.transaction()?;
        tx.execute("UPDATE projects SET display_name='rolled back' WHERE project_id='p'", [])?;
        Err(ErrorCode::DbWriteFailed.into())
    });
    assert!(failure.is_err());
    assert_eq!(db.usage_revision().unwrap(), inserted);
    assert_eq!(notices.lock().unwrap().len(), 1);
    db.write(|conn| { conn.execute("UPDATE projects SET display_name='after' WHERE project_id='p'", [])?; Ok(()) }).unwrap();
    assert_eq!(notices.lock().unwrap().len(), 2);
    let identity = db.usage_revision().unwrap().database_id;
    drop(db);
    assert_eq!(Database::open(dir.path()).unwrap().usage_revision().unwrap().database_id, identity);
    let other = tempfile::tempdir().unwrap();
    assert_ne!(Database::open(other.path()).unwrap().usage_revision().unwrap().database_id, identity);
}
