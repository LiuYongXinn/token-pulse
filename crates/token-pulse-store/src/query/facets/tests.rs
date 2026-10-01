use super::*;
use crate::{
    SourceRecord,
    batch::tests::{fixture, setup},
    query::tests::{extra, filter, ids},
};
use rusqlite::params;
use token_pulse_core::query::model_key;

fn request(dimension: FacetDimension, page_size: u16) -> FilterOptionsRequest {
    FilterOptionsRequest {
        query: FilterOptionsQuery {
            filter: filter(),
            dimension,
            search: String::new(),
            page_size,
        },
        cursor: None,
    }
}
fn fetch(db: &Database, request: &FilterOptionsRequest) -> FilterOptionsPage {
    db.filter_options("main", request, EpochMs::new(1234).unwrap())
        .unwrap()
}

#[test]
fn keyset_pages_keep_unknown_and_provider_identity_and_count_events() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    extra(&db, "a", 1100, 7, (Some("Same"), Some("p1")), None, None);
    extra(&db, "b", 1200, 8, (Some("Same"), Some("p2")), None, None);
    extra(&db, "c", 1300, 9, (Some("Same"), Some("p1")), None, None);
    let mut req = request(FacetDimension::Models, 1);
    let first = fetch(&db, &req);
    assert_eq!(first.options[0].key, None);
    assert_eq!(first.options[0].count.as_str(), "1");
    let snapshot = first.meta.snapshot_id.clone();
    let mut keys = Vec::new();
    req.cursor = first.next_cursor;
    while req.cursor.is_some() {
        let page = fetch(&db, &req);
        assert_eq!(page.meta.snapshot_id, snapshot);
        assert_eq!(page.meta.generated_at_ms.value(), 1234);
        keys.push((
            page.options[0].key.clone().unwrap(),
            page.options[0].count.as_str().to_owned(),
        ));
        req.cursor = page.next_cursor;
    }
    keys.sort();
    let mut expected = vec![
        (model_key(Some("p1"), Some("Same")).unwrap(), "2".into()),
        (model_key(Some("p2"), Some("Same")).unwrap(), "1".into()),
    ];
    expected.sort();
    assert_eq!(keys, expected);
    // Every terminal page frees its slot, even after many complete searches.
    for _ in 0..5 {
        assert_eq!(
            fetch(&db, &request(FacetDimension::Models, 200))
                .options
                .len(),
            3
        );
    }
}

