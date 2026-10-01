use super::super::tests::{event, install, session};
use super::*;
use crate::{
    batch::tests::{fixture, setup},
    query::tests::{filter, ids},
};

fn request(key: &str) -> SessionBundleRequest {
    SessionBundleRequest {
        session_key: key.into(),
        filter: filter(),
        price_basis: token_pulse_core::protocol::PriceBasis::EventTime {},
    }
}
fn fetch(db: &Database, request: &SessionBundleRequest) -> SessionBundle {
    db.session_bundle(request, EpochMs::new(2000).unwrap(), "detail-test")
        .unwrap()
}
#[test]
fn consumption_is_separate_from_lifetime_relations_classifications_and_context() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    session(&db, "child", Some("session"));
    event(&db, "child-event", "child", 2000, 50);
    db.write(|conn| {
        conn.execute("INSERT INTO context_snapshots VALUES('context','ledger-child','child-event',12000,'M',9007199254740993,NULL,'{}','\"confirmed\"')", [])?;
        conn.execute("INSERT INTO pending_usage VALUES('inherited','ledger-child','observation','inherited','inherited_prefix',NULL,'{\"private\":\"must not leave backend\"}')", [])?;
        conn.execute("INSERT INTO pending_usage VALUES('duplicate','ledger-child','observation','duplicate','verified_mirror',NULL,'{}')", [])?;
        Ok(())
    }).unwrap();
    let detail = fetch(&db, &request("child"));
    assert_eq!(detail.summary.total_tokens.as_str(), "50");
    assert_eq!(detail.identity.parent_key.as_deref(), Some("session"));
    assert_eq!(detail.classifications.len(), 2);
    assert!(
        detail
            .classifications
            .iter()
            .all(|c| c.observation_count.as_str() == "1")
    );
    assert_eq!(
        detail
            .latest_selected_activity
            .as_ref()
            .unwrap()
            .occurred_at_ms
            .value(),
        2000
    );
    assert_eq!(
        detail
            .latest_context
            .context_tokens
            .as_ref()
            .unwrap()
            .as_str(),
        "9007199254740993"
    );
    assert!(detail.latest_context.percentage.is_none());
    assert!(
        !serde_json::to_string(&detail)
            .unwrap()
            .contains("must not leave backend")
    );
    let parent = fetch(&db, &request("session"));
    assert_eq!(parent.child_count.as_str(), "1");
    assert_eq!(parent.children[0].session_key, "child");
    let mut req = request("child");
    req.filter.range.start_ms = EpochMs::new(3000).unwrap();
    let empty = fetch(&db, &req);
    assert_eq!(empty.summary.total_tokens.as_str(), "0");
    assert!(empty.summary.input_total.value.is_none());
    assert!(empty.latest_selected_activity.is_none());
    assert_eq!(empty.classifications.len(), 2);
    assert_eq!(empty.latest_context.observed_at_ms.unwrap().value(), 12000);
    req.filter = filter();
    req.filter.sessions = ids(&["session"], false);
    assert_eq!(fetch(&db, &req).summary.total_tokens.as_str(), "0");
    req.filter = filter();
    req.filter.sources = ids(&["missing"], false);
    assert_eq!(fetch(&db, &req).summary.total_tokens.as_str(), "0");
    assert_eq!(
        db.session_bundle(&request("missing"), EpochMs::new(1).unwrap(), "detail")
            .unwrap_err()
            .code,
        ErrorCode::InvalidQuery
    );
}

#[test]
fn all_detail_components_keep_the_pinned_transaction_after_writer_changes() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    session(&db, "child", Some("session"));
    event(&db, "old", "child", 1000, 50);
    let req = request("child");
    let old = db.snapshot(|tx, revision| {
        event(&db, "new", "child", 2000, 70); install(&db, 1_000_000_000);
        db.write(|conn| {
            conn.execute("UPDATE sessions SET provider_session_id='changed',parent_key=NULL WHERE session_key='child'", [])?;
            conn.execute("INSERT INTO context_snapshots VALUES('context','ledger-child','new',13000,'M',123,200,'{}','\"confirmed\"')", [])?;
            conn.execute("INSERT INTO pending_usage VALUES('pending','ledger-child','new','pending','lineage_pending',NULL,'{}')", [])?;
            Ok(())
        }).unwrap();
        bundle(tx, revision, &req, EpochMs::new(1).unwrap(), "pinned")
    }).unwrap();
    assert_eq!(old.summary.total_tokens.as_str(), "50");
    assert_eq!(old.identity.display_name, "Name-child");
    assert_eq!(old.identity.parent_key.as_deref(), Some("session"));
    assert!(old.latest_context.context_tokens.is_none() && old.classifications.is_empty());
    assert_eq!(old.pricing.unpriced_total_tokens.as_str(), "50");
    assert_eq!(old.meta.price_revision.as_str(), "0");
    let fresh = fetch(&db, &req);
    assert_eq!(fresh.summary.total_tokens.as_str(), "120");
    assert_eq!(fresh.identity.display_name, "changed");
    assert!(fresh.identity.parent_key.is_none());
    assert_eq!(fresh.classifications.len(), 1);
    assert_eq!(fresh.latest_context.context_tokens.unwrap().as_str(), "123");
    assert_eq!(
        fresh.pricing.currencies[0]
            .estimated_cost
            .as_ref()
            .unwrap()
            .as_str(),
        "0.000120000000000"
    );
}

#[test]
fn child_list_is_explicitly_bounded_and_invalid_evidence_never_becomes_an_empty_success() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    for i in 0..101 {
        session(&db, &format!("child-{i:03}"), Some("session"));
    }
    let parent = fetch(&db, &request("session"));
    assert_eq!(parent.child_count.as_str(), "101");
    assert_eq!(parent.children.len(), 100);
    assert!(parent.children_truncated);
    assert_eq!(parent.children[0].session_key, "child-000");
    assert_eq!(parent.children[99].session_key, "child-099");
    db.write(|conn| {
        conn.execute("INSERT INTO pending_usage VALUES('private','ledger','observation','inherited','unsafe details',NULL,'{}')", [])?; Ok(())
    }).unwrap();
    assert_eq!(
        db.session_bundle(&request("session"), EpochMs::new(1).unwrap(), "detail")
            .unwrap_err()
            .code,
        ErrorCode::DbCorrupt
    );
}

#[test]
fn aliases_resolve_inside_the_snapshot_and_mirror_children_are_not_extra_relationships() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    session(&db, "parent-alias", None);
    session(&db, "child", Some("parent-alias"));
    session(&db, "child-alias", Some("session"));
    db.create_job(
        "proof".into(),
        token_pulse_core::jobs::JobRequest {
            kind: token_pulse_core::protocol::JobKind::Rebuild,
            scope: token_pulse_core::jobs::JobScope::All {},
            request_key: "proof".into(),
        },
        1,
    )
    .unwrap();
    db.write(|conn| {
        conn.execute(
            "INSERT INTO session_aliases VALUES('parent-alias','session','proof')",
            [],
        )?;
        conn.execute(
            "INSERT INTO session_aliases VALUES('child-alias','child','proof')",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    let mut req = request("parent-alias");
    req.filter.sessions = ids(&["parent-alias"], false);
    let parent = fetch(&db, &req);
    assert_eq!(parent.identity.session_key, "session");
    assert_eq!(parent.summary.total_tokens.as_str(), "110");
    assert_eq!(parent.child_count.as_str(), "1");
    assert_eq!(parent.children[0].session_key, "child");
    assert_eq!(parent.children[0].parent_key.as_deref(), Some("session"));
}
