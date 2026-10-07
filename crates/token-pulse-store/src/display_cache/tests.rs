use super::*;
use crate::{
    batch::tests::{fixture, setup},
    query::tests::filter,
};
use token_pulse_core::{
    calendar::Grain,
    numeric::{DecimalInt, EpochMs},
    protocol::PriceBasis,
    query::DashboardRequest,
    settings::DisplayPrivacyMutation,
};
fn request() -> UsageDisplayRequest {
    UsageDisplayRequest::Dashboard {
        request: DashboardRequest {
            filter: filter(),
            price_basis: PriceBasis::EventTime {},
            grain: Grain::Hour,
            heatmap_range: filter().range,
        },
    }
}
fn save(db: &Database) -> DisplayCacheStamp {
    let stamp = db.display_cache_stamp().unwrap();
    let UsageDisplayRequest::Dashboard { request: query } = request() else {
        unreachable!()
    };
    let value = db
        .dashboard_bundle(&query, EpochMs::new(3000).unwrap(), "cached-test")
        .unwrap();
    db.remember_usage_display(
        &request(),
        UsageDisplayData::Dashboard { value },
        stamp.clone(),
    )
    .unwrap();
    stamp
}
#[test]
fn restart_restores_valid_complete_result_and_rejects_scope_format_corruption_and_identity() {
    let (dir, db) = setup();
    db.commit(fixture()).unwrap();
    save(&db);
    drop(db);
    let db = Database::open(dir.path()).unwrap();
    assert!(db.restore_usage_display(&request()).unwrap().data.is_some());
    let mut other = request();
    if let UsageDisplayRequest::Dashboard { request } = &mut other {
        request.filter.range.end_ms = EpochMs::new(9000).unwrap();
    }
    assert!(db.restore_usage_display(&other).unwrap().data.is_none());
    db.write(|conn| {
        conn.execute("UPDATE usage_display_cache SET format_version=99", [])?;
        Ok(())
    })
    .unwrap();
    assert!(db.restore_usage_display(&request()).unwrap().data.is_none());
    save(&db);
    db.write(|conn| {
        conn.execute(
            "UPDATE usage_display_cache SET result_json='{}',byte_length=2",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    assert!(db.restore_usage_display(&request()).unwrap().data.is_none());
    save(&db);
    db.write(|conn| {
        conn.execute(
            "UPDATE app_state SET database_instance_id=lower(hex(randomblob(16)))",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    assert!(db.restore_usage_display(&request()).unwrap().data.is_none());
}
#[test]
fn privacy_commit_erases_results_and_rejects_late_result_after_disable_and_view_only_changes() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    let old = save(&db);
    db.mutate_display_privacy(
        DisplayPrivacyMutation {
            privacy: true,
            expected_settings_revision: old.settings_revision.clone(),
        },
        EpochMs::new(3000).unwrap(),
    )
    .unwrap();
    assert!(db.restore_usage_display(&request()).unwrap().data.is_none());
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM usage_display_cache", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        Ok(())
    })
    .unwrap();
    db.mutate_display_privacy(
        DisplayPrivacyMutation {
            privacy: false,
            expected_settings_revision: DecimalInt::from_nonnegative(
                old.settings_revision.value() + 1,
            )
            .unwrap(),
        },
        EpochMs::new(3001).unwrap(),
    )
    .unwrap();
    let UsageDisplayRequest::Dashboard { request: query } = request() else {
        unreachable!()
    };
    let value = db
        .dashboard_bundle(&query, EpochMs::new(3000).unwrap(), "late-test")
        .unwrap();
    db.remember_usage_display(&request(), UsageDisplayData::Dashboard { value }, old)
        .unwrap();
    assert!(db.restore_usage_display(&request()).unwrap().data.is_none());
    save(&db);
    db.write(|conn| {
        conn.execute("UPDATE sources SET enabled=0", [])?;
        Ok(())
    })
    .unwrap();
    assert!(db.restore_usage_display(&request()).unwrap().data.is_none());
}

#[test]
fn persisted_pages_have_no_lease_and_capacity_evicts_whole_results() {
    use token_pulse_core::query::{UsageEventSort, UsageEventsQuery, UsageEventsRequest};
    let (dir, db) = setup();
    db.commit(fixture()).unwrap();
    let query = UsageEventsQuery {
        filter: filter(),
        price_basis: PriceBasis::EventTime {},
        sort: UsageEventSort::TimeDesc,
        page_size: 50,
    };
    let request = UsageDisplayRequest::Events {
        request: query.clone(),
    };
    let stamp = db.display_cache_stamp().unwrap();
    let mut page = db
        .query_usage_events(
            "main",
            &UsageEventsRequest {
                query,
                cursor: None,
            },
            EpochMs::new(3000).unwrap(),
        )
        .unwrap();
    page.next_cursor = Some("expired-private-capability".into());
    db.remember_usage_display(&request, UsageDisplayData::Events { value: page }, stamp)
        .unwrap();
    drop(db);
    let db = Database::open(dir.path()).unwrap();
    let restored = db.restore_usage_display(&request).unwrap();
    assert!(restored.has_more);
    let Some(UsageDisplayData::Events { value }) = restored.data else {
        panic!("missing page")
    };
    assert!(value.next_cursor.is_none());
    let UsageDisplayRequest::Dashboard { request: dashboard } = super::tests::request() else {
        unreachable!()
    };
    let value = db
        .dashboard_bundle(&dashboard, EpochMs::new(3000).unwrap(), "budget-test")
        .unwrap();
    for n in 0..25 {
        let mut scope = dashboard.clone();
        scope.filter.range.end_ms = EpochMs::new(3000 + n).unwrap();
        db.remember_usage_display(
            &UsageDisplayRequest::Dashboard { request: scope },
            UsageDisplayData::Dashboard {
                value: value.clone(),
            },
            db.display_cache_stamp().unwrap(),
        )
        .unwrap();
    }
    db.snapshot(|tx, _| {
        let (count, size): (i64, i64) = tx.query_row(
            "SELECT COUNT(*),SUM(byte_length) FROM usage_display_cache",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        assert_eq!(count, 20);
        assert!(size <= BUDGET as i64);
        Ok(())
    })
    .unwrap();
    assert!(db.restore_usage_display(&request).unwrap().data.is_none());
}
