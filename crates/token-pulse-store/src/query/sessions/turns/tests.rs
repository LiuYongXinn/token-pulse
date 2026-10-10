use super::super::tests::{event, install, session};
use super::*;
use crate::{
    batch::tests::{fixture, setup},
    query::tests::{filter, ids},
};
fn request(size: u16) -> TurnsRequest {
    TurnsRequest {
        query: TurnsQuery {
            session_key: "session".into(),
            filter: filter(),
            price_basis: token_pulse_core::protocol::PriceBasis::EventTime {},
            page_size: size,
        },
        cursor: None,
    }
}
fn fetch(db: &Database, req: &TurnsRequest) -> TurnsPage {
    db.query_turns("main", req, EpochMs::new(1000).unwrap())
        .unwrap()
}
#[test]
fn completion_times_are_whole_turn_metadata_and_keep_the_pinned_snapshot() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    turn_event(&db, "a", 3000, 100, Some("a"));
    turn_event(&db, "b", 2000, 50, Some("b"));
    let mut req = request(1);
    let first = fetch(&db, &req);
    assert!(first.turns[0].duration_ms.is_none());
    req.cursor = first.next_cursor;
    let before = db.usage_revision().unwrap();
    db.write(|conn| {
        conn.execute_batch("INSERT INTO turn_timing_records VALUES('generation',80,90,'provider-session','b',60000,1500); INSERT INTO turn_timing_records VALUES('generation',90,100,'provider-session','a',120000,NULL);")?;
        Ok(())
    }).unwrap();
    let pinned = fetch(&db, &req);
    assert_eq!(pinned.turns[0].turn_id, "b");
    assert!(pinned.turns[0].duration_ms.is_none());
    let fresh = fetch(&db, &request(20));
    assert_eq!(
        fresh.turns[1].duration_ms.as_ref().unwrap().as_str(),
        "60000"
    );
    assert_eq!(
        fresh.turns[1]
            .time_to_first_token_ms
            .as_ref()
            .unwrap()
            .as_str(),
        "1500"
    );
    let mut partial = request(20);
    partial.query.filter.range.start_ms = EpochMs::new(3000).unwrap();
    let scoped = fetch(&db, &partial);
    assert_eq!(scoped.turns.len(), 1);
    assert_eq!(
        scoped.turns[0].duration_ms.as_ref().unwrap().as_str(),
        "120000"
    );
    assert!(scoped.turns[0].time_to_first_token_ms.is_none());
    let after = db.usage_revision().unwrap();
    assert_eq!(after.data_revision, before.data_revision);
    assert_eq!(after.price_revision, before.price_revision);
    assert!(after.usage_view_revision.value() > before.usage_view_revision.value());
}
fn turn_event(db: &Database, id: &str, time: i64, total: i64, turn: Option<&str>) {
    crate::query::tests::extra(db, id, time, total, (Some("M"), Some("P")), turn, None);
    let id = id.to_owned();
    let turn = turn.map(str::to_owned);
    db.write(move |conn| {
        conn.execute(
            "UPDATE usage_events SET turn_id=?1,input_tokens_total=total_tokens,cached_input_tokens=0,output_tokens_total=0,reasoning_output_tokens=0 WHERE event_id=?2",
            rusqlite::params![turn, id],
        )?;
        Ok(())
    })
    .unwrap();
}
#[test]
fn exact_turn_grouping_never_counts_requests_as_turns_or_unknown_events_as_identified() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    turn_event(&db, "a1", 1000, i64::MAX, Some("a"));
    turn_event(&db, "a2", 3000, i64::MAX, Some("a"));
    turn_event(&db, "b", 3000, 10, Some("b"));
    turn_event(&db, "c", 2000, 12, Some("c"));
    turn_event(&db, "unknown", 2000, 7, None);
    turn_event(&db, "empty", 2000, 3, Some(""));
    session(&db, "other", None);
    event(&db, "other", "other", 4000, 100);
    let mut req = request(1);
    let first = fetch(&db, &req);
    assert_eq!(first.turns[0].turn_id, "a");
    assert_eq!(
        first.turns[0].summary.total_tokens.as_str(),
        "18446744073709551614"
    );
    assert_eq!(first.turns[0].summary.usage_event_count.as_str(), "2");
    assert_eq!(
        first.turns[0]
            .summary
            .reliable_turn_count
            .as_ref()
            .unwrap()
            .as_str(),
        "1"
    );
    assert!(first.turns[0].summary.reliable_turns_complete);
    assert_eq!(first.turns[0].first_at_ms.value(), 1000);
    assert_eq!(first.turns[0].last_at_ms.value(), 3000);
    assert_eq!(first.summary.total_tokens.as_str(), "18446744073709551756");
    assert_eq!(first.summary.usage_event_count.as_str(), "7");
    assert_eq!(
        first.summary.reliable_turn_count.as_ref().unwrap().as_str(),
        "3"
    );
    assert!(!first.summary.reliable_turns_complete);
    assert_eq!(first.unidentified_usage_event_count.as_str(), "3");
    req.cursor = first.next_cursor.clone();
    let second = fetch(&db, &req);
    assert_eq!(second.turns[0].turn_id, "b");
    assert_eq!(
        serde_json::to_value(&second.meta).unwrap(),
        serde_json::to_value(&first.meta).unwrap()
    );
    assert_eq!(second.summary.total_tokens, first.summary.total_tokens);
    req.cursor = second.next_cursor;
    let third = fetch(&db, &req);
    assert_eq!(third.turns[0].turn_id, "c");
    assert!(third.next_cursor.is_none());
    assert_eq!(
        db.query_turns("main", &req, EpochMs::new(1).unwrap())
            .unwrap_err()
            .code,
        ErrorCode::SnapshotExpired
    );
}
#[test]
fn writer_cannot_replace_paged_turn_membership_times_or_price() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    turn_event(&db, "a", 3000, 100, Some("a"));
    turn_event(&db, "b", 2000, 50, Some("b"));
    let mut req = request(1);
    let first = fetch(&db, &req);
    req.cursor = first.next_cursor;
    turn_event(&db, "new-b", 4000, 70, Some("b"));
    install(&db, 1_000_000_000);
    let old = fetch(&db, &req);
    assert_eq!(old.turns[0].turn_id, "b");
    assert_eq!(old.turns[0].summary.total_tokens.as_str(), "50");
    assert_eq!(old.turns[0].last_at_ms.value(), 2000);
    assert_eq!(old.turns[0].pricing.unpriced_total_tokens.as_str(), "50");
    assert_eq!(old.meta.price_revision.as_str(), "0");
    let fresh = fetch(&db, &request(200));
    assert_eq!(fresh.turns[0].turn_id, "b");
    assert_eq!(fresh.turns[0].summary.total_tokens.as_str(), "120");
    assert_eq!(
        fresh.turns[0].pricing.currencies[0]
            .estimated_cost
            .as_ref()
            .unwrap()
            .as_str(),
        "0.000120000000000"
    );
}
#[test]
fn filters_intersect_and_unknown_turns_keep_consumption_and_explicit_unknown_identity() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    let no_identity = fetch(&db, &request(200));
    assert!(no_identity.turns.is_empty());
    assert!(no_identity.summary.reliable_turn_count.is_none());
    assert_eq!(no_identity.summary.total_tokens.as_str(), "110");
    assert_eq!(no_identity.unidentified_usage_event_count.as_str(), "1");
    turn_event(&db, "a1", 1000, 20, Some("a"));
    turn_event(&db, "a2", 3000, 30, Some("a"));
    let mut req = request(200);
    req.query.filter.range.start_ms = EpochMs::new(3000).unwrap();
    let fragment = fetch(&db, &req);
    assert_eq!(fragment.turns[0].summary.total_tokens.as_str(), "30");
    assert_eq!(fragment.turns[0].first_at_ms.value(), 3000);
    for selection in [ids(&[], false), ids(&["other"], false)] {
        req.query.filter = filter();
        req.query.filter.sessions = selection;
        let empty = fetch(&db, &req);
        assert!(empty.turns.is_empty());
        assert_eq!(empty.summary.total_tokens.as_str(), "0");
    }
    req.query.filter = filter();
    req.query.filter.sources = ids(&["missing"], false);
    assert!(fetch(&db, &req).turns.is_empty());
}
#[test]
fn cursor_capability_binds_target_and_parameters_and_errors_release_readers() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    turn_event(&db, "a", 3000, 100, Some("a"));
    turn_event(&db, "b", 2000, 50, Some("b"));
    let mut req = request(1);
    req.cursor = fetch(&db, &req).next_cursor;
    assert_eq!(
        db.query_turns("other-owner", &req, EpochMs::new(1).unwrap())
            .unwrap_err()
            .code,
        ErrorCode::CursorInvalid
    );
    for bad in [
        TurnsQuery {
            session_key: "other".into(),
            ..req.query.clone()
        },
        TurnsQuery {
            page_size: 2,
            ..req.query.clone()
        },
        TurnsQuery {
            price_basis: token_pulse_core::protocol::PriceBasis::SpecifiedTime {
                specified_at_ms: EpochMs::new(1).unwrap(),
            },
            ..req.query.clone()
        },
    ] {
        assert_eq!(
            db.query_turns(
                "main",
                &TurnsRequest {
                    query: bad,
                    cursor: req.cursor.clone()
                },
                EpochMs::new(1).unwrap()
            )
            .unwrap_err()
            .code,
            ErrorCode::CursorInvalid
        );
    }
    db.close_turns("main", &req).unwrap();
    db.close_turns("main", &req).unwrap();
    db.write(|conn| {
        conn.execute(
            "UPDATE usage_events SET turn_id=char(10) WHERE event_id='a'",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    for _ in 0..6 {
        assert_eq!(
            db.query_turns("main", &request(200), EpochMs::new(1).unwrap())
                .unwrap_err()
                .code,
            ErrorCode::DbCorrupt
        );
    }
    db.write(|conn| {
        conn.execute("UPDATE usage_events SET turn_id='a' WHERE event_id='a'", [])?;
        Ok(())
    })
    .unwrap();
    assert_eq!(fetch(&db, &request(200)).turns.len(), 2);
}
