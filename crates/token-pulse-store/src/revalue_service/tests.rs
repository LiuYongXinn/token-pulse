use super::*;
use crate::{
    batch::tests::{fixture, setup},
    query::tests::{extra, filter},
};
use std::{sync::atomic::AtomicUsize, time::Instant};
use token_pulse_core::{
    jobs::JobScope, numeric::DecimalInt, pricing::revalue::PriceRevalueState, protocol::PriceBasis,
};

fn wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !predicate() {
        assert!(Instant::now() < deadline, "price worker deadline exceeded");
        thread::sleep(Duration::from_millis(10));
    }
}
fn populated() -> (tempfile::TempDir, Database) {
    let (dir, db) = setup();
    db.commit(fixture()).unwrap();
    (dir, db)
}
fn start(db: &Database) -> RevalueService {
    RevalueService::start_with_interval(db.clone(), Duration::from_millis(50), Arc::new(|| {}))
        .unwrap()
}
fn complete(db: &Database) {
    wait_until(|| {
        db.price_revalue_status()
            .unwrap()
            .latest_job
            .is_some_and(|j| j.state == PriceRevalueState::Succeeded)
    });
}
fn manual(key: &str) -> PriceRevalueRequest {
    PriceRevalueRequest {
        scope: JobScope::All {},
        basis: PriceBasis::SpecifiedTime {
            specified_at_ms: token_pulse_core::numeric::EpochMs::new(1500).unwrap(),
        },
        expected_price_revision: DecimalInt::parse("0").unwrap(),
        request_key: key.into(),
    }
}
type Gate = (
    Arc<dyn Fn() + Send + Sync>,
    mpsc::Receiver<String>,
    mpsc::SyncSender<()>,
);
fn gate(db: &Database, processed: &str) -> Gate {
    let db = db.clone();
    let processed = processed.to_owned();
    let once = Arc::new(AtomicBool::new(false));
    let (entered, saw) = mpsc::sync_channel(1);
    let (release, resumed) = mpsc::sync_channel(1);
    let resumed = Mutex::new(resumed);
    let callback = Arc::new(move || {
        if once.load(Ordering::Acquire) {
            return;
        }
        if let Some(job) = db.price_revalue_status().unwrap().active_job {
            if job.state == PriceRevalueState::Running
                && job.processed_events.as_str() == processed
                && !once.swap(true, Ordering::AcqRel)
            {
                entered.send(job.job_id).unwrap();
                resumed
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
            }
        }
    });
    (callback, saw, release)
}
#[test]
fn startup_new_evidence_and_price_changes_fill_independent_caches() {
    let (_dir, db) = populated();
    let before=db.snapshot(|tx,r|Ok((r.data,r.price,r.settings,tx.query_row("SELECT committed_offset FROM file_generations WHERE file_generation_id='generation'",[],|r|r.get::<_,i64>(0))?))).unwrap();
    let service = start(&db);
    complete(&db);
    assert_eq!(
        db.price_revalue_status().unwrap().uncached_ledgers.as_str(),
        "0"
    );
    db.snapshot(|tx,r| {assert_eq!((r.data,r.price,r.settings,tx.query_row("SELECT committed_offset FROM file_generations WHERE file_generation_id='generation'",[],|r|r.get::<_,i64>(0))?),before);Ok(())}).unwrap();
    extra(&db, "new", 2000, 7, (None, None), None, None);
    service.wake();
    wait_until(|| db.price_revalue_status().unwrap().uncached_ledgers.as_str() == "0");
    assert_eq!(
        db.usage_totals(&filter()).unwrap().total_tokens.as_str(),
        "117"
    );
    db.write(|conn| {
        conn.execute(
            "UPDATE app_state SET price_revision=1 WHERE singleton=1",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    service.wake();
    wait_until(|| {
        db.price_revalue_status()
            .unwrap()
            .latest_job
            .is_some_and(|j| {
                j.state == PriceRevalueState::Succeeded && j.price_revision.as_str() == "1"
            })
    });
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row(
                "SELECT COUNT(*) FROM valuation_sets WHERE state='ready'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            3
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(service.last_error(), None);
    service.shutdown();
    service.shutdown();
}
#[test]
fn running_cancel_discards_unpublished_candidate_and_does_not_restart() {
    let (_dir, db) = populated();
    let (notify, entered, release) = gate(&db, "1");
    let service =
        RevalueService::start_with_interval(db.clone(), Duration::from_millis(50), notify).unwrap();
    let id = entered.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(
        service.cancel_job(id.clone()).unwrap(),
        CancelJobResult::Accepted
    );
    release.send(()).unwrap();
    wait_until(|| db.get_price_revalue_job(&id).unwrap().job.state == PriceRevalueState::Cancelled);
    assert!(db.enqueue_automatic_price_revalue(2000).unwrap().is_none());
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row(
                "SELECT COUNT(*) FROM valuation_sets WHERE state='ready'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            0
        );
        Ok(())
    })
    .unwrap();
    service.shutdown();
    assert_eq!(
        db.usage_totals(&filter()).unwrap().total_tokens.as_str(),
        "110"
    );
}
#[test]
fn orderly_stop_persists_interruption_and_restart_completes_missing_cache() {
    let (_dir, db) = populated();
    let (notify, entered, release) = gate(&db, "1");
    let service = Arc::new(
        RevalueService::start_with_interval(db.clone(), Duration::from_millis(50), notify).unwrap(),
    );
    let id = entered.recv_timeout(Duration::from_secs(5)).unwrap();
    let stop = service.clone();
    let stopped = thread::spawn(move || stop.shutdown());
    wait_until(|| service.stopping.load(Ordering::Acquire));
    release.send(()).unwrap();
    stopped.join().unwrap();
    assert_eq!(
        db.get_price_revalue_job(&id).unwrap().job.state,
        PriceRevalueState::Interrupted
    );
    drop(service);
    let service = start(&db);
    complete(&db);
    assert_eq!(
        db.price_revalue_status().unwrap().uncached_ledgers.as_str(),
        "0"
    );
    service.shutdown();
}
#[test]
fn revision_change_during_job_creates_separate_price_sets() {
    let (_dir, db) = populated();
    let (notify, entered, release) = gate(&db, "0");
    let service =
        RevalueService::start_with_interval(db.clone(), Duration::from_millis(50), notify).unwrap();
    let first = entered.recv_timeout(Duration::from_secs(5)).unwrap();
    db.write(|conn| {
        conn.execute(
            "UPDATE app_state SET price_revision=1 WHERE singleton=1",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    release.send(()).unwrap();
    wait_until(|| {
        db.price_revalue_status()
            .unwrap()
            .latest_job
            .is_some_and(|j| {
                j.state == PriceRevalueState::Succeeded && j.price_revision.as_str() == "1"
            })
    });
    assert_eq!(
        db.get_price_revalue_job(&first)
            .unwrap()
            .job
            .price_revision
            .as_str(),
        "0"
    );
    assert_eq!(
        db.get_price_revalue_job(&first).unwrap().job.state,
        PriceRevalueState::Succeeded
    );
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row(
                "SELECT COUNT(DISTINCT price_revision) FROM valuation_sets WHERE state='ready'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            2
        );
        Ok(())
    })
    .unwrap();
    service.shutdown();
}
#[test]
fn manual_time_job_is_idempotent_and_empty_work_is_truthful() {
    let (_dir, db) = setup();
    let service = start(&db);
    let first = service.start_job("manual".into(), manual("key")).unwrap();
    assert_eq!(first.total_events.as_str(), "0");
    assert_eq!(
        service
            .start_job("duplicate".into(), manual("key"))
            .unwrap()
            .job_id,
        "manual"
    );
    complete(&db);
    assert_eq!(
        db.get_price_revalue_job("manual")
            .unwrap()
            .job
            .completed_ledgers
            .as_str(),
        "1"
    );
    service.shutdown();
    assert_eq!(
        service
            .start_job("after-stop".into(), manual("stop"))
            .unwrap_err()
            .code,
        ErrorCode::JobInterrupted
    );
}
#[test]
fn candidate_failure_is_visible_and_old_ready_cache_is_preserved() {
    let (_dir, db) = populated();
    db.build_event_valuation("ledger", &PriceBasis::EventTime {}, 1)
        .unwrap();
    db.write(|conn| {conn.execute("UPDATE app_state SET price_revision=1 WHERE singleton=1",[])?;conn.execute_batch("CREATE TRIGGER reject_price_publish BEFORE UPDATE OF state ON valuation_sets WHEN NEW.state='ready' BEGIN SELECT RAISE(ABORT,'fixture'); END;")?;Ok(())}).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let notices = calls.clone();
    let service = RevalueService::start_with_interval(
        db.clone(),
        Duration::from_millis(50),
        Arc::new(move || {
            notices.fetch_add(1, Ordering::Relaxed);
        }),
    )
    .unwrap();
    wait_until(|| {
        db.price_revalue_status()
            .unwrap()
            .latest_job
            .is_some_and(|j| j.state == PriceRevalueState::Failed)
    });
    let latest = db.price_revalue_status().unwrap().latest_job.unwrap();
    assert_eq!(latest.error, Some(ErrorCode::DbWriteFailed));
    assert!(db.enqueue_automatic_price_revalue(10).unwrap().is_none());
    assert!(calls.load(Ordering::Relaxed) >= 3);
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row(
                "SELECT COUNT(*) FROM valuation_sets WHERE state='ready' AND price_revision=0",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            1
        );
        Ok(())
    })
    .unwrap();
    service.shutdown();
}
