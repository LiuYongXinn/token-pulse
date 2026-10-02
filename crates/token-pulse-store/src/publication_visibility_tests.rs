//! Cross-entry checks for unpublished identities; source files are synthetic SQLite fixtures.
use crate::{
    Database, ErrorCode, SessionRegistration, StoreResult,
    batch::tests::{fixture, setup},
};
use rusqlite::{TransactionBehavior, params};
use token_pulse_core::{
    domain::{ACCOUNTING_VERSION, NormalizedObservation, PARSER_VERSION},
    jobs::JobScope,
    mini::{MiniScopeMutation, MiniSessionsQuery, MiniSessionsRequest},
    numeric::{DecimalInt, EpochMs},
    protocol::{MiniScope, PriceBasis, ScopeStart},
    query::{SessionBundleRequest, SessionSort, SessionsQuery, SessionsRequest},
    settings::TimezoneMutation,
};
fn at() -> EpochMs {
    EpochMs::new(2000).unwrap()
}
fn unpublished(db: &Database) {
    let mut observation = fixture().observations.remove(0).record;
    let NormalizedObservation::Usage(u) = &mut observation else {
        unreachable!()
    };
    u.session_key = "draft".into();
    u.physical_position.file_generation_id = "draft-generation".into();
    let encoded = serde_json::to_string(&observation).unwrap();
    db.write(move|conn| {
        let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("INSERT INTO sessions(session_key,provider,provider_session_id,identity_status,parent_key) VALUES('draft','codex','provider-session','candidate','session'),('draft-parent','codex','unpublished-parent','candidate',NULL)",[])?;
        tx.execute("INSERT INTO ledger_generations(ledger_id,session_key,state,parser_version,accounting_version,base_data_revision,created_at_ms,input_manifest_json) VALUES('draft-ledger','draft','candidate',?1,?2,1,1,'{}')",params![PARSER_VERSION,ACCOUNTING_VERSION])?;
        tx.execute("INSERT INTO file_generations(file_generation_id,file_id,state,identity_json,observed_size,committed_offset,checkpoint_revision,anchor_json,reader_context_json,parser_version,created_at_ms) VALUES('draft-generation','file','candidate','null',101,100,1,'[]','{}',?1,1)",[PARSER_VERSION])?;
        tx.execute("INSERT INTO observations(observation_id,file_generation_id,byte_offset,byte_end,session_key,kind,normalized_json,payload_fingerprint,format_version) VALUES('draft-observation','draft-generation',0,100,'draft','usage',?1,'synthetic',?2)",params![encoded,PARSER_VERSION])?;
        tx.execute("INSERT INTO file_session_bindings VALUES('draft-generation','draft',0,'session_header')",[])?;
        tx.execute("INSERT INTO usage_events(event_id,ledger_id,origin_observation_id,occurred_at_ms,episode_id,input_tokens_total,cached_input_tokens,output_tokens_total,reasoning_output_tokens,source_total_tokens,total_tokens,calculation_method,quality_json) VALUES('draft-event','draft-ledger','draft-observation',1000,'episode',100,60,10,2,110,110,'last_new_stream','{}')",[])?;
        tx.commit()?;Ok(())
    }).unwrap();
}
fn published_empty(db: &Database, key: &str, parent: Option<&str>) {
    db.ensure_session(SessionRegistration {
        session_key: key.into(),
        provider_session_id: Some(key.into()),
        parent_key: parent.map(str::to_owned),
        parent_provider_id: parent.map(|_| "unpublished-parent".into()),
        created_at_ms: None,
        ledger_id: format!("ledger-{key}"),
        registered_at_ms: 1,
    })
    .unwrap();
}
fn mini(size: u16) -> MiniSessionsRequest {
    MiniSessionsRequest {
        query: MiniSessionsQuery {
            search: "".into(),
            page_size: size,
        },
        cursor: None,
    }
}
fn bundle(db: &Database, key: &str) -> StoreResult<token_pulse_core::query::SessionBundle> {
    db.session_bundle(
        &SessionBundleRequest {
            session_key: key.into(),
            filter: crate::query::tests::filter(),
            price_basis: PriceBasis::EventTime {},
        },
        at(),
        "visibility",
    )
}
#[test]
fn unpublished_identity_and_candidate_facts_are_hidden_from_usage_details_relations_context_and_scope()
 {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    unpublished(&db);
    db.mutate_display_timezone(
        TimezoneMutation::Initialize {
            system_timezone: "UTC".into(),
        },
        at(),
    )
    .unwrap();
    published_empty(&db, "published-empty", Some("draft-parent"));
    db.write(|conn| {
        conn.execute("UPDATE sessions SET parent_key='draft-parent',parent_provider_id='unpublished-parent' WHERE session_key='session'",[])?;
        Ok(())
    }).unwrap();
    let options = db.mini_sessions("mini", &mini(100), at()).unwrap();
    assert_eq!(
        options
            .options
            .iter()
            .map(|o| o.session_key.as_str())
            .collect::<Vec<_>>(),
        ["published-empty", "session"]
    );
    assert_eq!(
        db.usage_totals(&crate::query::tests::filter())
            .unwrap()
            .total_tokens
            .as_str(),
        "110"
    );
    assert_eq!(
        bundle(&db, "draft").unwrap_err().code,
        ErrorCode::InvalidQuery
    );
    assert_eq!(
        db.latest_context("draft").unwrap_err().code,
        ErrorCode::InvalidQuery
    );
    assert_eq!(
        db.query_turns(
            "main",
            &token_pulse_core::query::TurnsRequest {
                query: token_pulse_core::query::TurnsQuery {
                    session_key: "draft".into(),
                    filter: crate::query::tests::filter(),
                    price_basis: PriceBasis::EventTime {},
                    page_size: 10,
                },
                cursor: None,
            },
            at()
        )
        .unwrap_err()
        .code,
        ErrorCode::InvalidQuery
    );
    assert_eq!(bundle(&db, "session").unwrap().child_count.as_str(), "0");
    let visible = bundle(&db, "published-empty").unwrap();
    assert!(visible.identity.parent_key.is_none());
    assert!(visible.identity.parent_display_name.is_none());
    // The published child's own provider evidence is retained even while its parent is absent.
    assert_eq!(
        visible.identity.parent_provider_id.as_deref(),
        Some("unpublished-parent")
    );
    assert!(visible.latest_context.observed_at_ms.is_none());
    let page = db
        .query_sessions(
            "main",
            &SessionsRequest {
                query: SessionsQuery {
                    filter: crate::query::tests::filter(),
                    price_basis: PriceBasis::EventTime {},
                    sort: SessionSort::LatestDesc,
                    page_size: 100,
                },
                cursor: None,
            },
            at(),
        )
        .unwrap();
    assert_eq!(page.sessions.len(), 1);
    assert_eq!(page.sessions[0].child_count.as_str(), "0");
    assert!(page.sessions[0].parent_key.is_none());
    assert!(page.sessions[0].parent_display_name.is_none());
    assert_eq!(
        page.sessions[0].parent_provider_id.as_deref(),
        Some("unpublished-parent")
    );
    let before = db.mini_scope().unwrap();
    assert_eq!(
        db.mutate_mini_scope(
            MiniScopeMutation {
                expected_settings_revision: before.settings_revision.clone(),
                mini_scope: MiniScope::Session {
                    session_key: "draft".into(),
                    start: ScopeStart::Fixed {
                        start_ms: EpochMs::new(0).unwrap()
                    }
                }
            },
            at()
        )
        .unwrap_err()
        .code,
        ErrorCode::InvalidQuery
    );
    let after = db.mini_scope().unwrap();
    assert_eq!(after.settings_revision, before.settings_revision);
    assert_eq!(after.mini_scope, before.mini_scope);
    assert!(!db.session_has_usage("draft").unwrap());
    assert!(
        !db.related_session_exists("provider-session", "session")
            .unwrap()
    );
    db.snapshot(|_, r| {
        assert_eq!(r.data, 1);
        assert_eq!(r.price, 0);
        Ok(())
    })
    .unwrap();
}
#[test]
fn ordinary_dependency_groups_exclude_unpublished_peers_parents_children_and_source_bindings() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    unpublished(&db);
    for scope in [
        JobScope::All {},
        JobScope::Sources {
            source_ids: vec!["source".into()],
        },
        JobScope::Sessions {
            session_keys: vec!["session".into()],
        },
    ] {
        db.snapshot(|tx, _| {
            assert_eq!(
                crate::rebuild::dependency_closure(tx, &scope)?
                    .into_iter()
                    .collect::<Vec<_>>(),
                ["session"]
            );
            Ok(())
        })
        .unwrap();
    }
    assert_eq!(
        db.rebuild_has_targets(&JobScope::Sessions {
            session_keys: vec!["draft".into()]
        })
        .unwrap_err()
        .code,
        ErrorCode::InvalidQuery
    );
    // An unpublished matching peer must not trigger automatic proof work.
    db.write(|conn| {conn.execute("INSERT INTO pending_usage VALUES('pending','ledger','observation','pending','waiting_identity',NULL,'{}')",[])?;Ok(())}).unwrap();
    assert!(db.enqueue_proof_rebuild(2).unwrap().is_none());
    // An explicit need to rebuild is supported despite an incomplete, unselected candidate.
    db.write(|conn| {
        conn.execute("INSERT INTO file_usage_cursors VALUES('ledger','generation',0,'rebuild_required')",[])?;
        conn.execute("INSERT INTO file_session_bindings VALUES('draft-generation','session',0,'session_header')",[])?;
        Ok(())
    }).unwrap();
    let job = db.enqueue_proof_rebuild(3).unwrap().unwrap();
    assert_eq!(job.kind, token_pulse_core::protocol::JobKind::Rebuild);
    db.snapshot(|tx, _| {
        assert_eq!(
            crate::rebuild::dependency_closure(tx, &db.get_job(&job.job_id)?.request.scope)?
                .into_iter()
                .collect::<Vec<_>>(),
            ["session"]
        );
        Ok(())
    })
    .unwrap();
}
#[test]
fn mini_cursor_keeps_old_visibility_when_a_new_identity_receives_its_active_pointer() {
    let (_dir, db) = setup();
    db.commit(fixture()).unwrap();
    unpublished(&db);
    db.mutate_display_timezone(
        TimezoneMutation::Initialize {
            system_timezone: "UTC".into(),
        },
        at(),
    )
    .unwrap();
    published_empty(&db, "a", None);
    let mut req = mini(1);
    let first = db.mini_sessions("mini", &req, at()).unwrap();
    assert_eq!(first.options[0].session_key, "a");
    req.cursor = first.next_cursor;
    // Simulate the identity part of publication in a single Writer transaction.
    // This is a query isolation test, not acceptance of replacement-file publication.
    db.write(|conn| {
        let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("UPDATE ledger_generations SET state='active' WHERE ledger_id='draft-ledger'",[])?;
        tx.execute("UPDATE sessions SET active_ledger_id='draft-ledger',identity_status='confirmed' WHERE session_key='draft'",[])?;
        tx.execute("UPDATE app_state SET data_revision=data_revision+1",[])?;
        tx.commit()?;Ok(())
    }).unwrap();
    let next = db.mini_sessions("mini", &req, at()).unwrap();
    assert_eq!(next.meta.data_revision, first.meta.data_revision);
    assert_eq!(next.options[0].session_key, "session");
    assert!(next.next_cursor.is_none());
    let current = db.mini_sessions("mini", &mini(100), at()).unwrap();
    assert_eq!(
        current.meta.data_revision,
        DecimalInt::from_nonnegative(2).unwrap()
    );
    assert_eq!(
        current
            .options
            .iter()
            .map(|o| o.session_key.as_str())
            .collect::<Vec<_>>(),
        ["a", "draft", "session"]
    );
    assert_eq!(bundle(&db, "session").unwrap().child_count.as_str(), "1");
    assert_eq!(
        bundle(&db, "draft").unwrap().identity.parent_key.as_deref(),
        Some("session")
    );
}
