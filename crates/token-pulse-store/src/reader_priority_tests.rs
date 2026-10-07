use crate::{
    Database,
    batch::tests::{fixture, setup},
    query::tests::filter,
};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use token_pulse_core::{
    calendar::Grain, numeric::EpochMs, protocol::PriceBasis, query::DashboardRequest,
};
#[test]
fn occupied_background_readers_cannot_block_versions_mini_or_foreground_dashboard() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    db.mutate_display_timezone(
        token_pulse_core::settings::TimezoneMutation::Initialize {
            system_timezone: "UTC".into(),
        },
        EpochMs::new(0).unwrap(),
    )
    .unwrap();
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let (started, ready) = mpsc::channel();
    let workers: Vec<_> = (0..2)
        .map(|_| {
            let db = db.clone();
            let gate = gate.clone();
            let started = started.clone();
            std::thread::spawn(move || {
                db.snapshot(|_, _| {
                    started.send(()).unwrap();
                    let (lock, signal) = &*gate;
                    drop(
                        signal
                            .wait_while(lock.lock().unwrap(), |released| !*released)
                            .unwrap(),
                    );
                    Ok(())
                })
                .unwrap();
            })
        })
        .collect();
    ready.recv().unwrap();
    ready.recv().unwrap();
    let (reply, result) = mpsc::channel();
    let urgent = db.clone();
    let foreground = std::thread::spawn(move || {
        let _ = reply.send(read_urgent(&urgent));
    });
    let received = result.recv_timeout(std::time::Duration::from_secs(2));
    *gate.0.lock().unwrap() = true;
    gate.1.notify_all();
    for worker in workers {
        worker.join().unwrap();
    }
    foreground.join().unwrap();
    let received = received.unwrap();
    assert!(received.is_ok(), "{received:?}");
}
fn read_urgent(db: &Database) -> crate::StoreResult<()> {
    db.usage_revision()?;
    db.display_settings()?;
    db.sources_snapshot()?;
    db.mini_usage(EpochMs::new(3000)?, "priority-mini")?;
    let request = DashboardRequest {
        filter: filter(),
        price_basis: PriceBasis::EventTime {},
        grain: Grain::Hour,
        heatmap_range: filter().range,
    };
    assert_eq!(
        db.dashboard_bundle(&request, EpochMs::new(3000)?, "priority-main")?
            .summary
            .total_tokens
            .as_str(),
        "110"
    );
    Ok(())
}

#[test]
fn a_slow_detail_reader_leaves_a_second_interactive_connection_available() {
    let (_dir, db) = setup();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let slow_db = db.clone();
    let slow = std::thread::spawn(move || {
        slow_db.interactive_snapshot(|_, _| {
            ready_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            Ok(())
        })
    });
    ready_rx.recv().unwrap();
    let (reply_tx, reply_rx) = mpsc::channel();
    let fast = std::thread::spawn(move || {
        reply_tx.send(db.mini_scope()).unwrap();
    });
    let reply = reply_rx.recv_timeout(std::time::Duration::from_secs(2));
    release_tx.send(()).unwrap();
    slow.join().unwrap().unwrap();
    fast.join().unwrap();
    assert!(reply.unwrap().is_ok());
}
