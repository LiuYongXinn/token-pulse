//! Application-owned facts and synthetic prices with independent expected sums.
use super::*;
use crate::{
    batch::tests::{fixture, setup},
    query::tests::{extra, filter, ids},
};
use rusqlite::params;
use token_pulse_core::{
    pricing::{PriceRuleDraft, PriceRuleMutation},
    protocol::{CoverageState, DimensionSelection, PriceBasis},
    query::GroupSort,
};
fn request(dimension: GroupDimension) -> GroupedUsageRequest {
    GroupedUsageRequest {
        filter: filter(),
        price_basis: PriceBasis::EventTime {},
        dimension,
        sort: GroupSort::TotalDesc,
        limit: 200,
        cursor: None,
    }
}
#[test]
fn pages_cross_two_hundred_groups_and_keep_snapshot_and_binding() {
    let (_directory, db) = setup();
    db.commit(fixture()).unwrap();
    for n in 0..205 {
        let id = format!("model-{n:03}");
        extra(&db, &id, 2000, 1, (Some(&id), None), None, None);
    }
    let mut query = request(GroupDimension::Models);
    query.sort = GroupSort::NameAsc;
    let first = db
        .grouped_usage_page("main", &query, EpochMs::new(10_000).unwrap())
        .unwrap();
    assert_eq!(first.groups.len(), 200);
    assert_eq!(first.total_group_count.as_str(), "206");
    query.cursor = first.next_cursor.clone();
    assert_eq!(
        db.grouped_usage_page("other", &query, EpochMs::new(20_000).unwrap())
            .unwrap_err()
            .code,
        ErrorCode::CursorInvalid
    );
    let mut changed = query.clone();
    changed.sort = GroupSort::TotalDesc;
    assert_eq!(
        db.grouped_usage_page("main", &changed, EpochMs::new(20_000).unwrap())
            .unwrap_err()
            .code,
        ErrorCode::CursorInvalid
    );
    extra(&db, "later", 2000, 1, (Some("later"), None), None, None);
    let second = db
        .grouped_usage_page("main", &query, EpochMs::new(20_000).unwrap())
        .unwrap();
    assert_eq!(second.groups.len(), 6);
    assert!(second.next_cursor.is_none());
    assert_eq!(
        serde_json::to_value(&first.meta).unwrap(),
        serde_json::to_value(&second.meta).unwrap()
    );
    assert_eq!(first.summary.total_tokens, second.summary.total_tokens);
    assert_eq!(second.total_group_count.as_str(), "206");
    let keys: std::collections::BTreeSet<_> = first
        .groups
        .iter()
        .chain(&second.groups)
        .map(|g| &g.key)
        .collect();
    assert_eq!(keys.len(), 206);
}
fn read(db: &Database, request: &GroupedUsageRequest) -> GroupedUsageBundle {
    db.grouped_usage_bundle(request, EpochMs::new(10_000).unwrap(), "synthetic-groups")
        .unwrap()
}
fn draft(rate: i128) -> PriceRuleDraft {
    PriceRuleDraft {
        provider: "synthetic-provider".into(),
        model_exact: "synthetic-model".into(),
        source_id: None,
        currency: "USD".into(),
        effective_from_ms: EpochMs::new(0).unwrap(),
        effective_to_ms: None,
        priority: 0,
        input_rate_atoms: DecimalInt::from_nonnegative(rate).unwrap(),
        cache_write_rate_atoms: None,
        cached_rate_atoms: Some(DecimalInt::from_nonnegative(rate).unwrap()),
        output_rate_atoms: DecimalInt::from_nonnegative(rate).unwrap(),
        origin_reference: Some("synthetic test only".into()),
    }
}
fn known(db: &Database) {
    db.write(|conn| {
        conn.execute("UPDATE observations SET normalized_json=json_set(normalized_json,'$.effective_metadata.provider','synthetic-provider') WHERE observation_id='observation'", [])?;
        conn.execute("UPDATE usage_events SET model='synthetic-model' WHERE event_id='event'", [])?;
        Ok(())
    }).unwrap();
}