#[test]
fn literal_unicode_search_and_other_dimensions_remain_bound() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.write(|conn| {
        conn.execute(
            "INSERT INTO projects VALUES('project-a','synthetic-a','Display A','中文 Alias A',1)",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    extra(
        &db,
        "a",
        1100,
        7,
        (Some("Mixed中文%_') OR 1=1--"), Some("P")),
        None,
        Some("project-a"),
    );
    extra(
        &db,
        "b",
        5000,
        8,
        (Some("outside"), Some("P")),
        None,
        Some("project-a"),
    );
    let mut req = request(FacetDimension::Models, 200);
    req.query.filter.models = ids(&["nonexistent"], false); // Own dimension ignored.
    req.query.filter.projects = ids(&["project-a"], false);
    req.query.search = "mixed中文%_') or 1=1--".into();
    let page = fetch(&db, &req);
    assert_eq!(page.options.len(), 1);
    assert_eq!(page.options[0].count.as_str(), "1"); // Half-open end excludes b.
    req.query.search = "%_".into();
    assert_eq!(fetch(&db, &req).options.len(), 1);
    req.query.search = "absent%".into();
    assert!(fetch(&db, &req).options.is_empty());
    let mut project = request(FacetDimension::Projects, 200);
    project.query.filter.projects = ids(&["nonexistent"], false);
    project.query.filter.models = ids(
        &[&model_key(Some("P"), Some("Mixed中文%_') OR 1=1--")).unwrap()],
        false,
    );
    project.query.search = "中文 alias".into();
    let page = fetch(&db, &project);
    assert_eq!(page.options[0].key.as_deref(), Some("project-a"));
    assert_eq!(page.options[0].display_name, "中文 Alias A");
    let mut session = request(FacetDimension::Sessions, 200);
    session.query.filter.sessions = ids(&["missing"], false);
    assert_eq!(
        fetch(&db, &session).options[0].display_name,
        "provider-session"
    );
    session.query.filter.sources = ids(&["missing"], false);
    assert!(fetch(&db, &session).options.is_empty());
}

#[test]
fn source_universe_includes_paused_zero_and_deduplicates_provenance() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    for (id, enabled) in [("mirror", true), ("empty", false)] {
        db.add_source(SourceRecord {
            source_id: id.into(),
            root_path: format!("synthetic-{id}"),
            directory_identity: None,
            kind: "local".into(),
            enabled,
            created_at_ms: 1,
        })
        .unwrap();
    }
    db.write(|conn| {
        conn.execute("INSERT INTO source_files(file_id,source_id,canonical_path,status) VALUES('mf','mirror','synthetic-mirror.jsonl','known')",[])?;
        conn.execute("INSERT INTO file_generations SELECT 'mg','mf',state,identity_json,observed_size,committed_offset,checkpoint_revision,anchor_json,reader_context_json,parser_version,created_at_ms FROM file_generations WHERE file_generation_id='generation'",[])?;
        for i in 1..=2 {
            conn.execute("INSERT INTO observations SELECT ?1,'mg',?2,?3,session_key,kind,observed_at_ms,stable_record_id,turn_id,stream_hint,model,project_id,normalized_json,payload_fingerprint,format_version FROM observations WHERE observation_id='observation'",params![format!("copy-{i}"),i*100,i*100+100])?;
            conn.execute("INSERT INTO event_provenance VALUES('event',?1,'mirror')",[format!("copy-{i}")])?;
        } Ok(())
    }).unwrap();
    let mut req = request(FacetDimension::Sources, 200);
    req.query.filter.sources = ids(&["missing"], false);
    let page = fetch(&db, &req);
    assert_eq!(
        page.options
            .iter()
            .map(|o| (o.key.as_deref().unwrap(), o.count.as_str()))
            .collect::<Vec<_>>(),
        vec![("empty", "0"), ("mirror", "1"), ("source", "1")]
    );
    assert!(page.options[0].display_name.ends_with("（暂停）"));
    req.query.filter.projects = ids(&["missing"], false);
    assert!(
        fetch(&db, &req)
            .options
            .iter()
            .all(|o| o.count.as_str() == "0")
    );
}

#[test]
fn old_pages_keep_facts_revisions_and_time_and_reject_rebinding() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    extra(&db, "a", 1100, 7, (Some("before"), Some("P")), None, None);
    let mut req = request(FacetDimension::Models, 1);
    let first = fetch(&db, &req);
    req.cursor = first.next_cursor.clone();
    for changed in 0..3 {
        let mut invalid = req.clone();
        match changed {
            0 => invalid.query.search = "before".into(),
            1 => invalid.query.page_size = 2,
            _ => invalid.query.filter.sources = ids(&["source"], false),
        }
        assert_eq!(
            db.filter_options("main", &invalid, EpochMs::new(9999).unwrap())
                .unwrap_err()
                .code,
            ErrorCode::CursorInvalid
        );
    }
    assert_eq!(
        db.filter_options("other", &req, EpochMs::new(9999).unwrap())
            .unwrap_err()
            .code,
        ErrorCode::CursorInvalid
    );
    extra(&db, "new", 1200, 8, (Some("after"), Some("P")), None, None);
    db.write(|conn| {
        conn.execute("UPDATE app_state SET price_revision=price_revision+1", [])?;
        Ok(())
    })
    .unwrap();
    let next = db
        .filter_options("main", &req, EpochMs::new(9999).unwrap())
        .unwrap();
    assert_eq!(next.options.len(), 1);
    assert_eq!(next.options[0].display_name, "before · P");
    assert_eq!(
        serde_json::to_value(&next.meta).unwrap(),
        serde_json::to_value(&first.meta).unwrap()
    );
    assert!(next.next_cursor.is_none());
    assert_eq!(
        db.filter_options("main", &req, EpochMs::new(9999).unwrap())
            .unwrap_err()
            .code,
        ErrorCode::SnapshotExpired
    );
    let fresh = fetch(&db, &request(FacetDimension::Models, 200));
    assert_eq!(fresh.options.len(), 3);
    assert_ne!(fresh.meta.data_revision, first.meta.data_revision);
    assert_ne!(fresh.meta.price_revision, first.meta.price_revision);
}

#[test]
fn bad_input_and_corrupt_keys_never_occupy_slots() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    let mut req = request(FacetDimension::Projects, 0);
    assert_eq!(
        db.filter_options("main", &req, EpochMs::new(1).unwrap())
            .unwrap_err()
            .code,
        ErrorCode::InvalidQuery
    );
    req.query.page_size = 200;
    req.cursor = Some("a".repeat(151));
    assert_eq!(
        db.filter_options("main", &req, EpochMs::new(1).unwrap())
            .unwrap_err()
            .code,
        ErrorCode::CursorInvalid
    );
    req.cursor = None;
    let oversized = "p".repeat(257);
    db.write(move |conn| {
        conn.execute(
            "INSERT INTO projects VALUES(?1,'synthetic','label',NULL,1)",
            [&oversized],
        )?;
        Ok(())
    })
    .unwrap();
    extra(
        &db,
        "bad",
        1100,
        7,
        (None, None),
        None,
        Some(&"p".repeat(257)),
    );
    for _ in 0..4 {
        assert_eq!(
            db.filter_options("main", &req, EpochMs::new(1).unwrap())
                .unwrap_err()
                .code,
            ErrorCode::DbCorrupt
        );
    }
    assert_eq!(
        fetch(&db, &request(FacetDimension::Models, 200))
            .options
            .len(),
        1
    );
}
