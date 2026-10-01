use super::*;
use crate::{
    SessionRegistration,
    batch::tests::{fixture, setup},
    query::tests::{extra, filter, ids},
};
use rusqlite::params;
use token_pulse_core::{
    pricing::{PriceRuleDraft, PriceRuleMutation},
    query::model_key,
};

fn request(sort: SessionSort, size: u16) -> SessionsRequest {
    SessionsRequest {
        query: SessionsQuery {
            filter: filter(),
            price_basis: token_pulse_core::protocol::PriceBasis::EventTime {},
            sort,
            page_size: size,
        },
        cursor: None,
    }
}
fn fetch(db: &Database, req: &SessionsRequest) -> SessionsPage {
    db.query_sessions("main", req, EpochMs::new(1234).unwrap())
        .unwrap()
}
fn session(db: &Database, key: &str, parent: Option<&str>) {
    db.ensure_session(SessionRegistration {
        session_key: key.into(),
        provider_session_id: Some(format!("Name-{key}")),
        parent_key: parent.map(str::to_owned),
        parent_provider_id: parent.map(|key| format!("Name-{key}")),
        created_at_ms: None,
        ledger_id: format!("ledger-{key}"),
        registered_at_ms: 1,
    })
    .unwrap();
}
fn event(db: &Database, id: &str, key: &str, time: i64, total: i64) {
    extra(
        db,
        id,
        time,
        total,
        (Some("M"), Some("P")),
        Some("turn"),
        None,
    );
    let id = id.to_owned();
    let key = key.to_owned();
    db.write(move |conn| {
        conn.execute("UPDATE observations SET session_key=?1 WHERE observation_id=?2", params![key, id])?;
        conn.execute("UPDATE usage_events SET ledger_id=?1,input_tokens_total=total_tokens,cached_input_tokens=0,output_tokens_total=0,reasoning_output_tokens=0 WHERE event_id=?2", params![format!("ledger-{key}"), id])?;
        conn.execute("UPDATE app_state SET data_revision=data_revision+1", [])?;
        Ok(())
    }).unwrap();
}
fn install(db: &Database, rate: i128) {
    db.mutate_price_rule(
        PriceRuleMutation::Create {
            draft: PriceRuleDraft {
                provider: "P".into(),
                model_exact: "M".into(),
                source_id: None,
                currency: "USD".into(),
                effective_from_ms: EpochMs::new(0).unwrap(),
                effective_to_ms: None,
                priority: 0,
                input_rate_atoms: DecimalInt::from_nonnegative(rate).unwrap(),
                cached_rate_atoms: Some(DecimalInt::from_nonnegative(rate).unwrap()),
                output_rate_atoms: DecimalInt::from_nonnegative(rate).unwrap(),
                origin_reference: Some("synthetic fixture".into()),
            },
        },
        0,
        1,
    )
    .unwrap();
}

#[test]
fn exact_total_and_latest_keysets_include_ties_and_do_not_count_requests_as_turns() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    session(&db, "a", None);
    session(&db, "b", None);
    event(&db, "a1", "a", 2000, i64::MAX);
    event(&db, "a2", "a", 2000, i64::MAX);
    event(&db, "b1", "b", 2000, i64::MAX);
    for sort in [SessionSort::TotalDesc, SessionSort::LatestDesc] {
        let mut req = request(sort, 1);
        let first = fetch(&db, &req);
        assert_eq!(first.sessions[0].session_key, "a");
        assert_eq!(
            first.sessions[0].summary.total_tokens.as_str(),
            "18446744073709551614"
        );
        assert_eq!(first.sessions[0].summary.usage_event_count.as_str(), "2");
        assert_eq!(
            first.sessions[0]
                .summary
                .reliable_turn_count
                .as_ref()
                .unwrap()
                .as_str(),
            "1"
        );
        assert_eq!(first.summary.total_tokens.as_str(), "27670116110564327531");
        assert_eq!(first.summary.session_count.as_str(), "3");
        req.cursor = first.next_cursor;
        let second = fetch(&db, &req);
        assert_eq!(second.sessions[0].session_key, "b");
        assert_eq!(
            serde_json::to_value(&second.meta).unwrap(),
            serde_json::to_value(&first.meta).unwrap()
        );
        assert_eq!(second.summary.total_tokens, first.summary.total_tokens);
        req.cursor = second.next_cursor;
        let third = fetch(&db, &req);
        assert_eq!(third.sessions[0].session_key, "session");
        assert!(third.sessions[0].summary.reliable_turn_count.is_none());
        assert!(!third.sessions[0].summary.reliable_turns_complete);
        assert!(third.next_cursor.is_none());
        assert_eq!(
            db.query_sessions("main", &req, EpochMs::new(2000).unwrap())
                .unwrap_err()
                .code,
            ErrorCode::SnapshotExpired
        );
    }
}