#[test]
fn grouped_pending_matches_each_independent_scope_and_keeps_an_old_snapshot() {
    let (_directory, db) = setup();
    db.commit(fixture()).unwrap();
    known(&db);
    db.write(|c| {
        c.execute("INSERT INTO projects VALUES('a','synthetic-a','A',NULL,1)", [])?;
        c.execute("UPDATE observations SET model='synthetic-model',project_id='a' WHERE observation_id='observation'", [])?;
        c.execute("UPDATE usage_events SET project_id='a' WHERE event_id='event'", [])?;
        Ok(())
    }).unwrap();
    extra(
        &db,
        "other",
        2000,
        1,
        (Some("synthetic-model"), Some("other-provider")),
        None,
        Some("a"),
    );
    extra(&db, "unknown", 3000, 1, (None, None), None, None);
    db.write(|c| {
        for (id, observation, kind, vector) in [
            ("known-pending", "observation", "pending", "{}"),
            ("known-amount", "observation", "unattributed", r#"{"input_total":9007199254740993,"cached_input":0,"output_total":0,"reasoning_output":0,"reported_total":9007199254740993}"#),
            ("other-missing", "other", "unattributed", "{}"),
            ("unknown-pending", "unknown", "pending", "{}"),
        ] {
            c.execute("INSERT INTO pending_usage VALUES(?1,'ledger',?2,?3,'synthetic',NULL,?4)", params![id,observation,kind,vector])?;
        }
        c.execute("UPDATE observations SET observed_at_ms=NULL WHERE observation_id='unknown'", [])?;
        Ok(())
    }).unwrap();
    let verify = |tx: &Transaction<'_>| -> StoreResult<Vec<serde_json::Value>> {
        let mut requests = vec![filter()];
        let mut narrowed = filter();
        narrowed.range.start_ms = EpochMs::new(2000)?;
        requests.push(narrowed);
        let mut empty = filter();
        empty.sources = ids(&[], false);
        requests.push(empty);
        let mut unknown = filter();
        unknown.models = ids(&[], true);
        requests.push(unknown);
        let mut project = filter();
        project.projects = ids(&["a"], true);
        requests.push(project);
        let mut results = vec![];
        for scope in requests {
            let totals = super::super::totals(tx, &scope)?;
            let common = coverage::coverage(tx, &scope, &totals)?;
            for dimension in [GroupDimension::Models, GroupDimension::Projects] {
                let groups = grouped(tx, &scope, dimension, GroupSort::TotalDesc, 200)?;
                let actual = coverage::grouped_coverage(tx, &scope, dimension, &groups, &common)?;
                assert_eq!(actual.len(), groups.len());
                for (group, value) in groups.iter().zip(actual) {
                    let mut selected = scope.clone();
                    let selection = DimensionSelection::Ids {
                        ids: group.key.iter().cloned().collect(),
                        include_unknown: group.key.is_none(),
                    };
                    match dimension {
                        GroupDimension::Models => selected.models = selection,
                        GroupDimension::Projects => selected.projects = selection,
                    }
                    let expected =
                        coverage::narrowed_coverage(tx, &selected, &group.totals, &common)?;
                    assert_eq!(
                        serde_json::to_value(&value).unwrap(),
                        serde_json::to_value(expected).unwrap()
                    );
                    results.push(serde_json::to_value(value).unwrap());
                }
            }
        }
        Ok(results)
    };
    let before = db
        .snapshot(|tx, _| {
            let before = verify(tx)?;
            db.write(|c| {
                c.execute("UPDATE sources SET enabled=0", [])?;
                c.execute(
                    "UPDATE pending_usage SET vector_json='{}' WHERE pending_id='known-amount'",
                    [],
                )?;
                Ok(())
            })?;
            assert_eq!(verify(tx)?, before);
            Ok(before)
        })
        .unwrap();
    let after = db.snapshot(|tx, _| verify(tx)).unwrap();
    assert_ne!(after, before);
}

