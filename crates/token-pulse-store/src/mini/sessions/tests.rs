use super::*;
use crate::SessionRegistration;
use token_pulse_core::{
    privacy::{DisplayPolicyStamp, PrivacyState, PrivateResponse},
    settings::TimezoneMutation,
};
fn at(value: i64) -> EpochMs {
    EpochMs::new(value).unwrap()
}
fn setup() -> (tempfile::TempDir, Database) {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path()).unwrap();
    (directory, database)
}
fn initialize(db: &Database) {
    db.mutate_display_timezone(
        TimezoneMutation::Initialize {
            system_timezone: "UTC".into(),
        },
        at(100),
    )
    .unwrap();
}
fn session(db: &Database, key: &str, name: &str) {
    db.ensure_session(SessionRegistration {
        session_key: key.into(),
        provider_session_id: Some(name.into()),
        parent_key: None,
        parent_provider_id: None,
        created_at_ms: None,
        ledger_id: format!("ledger-{key}"),
        registered_at_ms: 1,
    })
    .unwrap();
}
fn request(size: u16) -> MiniSessionsRequest {
    MiniSessionsRequest {
        query: MiniSessionsQuery {
            search: "".into(),
            page_size: size,
        },
        cursor: None,
    }
}
#[test]
fn canonical_registered_sessions_with_no_events_keep_a_frozen_keyset_during_writes() {
    let (_dir, db) = setup();
    initialize(&db);
    session(&db, "a", "同名");
    session(&db, "b", "同名");
    session(&db, "a-mirror", "mirror");
    db.create_job(
        "synthetic-proof".into(),
        token_pulse_core::jobs::JobRequest {
            kind: token_pulse_core::protocol::JobKind::Rebuild,
            scope: token_pulse_core::jobs::JobScope::All {},
            request_key: "synthetic-proof".into(),
        },
        1,
    )
    .unwrap();
    db.write(|conn| {
        conn.execute(
            "INSERT INTO session_aliases VALUES('a-mirror','a','synthetic-proof')",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    let mut req = request(1);
    let first = db.mini_sessions("mini", &req, at(1234)).unwrap();
    assert_eq!(first.options[0].session_key, "a");
    assert_eq!(first.options[0].display_name, "同名");
    req.cursor = first.next_cursor;
    session(&db, "a-between", "added after snapshot");
    db.write(|conn| { conn.execute("UPDATE sessions SET provider_session_id='changed after snapshot' WHERE session_key='b'", [])?; Ok(()) }).unwrap();
    let next = db.mini_sessions("mini", &req, at(9999)).unwrap();
    assert_eq!(next.options.len(), 1);
    assert_eq!(next.options[0].session_key, "b");
    assert_eq!(next.options[0].display_name, "同名");
    assert_eq!(
        serde_json::to_value(next.meta).unwrap(),
        serde_json::to_value(first.meta).unwrap()
    );
    assert!(next.next_cursor.is_none());
    for _ in 0..5 {
        let fresh = db.mini_sessions("mini", &request(100), at(2222)).unwrap();
        assert_eq!(
            fresh
                .options
                .iter()
                .map(|o| o.session_key.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "a-between", "b"]
        );
        assert_eq!(fresh.options[2].display_name, "changed after snapshot");
    }
}
#[test]
fn cursor_binds_window_search_and_page_size_and_close_releases_only_the_original_lease() {
    let (_dir, db) = setup();
    initialize(&db);
    session(&db, "a", "A");
    session(&db, "b", "B");
    let mut req = request(1);
    let first = db.mini_sessions("mini", &req, at(1000)).unwrap();
    req.cursor = first.next_cursor;
    assert_eq!(
        db.mini_sessions("main", &req, at(1001)).unwrap_err().code,
        ErrorCode::CursorInvalid
    );
    let mut different = req.clone();
    different.query.search = "B".into();
    assert_eq!(
        db.mini_sessions("mini", &different, at(1001))
            .unwrap_err()
            .code,
        ErrorCode::CursorInvalid
    );
    different = req.clone();
    different.query.page_size = 2;
    assert_eq!(
        db.close_mini_sessions("mini", &different).unwrap_err().code,
        ErrorCode::CursorInvalid
    );
    assert_eq!(
        db.close_mini_sessions("main", &req).unwrap_err().code,
        ErrorCode::CursorInvalid
    );
    let mut tampered = req.clone();
    let cursor = tampered.cursor.as_mut().unwrap();
    cursor.replace_range(50..51, if &cursor[50..51] == "A" { "B" } else { "A" });
    assert_eq!(
        db.mini_sessions("mini", &tampered, at(1001))
            .unwrap_err()
            .code,
        ErrorCode::CursorInvalid
    );
    db.close_mini_sessions("mini", &req).unwrap();
    db.close_mini_sessions("mini", &req).unwrap();
    assert_eq!(
        db.mini_sessions("mini", &req, at(1001)).unwrap_err().code,
        ErrorCode::SnapshotExpired
    );
    assert_eq!(
        db.mini_sessions("mini", &request(100), at(1002))
            .unwrap()
            .options
            .len(),
        2
    );
}
#[test]
fn literal_unicode_search_validation_and_unknown_settings_release_failed_leases() {
    let (_dir, db) = setup();
    session(&db, "a", "混合中文%_') OR 1=1--");
    session(&db, "b", "other");
    assert_eq!(
        db.mini_sessions("mini", &request(100), at(1234))
            .unwrap_err()
            .code,
        ErrorCode::InvalidQuery
    );
    initialize(&db);
    let mut req = request(100);
    req.query.search = "中文%_') or 1=1--".into();
    let page = db.mini_sessions("mini", &req, at(1234)).unwrap();
    assert_eq!(page.options.len(), 1);
    assert_eq!(page.options[0].session_key, "a");
    req.query.search = "absent%".into();
    assert!(
        db.mini_sessions("mini", &req, at(1234))
            .unwrap()
            .options
            .is_empty()
    );
    for query in [
        MiniSessionsQuery {
            search: "\n".into(),
            page_size: 10,
        },
        MiniSessionsQuery {
            search: "中".repeat(257),
            page_size: 10,
        },
        MiniSessionsQuery {
            search: "".into(),
            page_size: 0,
        },
        MiniSessionsQuery {
            search: "".into(),
            page_size: 101,
        },
    ] {
        req.query = query;
        assert_eq!(
            db.mini_sessions("mini", &req, at(1234)).unwrap_err().code,
            ErrorCode::InvalidQuery
        );
    }
    assert!(
        serde_json::from_str::<MiniSessionsRequest>(
            r#"{"query":{"search":"","page_size":10,"source_path":"private"},"cursor":null}"#
        )
        .is_err()
    );
    req = request(10);
    req.cursor = Some("query-snapshot-id".into());
    assert_eq!(
        db.close_mini_sessions("mini", &req).unwrap_err().code,
        ErrorCode::CursorInvalid
    );
}
#[test]
fn candidate_pages_serialize_with_latest_privacy_without_changing_ids_or_snapshot_capability() {
    let (_dir, db) = setup();
    initialize(&db);
    session(&db, "a", "private-provider-A");
    session(&db, "b", "private-provider-B");
    let page = db.mini_sessions("mini", &request(1), at(1234)).unwrap();
    let policy = PrivacyState::new(DisplayPolicyStamp {
        settings_revision: DecimalInt::parse("1").unwrap(),
        privacy: false,
    });
    let response = PrivateResponse::new("private-page".into(), page.clone(), policy.clone());
    policy
        .publish(DisplayPolicyStamp {
            settings_revision: DecimalInt::parse("2").unwrap(),
            privacy: true,
        })
        .unwrap();
    let value = serde_json::to_value(response).unwrap();
    assert!(!value.to_string().contains("private-provider"));
    assert_eq!(value["data"]["options"][0]["session_key"], "a");
    assert_eq!(value["data"]["next_cursor"], page.next_cursor.unwrap());
    assert_eq!(
        value["data"]["meta"],
        serde_json::to_value(page.meta).unwrap()
    );
    assert_eq!(page.options[0].display_name, "private-provider-A");
}