#[test]
fn selected_event_attribution_is_separate_from_latest_context_and_registered_relations() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    session(&db, "child", Some("session"));
    session(&db, "outside", Some("session"));
    event(&db, "early", "child", 1000, 20);
    event(&db, "late", "child", 2000, 30);
    event(&db, "outside", "outside", 5000, 40);
    db.write(|conn| {
        conn.execute("INSERT INTO projects VALUES('project','x','Actual label',NULL,1)", [])?;
        conn.execute("UPDATE usage_events SET project_id='project' WHERE event_id='late'", [])?;
        conn.execute("INSERT INTO context_snapshots VALUES('context','ledger-child','late',12000,'Other model',9007199254740993,NULL,'{}','\"confirmed\"')", [])?;
        Ok(())
    }).unwrap();
    let page = fetch(&db, &request(SessionSort::LatestDesc, 200));
    assert_eq!(page.sessions.len(), 2);
    let child = &page.sessions[0];
    assert_eq!(child.summary.total_tokens.as_str(), "50");
    assert_eq!(child.latest_at_ms.value(), 2000);
    assert_eq!(child.latest_project_name.as_deref(), Some("Actual label"));
    assert_eq!(child.latest_model.as_deref(), Some("M"));
    assert_eq!(child.parent_key.as_deref(), Some("session"));
    assert_eq!(
        child.parent_display_name.as_deref(),
        Some("provider-session")
    );
    assert_eq!(
        child
            .latest_context
            .context_tokens
            .as_ref()
            .unwrap()
            .as_str(),
        "9007199254740993"
    );
    assert!(
        child.latest_context.model_context_window.is_none()
            && child.latest_context.percentage.is_none()
    );
    assert_eq!(child.latest_context.observed_at_ms.unwrap().value(), 12000);
    assert_eq!(page.sessions[1].child_count.as_str(), "2"); // Across dates, explicitly not selected consumption.
    assert!(page.sessions[1].latest_context.context_tokens.is_none());
    let mut req = request(SessionSort::LatestDesc, 200);
    req.query.filter.projects = ids(&["project"], false);
    assert_eq!(fetch(&db, &req).summary.total_tokens.as_str(), "30");
    req.query.filter.models = ids(&[&model_key(Some("P"), Some("M")).unwrap()], false);
    assert_eq!(fetch(&db, &req).summary.total_tokens.as_str(), "30");
    req.query.filter.sources = ids(&["missing"], false);
    let empty = fetch(&db, &req);
    assert!(empty.sessions.is_empty() && empty.next_cursor.is_none());
    assert_eq!(empty.summary.total_tokens.as_str(), "0");
    assert!(empty.summary.input_total.value.is_none());
}

