use super::*;
use crate::{
    batch::tests::{fixture, setup},
    query::tests::filter,
};

#[test]
fn latest_context_is_not_consumption_and_never_fills_missing_capacity_or_counts() {
    let (_dir, db) = setup();
    let empty = db.latest_context("session").unwrap();
    assert!(
        empty.context_tokens.is_none()
            && empty.model_context_window.is_none()
            && empty.percentage.is_none()
            && empty.observed_at_ms.is_none()
    );
    assert_eq!(empty.quality, "unknown");
    db.commit(fixture()).unwrap();
    db.write(|conn| { conn.execute("INSERT INTO context_snapshots VALUES('context','ledger','observation',2000,NULL,1000,2000,'{}','\"confirmed\"')",[])?; Ok(()) }).unwrap();
    let c = db.latest_context("session").unwrap();
    assert_eq!(c.context_tokens.unwrap().as_str(), "1000");
    assert_eq!(c.percentage, Some(50.0));
    assert_eq!(c.quality, "confirmed");
    assert_eq!(c.observed_at_ms.unwrap().value(), 2000);
    assert_eq!(
        db.usage_totals(&filter()).unwrap().total_tokens.as_str(),
        "110"
    );
    db.write(|conn| { conn.execute("INSERT INTO context_snapshots VALUES('later','ledger','observation',3000,NULL,NULL,2000,'{}','\"pending\"')",[])?; Ok(()) }).unwrap();
    let c = db.latest_context("session").unwrap();
    assert!(c.context_tokens.is_none() && c.percentage.is_none());
    assert_eq!(c.model_context_window.unwrap().as_str(), "2000");
    assert_eq!(c.quality, "pending");
}

#[test]
fn context_alias_and_latest_value_remain_in_one_real_snapshot_with_exact_large_tokens() {
    use crate::SessionRegistration;
    use token_pulse_core::{
        jobs::{JobRequest, JobScope},
        protocol::JobKind,
    };
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.ensure_session(SessionRegistration {
        session_key: "alias".into(),
        provider_session_id: None,
        parent_key: None,
        parent_provider_id: None,
        created_at_ms: None,
        ledger_id: "alias-ledger".into(),
        registered_at_ms: 1,
    })
    .unwrap();
    db.create_job(
        "proof".into(),
        JobRequest {
            kind: JobKind::Rebuild,
            scope: JobScope::All {},
            request_key: "proof".into(),
        },
        1,
    )
    .unwrap();
    db.write(|conn| {
        conn.execute("INSERT INTO session_aliases VALUES('alias','session','proof')",[])?;
        conn.execute("INSERT INTO context_snapshots VALUES('context','ledger','observation',2000,NULL,9007199254740993,10000000000000000,'{}','\"confirmed\"')",[])?; Ok(())
    }).unwrap();
    db.snapshot(|tx, _| {
        db.write(|conn| {
            conn.execute(
                "UPDATE context_snapshots SET context_tokens=0,observed_at_ms=3000",
                [],
            )?;
            conn.execute("DELETE FROM session_aliases", [])?;
            Ok(())
        })
        .unwrap();
        let c = latest_context(tx, "alias")?;
        assert_eq!(c.context_tokens.unwrap().as_str(), "9007199254740993");
        assert!((c.percentage.unwrap() - 90.07199254740992).abs() < 0.000000000001);
        assert_eq!(c.observed_at_ms.unwrap().value(), 2000);
        Ok(())
    })
    .unwrap();
    assert!(db.latest_context("alias").unwrap().context_tokens.is_none());
    assert_eq!(db.latest_context("session").unwrap().percentage, Some(0.0));
}

#[test]
fn context_above_capacity_retains_percentage_unknown_window_and_rejects_corruption() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.write(|conn| {
        conn.execute("INSERT INTO context_snapshots VALUES('a','ledger','observation',2000,NULL,30,20,'{}','\"confirmed\"')",[])?;
        conn.execute("INSERT INTO context_snapshots VALUES('z','ledger','observation',2000,NULL,0,NULL,'{}','\"pending\"')",[])?; Ok(())
    }).unwrap();
    assert_eq!(
        db.latest_context("session").unwrap().percentage,
        Some(150.0)
    );
    db.write(|conn| {
        conn.execute("DELETE FROM context_snapshots WHERE context_id='a'", [])?;
        Ok(())
    })
    .unwrap();
    let c = db.latest_context("session").unwrap();
    assert_eq!(c.context_tokens.unwrap().as_str(), "0");
    assert!(c.model_context_window.is_none() && c.percentage.is_none());
    db.write(|conn| {
        conn.execute(
            "UPDATE context_snapshots SET quality_json='[\"invented\"]'",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        db.latest_context("session").unwrap_err().code,
        ErrorCode::DbCorrupt
    );
    for invalid in ["", "missing", "session') OR 1=1 --", "\n"] {
        assert_eq!(
            db.latest_context(invalid).unwrap_err().code,
            ErrorCode::InvalidQuery
        );
    }
}