#[test]
fn model_groups_keep_exact_prices_null_category_and_pending_coverage() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    known(&db);
    db.mutate_price_rule(PriceRuleMutation::Create { draft: draft(2) }, 0, 1)
        .unwrap();
    extra(&db, "unknown-a", 2000, 7, (None, Some("a")), None, None);
    extra(&db, "unknown-b", 2100, 9, (None, Some("b")), None, None);
    extra(
        &db,
        "other",
        2200,
        20,
        (Some("synthetic-model"), Some("other-provider")),
        None,
        None,
    );
    db.write(|conn| {
        conn.execute("INSERT INTO observations SELECT 'pending','generation',1000,1100,session_key,kind,NULL,NULL,NULL,NULL,NULL,NULL,'{}','pending','synthetic' FROM observations WHERE observation_id='observation'", [])?;
        conn.execute("INSERT INTO pending_usage VALUES('pending','ledger','pending','pending','synthetic',NULL,'{}')", [])?;
        Ok(())
    }).unwrap();
    let b = read(&db, &request(GroupDimension::Models));
    assert_eq!(b.summary.total_tokens.as_str(), "146");
    assert_eq!(b.total_group_count.as_str(), "3");
    assert!(!b.truncated);
    assert_eq!(b.pricing.priced_total_tokens.as_str(), "110");
    assert_eq!(b.pricing.unpriced_total_tokens.as_str(), "36");
    assert_eq!(
        b.pricing.currencies[0]
            .estimated_cost
            .as_ref()
            .unwrap()
            .as_str(),
        "0.000000000000220"
    );
    assert_eq!(b.coverage.pending_observation_count.as_str(), "1");
    assert_eq!(
        b.groups[0].key,
        model_key(Some("synthetic-provider"), Some("synthetic-model"))
    );
    assert_eq!(b.groups[0].pricing.priced_total_tokens.as_str(), "110");
    assert!(b.groups[0].totals.input_total.complete);
    assert_eq!(b.groups[0].coverage.pending_observation_count.as_str(), "0");
    let unknown = b.groups.iter().find(|g| g.key.is_none()).unwrap();
    assert_eq!(unknown.totals.total_tokens.as_str(), "16");
    assert!(unknown.totals.input_total.value.is_none());
    assert!(unknown.pricing.currencies.is_empty());
    assert_eq!(unknown.coverage.pending_observation_count.as_str(), "1");
    assert!(matches!(unknown.coverage.state, CoverageState::Partial));
    let mut r = request(GroupDimension::Models);
    r.filter.sources = ids(&["source"], false);
    assert_eq!(read(&db, &r).summary.total_tokens.as_str(), "146");
    r.filter.models = ids(&[], true);
    let only = read(&db, &r);
    assert_eq!(only.total_group_count.as_str(), "1");
    assert_eq!(only.summary.total_tokens.as_str(), "16");
    r.filter.range.start_ms = EpochMs::new(2200).unwrap();
    assert!(read(&db, &r).groups.is_empty());
}