#[test]
fn writer_changes_cannot_replace_old_paged_facts_prices_context_or_relations() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    session(&db, "a", None);
    session(&db, "b", Some("session"));
    event(&db, "a", "a", 3000, 100);
    event(&db, "b", "b", 2000, 50);
    let mut req = request(SessionSort::LatestDesc, 1);
    let first = fetch(&db, &req);
    req.cursor = first.next_cursor.clone();
    event(&db, "new-b", "b", 4000, 70);
    install(&db, 1_000_000_000);
    db.write(|conn| {
        conn.execute("UPDATE sessions SET provider_session_id='changed',parent_key='a' WHERE session_key='b'", [])?;
        conn.execute("INSERT INTO context_snapshots VALUES('new','ledger-b','b',13000,'M',123,200,'{}','\"confirmed\"')", [])?;
        Ok(())
    }).unwrap();
    let old = fetch(&db, &req);
    assert_eq!(old.sessions[0].session_key, "b");
    assert_eq!(old.sessions[0].display_name, "Name-b");
    assert_eq!(old.sessions[0].parent_key.as_deref(), Some("session"));
    assert_eq!(old.sessions[0].summary.total_tokens.as_str(), "50");
    assert_eq!(old.sessions[0].pricing.unpriced_total_tokens.as_str(), "50");
    assert!(
        old.sessions[0].pricing.currencies.is_empty()
            && old.sessions[0].latest_context.context_tokens.is_none()
    );
    assert_eq!(old.meta.price_revision.as_str(), "0");
    assert_eq!(old.meta.data_revision, first.meta.data_revision);
    let fresh = fetch(&db, &request(SessionSort::LatestDesc, 200));
    assert_eq!(fresh.sessions[0].session_key, "b");
    assert_eq!(fresh.sessions[0].summary.total_tokens.as_str(), "120");
    assert_eq!(fresh.sessions[0].display_name, "changed");
    assert_eq!(fresh.sessions[0].parent_key.as_deref(), Some("a"));
    assert_eq!(
        fresh.sessions[0].pricing.currencies[0]
            .estimated_cost
            .as_ref()
            .unwrap()
            .as_str(),
        "0.000120000000000"
    );
    assert_eq!(
        fresh.sessions[0]
            .latest_context
            .context_tokens
            .as_ref()
            .unwrap()
            .as_str(),
        "123"
    );
    assert_eq!(fresh.meta.price_revision.as_str(), "1");
    assert_ne!(fresh.meta.data_revision, first.meta.data_revision);
    db.close_sessions("main", &req).unwrap();
}

#[test]
fn cursors_bind_owner_filter_sort_price_basis_and_page_size_and_closing_is_idempotent() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    session(&db, "a", None);
    event(&db, "a", "a", 2000, 50);
    let mut req = request(SessionSort::LatestDesc, 1);
    req.cursor = fetch(&db, &req).next_cursor;
    assert_eq!(
        db.query_sessions("mini", &req, EpochMs::new(1000).unwrap())
            .unwrap_err()
            .code,
        ErrorCode::CursorInvalid
    );
    for field in 0..4 {
        let mut changed = req.clone();
        match field {
            0 => changed.query.sort = SessionSort::TotalDesc,
            1 => changed.query.page_size = 2,
            2 => changed.query.filter.sources = ids(&["source"], false),
            _ => {
                changed.query.price_basis = token_pulse_core::protocol::PriceBasis::SpecifiedTime {
                    specified_at_ms: EpochMs::new(1000).unwrap(),
                }
            }
        }
        assert_eq!(
            db.query_sessions("main", &changed, EpochMs::new(1000).unwrap())
                .unwrap_err()
                .code,
            ErrorCode::CursorInvalid
        );
        assert_eq!(
            db.close_sessions("main", &changed).unwrap_err().code,
            ErrorCode::CursorInvalid
        );
    }
    db.close_sessions("main", &req).unwrap();
    db.close_sessions("main", &req).unwrap();
    assert_eq!(
        db.query_sessions("main", &req, EpochMs::new(1000).unwrap())
            .unwrap_err()
            .code,
        ErrorCode::SnapshotExpired
    );
    for _ in 0..5 {
        assert!(
            fetch(&db, &request(SessionSort::LatestDesc, 200))
                .next_cursor
                .is_none()
        );
    }
    let mut bad = request(SessionSort::LatestDesc, 201);
    assert_eq!(
        db.query_sessions("main", &bad, EpochMs::new(1000).unwrap())
            .unwrap_err()
            .code,
        ErrorCode::InvalidQuery
    );
    bad.query.page_size = 50;
    bad.cursor = Some("a".repeat(151));
    assert_eq!(
        db.query_sessions("main", &bad, EpochMs::new(1000).unwrap())
            .unwrap_err()
            .code,
        ErrorCode::CursorInvalid
    );
}