#[test]
fn project_groups_use_actual_event_project_alias_and_expose_truncation_without_changing_summary() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    known(&db);
    db.write(|conn| {
        conn.execute(
            "INSERT INTO projects VALUES('a','synthetic-a','Project A','Alias A',1)",
            [],
        )?;
        conn.execute(
            "INSERT INTO projects VALUES('b','synthetic-b','Project B',NULL,1)",
            [],
        )?;
        conn.execute(
            "UPDATE usage_events SET project_id='a' WHERE event_id='event'",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    extra(&db, "second", 2000, 11, (None, None), None, Some("b"));
    extra(&db, "unknown", 2100, 7, (None, None), None, None);
    db.mutate_price_rule(PriceRuleMutation::Create { draft: draft(1) }, 0, 1)
        .unwrap();
    let mut r = request(GroupDimension::Projects);
    r.limit = 1;
    let b = read(&db, &r);
    assert_eq!(b.summary.total_tokens.as_str(), "128");
    assert_eq!(b.total_group_count.as_str(), "3");
    assert!(b.truncated);
    assert_eq!(b.groups.len(), 1);
    assert_eq!(b.groups[0].key.as_deref(), Some("a"));
    assert_eq!(b.groups[0].display_name, "Alias A");
    assert_eq!(b.pricing.unpriced_total_tokens.as_str(), "18");
    assert_eq!(b.groups[0].pricing.unpriced_total_tokens.as_str(), "0");
    r.limit = 200;
    r.sort = GroupSort::NameAsc;
    let b = read(&db, &r);
    assert!(!b.truncated);
    assert_eq!(
        b.groups
            .iter()
            .map(|g| g.display_name.as_str())
            .collect::<Vec<_>>(),
        ["Alias A", "Project B", "未知项目"]
    );
    r.filter.projects = ids(&["b"], true);
    assert_eq!(read(&db, &r).summary.total_tokens.as_str(), "18");
    r.filter.sessions = ids(&["missing"], false);
    let b = read(&db, &r);
    assert!(b.groups.is_empty());
    assert_eq!(b.total_group_count.as_str(), "0");
    assert!(!b.truncated);
    assert!(b.meta.parser_versions.is_empty());
    assert!(b.pricing.currencies.is_empty());
}

#[test]
fn grouped_bundle_remains_identical_in_a_real_old_snapshot_after_facts_aliases_and_prices_change() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    known(&db);
    db.write(|conn| {
        conn.execute(
            "INSERT INTO projects VALUES('a','synthetic-a','Project A','Old alias',1)",
            [],
        )?;
        conn.execute(
            "UPDATE usage_events SET project_id='a' WHERE event_id='event'",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    db.mutate_price_rule(PriceRuleMutation::Create { draft: draft(1) }, 0, 1)
        .unwrap();
    let r = request(GroupDimension::Models);
    let project_request = request(GroupDimension::Projects);
    db.snapshot(|tx, revision| {
        let before = bundle(tx, revision, &r, EpochMs::new(1234).unwrap(), "pinned")?;
        let projects_before = bundle(
            tx,
            revision,
            &project_request,
            EpochMs::new(1234).unwrap(),
            "pinned-projects",
        )?;
        let rule = crate::pricing::rules_at(tx, revision.price)?
            .rules
            .remove(0);
        db.mutate_price_rule(
            PriceRuleMutation::Replace {
                rule_id: rule.rule_id,
                draft: draft(2),
            },
            1,
            2,
        )?;
        extra(&db, "concurrent", 2000, 17, (None, None), None, None);
        db.write(|conn| {
            conn.execute(
                "UPDATE projects SET user_alias='New alias' WHERE project_id='a'",
                [],
            )?;
            Ok(())
        })?;
        assert_eq!(
            serde_json::to_value(&projects_before)?,
            serde_json::to_value(bundle(
                tx,
                revision,
                &project_request,
                EpochMs::new(1234).unwrap(),
                "pinned-projects"
            )?)?
        );
        assert_eq!(
            serde_json::to_value(&before)?,
            serde_json::to_value(bundle(
                tx,
                revision,
                &r,
                EpochMs::new(1234).unwrap(),
                "pinned"
            )?)?
        );
        assert_eq!(before.meta.price_revision.as_str(), "1");
        Ok(())
    })
    .unwrap();
    let b = read(&db, &r);
    assert_eq!(
        read(&db, &project_request).groups[0].display_name,
        "New alias"
    );
    assert_eq!(b.meta.price_revision.as_str(), "2");
    assert_eq!(b.summary.total_tokens.as_str(), "127");
    assert_eq!(
        b.pricing.currencies[0]
            .estimated_cost
            .as_ref()
            .unwrap()
            .as_str(),
        "0.000000000000220"
    );
}

#[test]
fn numeric_sort_large_aggregates_ties_and_request_bounds_are_exact() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    extra(
        &db,
        "huge-a",
        2000,
        9_007_199_254_740_993,
        (Some("a"), None),
        None,
        None,
    );
    extra(
        &db,
        "huge-b",
        2100,
        9_007_199_254_740_993,
        (Some("b"), None),
        None,
        None,
    );
    extra(&db, "ten", 2200, 10, (Some("ten"), None), None, None);
    let r = request(GroupDimension::Models);
    let b = read(&db, &r);
    assert_eq!(b.summary.total_tokens.as_str(), "18014398509482106");
    assert_eq!(b.groups[0].totals.total_tokens.as_str(), "9007199254740993");
    assert!(b.groups[0].key < b.groups[1].key);
    assert_eq!(b.groups[2].totals.total_tokens.as_str(), "110");
    assert_eq!(b.groups[3].totals.total_tokens.as_str(), "10");
    for limit in [0, 201, u16::MAX] {
        let mut bad = r.clone();
        bad.limit = limit;
        assert_eq!(
            db.grouped_usage_bundle(&bad, EpochMs::new(1).unwrap(), "bad")
                .unwrap_err()
                .code,
            ErrorCode::InvalidQuery
        );
    }
    assert_eq!(
        db.grouped_usage_bundle(&r, EpochMs::new(1).unwrap(), "")
            .unwrap_err()
            .code,
        ErrorCode::InvalidQuery
    );
    let mut injected = r;
    injected.filter.models = ids(&["x') OR 1=1 --"], false);
    assert_eq!(read(&db, &injected).summary.total_tokens.as_str(), "0");
}
