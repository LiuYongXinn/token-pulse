use super::*;
use crate::{
    batch::tests::{fixture, setup},
    jobs::JobAdvance,
};
use token_pulse_core::{
    domain::EffectiveMetadata,
    jobs::{JobProgress, JobRequest},
};

fn populated() -> (tempfile::TempDir, Database) {
    let (dir, db) = setup();
    db.commit(fixture()).unwrap();
    (dir, db)
}
fn queued(db: &Database, id: &str, scope: JobScope) {
    db.create_job(
        id.into(),
        JobRequest {
            kind: JobKind::Rebuild,
            scope,
            request_key: id.into(),
        },
        2,
    )
    .unwrap();
}
fn advance(db: &Database, id: &str, expected: JobState, next: JobState) {
    let stored = db.get_job(id).unwrap();
    db.advance_job(
        id.into(),
        JobAdvance {
            expected,
            next,
            progress: JobProgress::default(),
            checkpoint: stored.checkpoint,
            error: None,
            at_ms: 3,
        },
    )
    .unwrap();
}
fn staging(db: &Database, key: &str, provider: &str, count: usize, ready: bool) {
    staging_with_fingerprint(db, key, provider, count, ready, None);
}
fn staging_with_fingerprint(
    db: &Database,
    key: &str,
    provider: &str,
    count: usize,
    ready: bool,
    large_fingerprint: Option<&str>,
) {
    let size = ((count + 1) * 10) as i64;
    db.begin_file_read_candidate(BeginFileCandidate {
        generation_id: "replacement".into(),
        file_id: "file".into(),
        expected_generation_id: "generation".into(),
        expected_checkpoint_revision: 1,
        identity: "replacement-identity".into(),
        observed_size: size,
        at_ms: 2,
    })
    .unwrap();
    let metadata = EffectiveMetadata {
        cwd: Some("E:/candidate-project".into()),
        model: Some("candidate-model".into()),
        parent_provider_id: Some("candidate-parent".into()),
        ..Default::default()
    };
    let mut observations = vec![ObservationWrite {
        observation_id: "candidate-head".into(),
        session_key: Some(key.into()),
        record: NormalizedObservation::SessionMetadata {
            physical_position: PhysicalPosition {
                file_generation_id: "replacement".into(),
                byte_offset: 0,
                byte_end: 10,
            },
            provider_session_id: provider.into(),
            metadata: metadata.clone(),
            created_at_ms: Some(42),
        },
        payload_fingerprint: "necessary-header".into(),
    }];
    for i in 0..count {
        let mut o = fixture().observations.remove(0);
        o.observation_id = format!("candidate-{i}");
        o.session_key = Some(key.into());
        let NormalizedObservation::Usage(u) = &mut o.record else {
            unreachable!()
        };
        u.session_key = key.into();
        u.physical_position = PhysicalPosition {
            file_generation_id: "replacement".into(),
            byte_offset: ((i + 1) * 10) as u64,
            byte_end: ((i + 2) * 10) as u64,
        };
        u.effective_metadata = metadata.clone();
        observations.push(o);
    }
    if let Some(fingerprint) = large_fingerprint {
        for o in &mut observations {
            o.payload_fingerprint = fingerprint.into();
        }
    }
    let chunk_size = if large_fingerprint.is_some() { 64 } else { 500 };
    for (index, chunk) in observations.chunks(chunk_size).enumerate() {
        let end = position(&chunk.last().unwrap().record).0.byte_end as i64;
        let start = position(&chunk[0].record).0.byte_offset as i64;
        db.stage_file_candidate_batch(FileCandidateBatch {
            generation_id: "replacement".into(),
            expected_offset: start,
            expected_checkpoint_revision: index as i64,
            next_offset: end,
            observed_size: size,
            anchors: vec![],
            context: ReaderContext {
                session_key: Some(key.into()),
                requires_sequence_rebuild: true,
                independent_head_available: false,
                metadata: metadata.clone(),
                ..Default::default()
            },
            observations: chunk.to_vec(),
            diagnostics: vec![],
            at_ms: 3,
        })
        .unwrap();
    }
    if ready {
        db.seal_file_read_candidate(
            "replacement".into(),
            observations.len().div_ceil(chunk_size) as i64,
            4,
        )
        .unwrap();
    }
}
fn claim(db: &Database, id: &str) -> FileCandidateRegistration {
    let revision = db
        .file_read_candidate("replacement")
        .unwrap()
        .checkpoint
        .checkpoint_revision;
    db.claim_file_candidate_for_rebuild("replacement".into(), id.into(), revision, 4)
        .unwrap()
}
fn active(db: &Database) -> (i64, i64, i64, String, i64, i64, String) {
    db.snapshot(|tx,r|Ok((r.data,r.price,r.settings,tx.query_row("SELECT current_generation_id FROM source_files WHERE file_id='file'",[],|r|r.get(0))?,tx.query_row("SELECT committed_offset FROM file_generations WHERE file_generation_id='generation'",[],|r|r.get(0))?,tx.query_row("SELECT checkpoint_revision FROM file_generations WHERE file_generation_id='generation'",[],|r|r.get(0))?,tx.query_row("SELECT sum_token_decimal(total_tokens) FROM active_usage_events",[],|r|r.get(0))?))).unwrap()
}
#[test]
fn bounded_registration_reopens_with_hidden_new_identity_and_preserves_old_facts_and_checkpoints() {
    let (dir, db) = populated();
    let old = active(&db);
    staging(&db, "new-session", "new-provider", 130, true);
    queued(
        &db,
        "owner",
        JobScope::Sources {
            source_ids: vec!["source".into()],
        },
    );
    assert_eq!(claim(&db, "owner").after_offset, -1);
    advance(&db, "owner", JobState::Queued, JobState::Running);
    let first = db
        .register_file_candidate_inputs("replacement".into(), "owner".into(), -1, 5)
        .unwrap();
    assert_eq!(first.registered_rows, 128);
    assert!(!first.complete);
    assert_eq!(first.after_offset, 1270);
    assert_eq!(active(&db), old);
    assert_eq!(claim(&db, "owner").after_offset, first.after_offset);
    assert_eq!(
        db.register_file_candidate_inputs("replacement".into(), "owner".into(), -1, 5)
            .unwrap_err()
            .code,
        ErrorCode::CheckpointConflict
    );
    drop(db);
    let db = Database::open(dir.path()).unwrap();
    let saved = &db.rebuild_file_candidates("owner").unwrap()[0];
    assert_eq!(saved.after_offset, first.after_offset);
    let last = db
        .register_file_candidate_inputs("replacement".into(), "owner".into(), saved.after_offset, 6)
        .unwrap();
    assert_eq!(last.registered_rows, 3);
    assert!(last.complete);
    assert_eq!(last.after_offset, 1300);
    assert_eq!(
        db.register_file_candidate_inputs(
            "replacement".into(),
            "owner".into(),
            last.after_offset,
            7
        )
        .unwrap()
        .registered_rows,
        0
    );
    assert_eq!(active(&db), old);
    db.snapshot(|tx,_| {
        assert!(tx.query_row("SELECT active_ledger_id IS NULL FROM sessions WHERE session_key='new-session'",[],|r|r.get::<_,bool>(0))?);
        assert_eq!(tx.query_row("SELECT COUNT(*) FROM observations WHERE file_generation_id='replacement'",[],|r|r.get::<_,i64>(0))?,131);
        assert_eq!(tx.query_row("SELECT COUNT(*) FROM stream_states",[],|r|r.get::<_,i64>(0))?,1);
        assert_eq!(tx.query_row("SELECT COUNT(*) FROM ledger_generations",[],|r|r.get::<_,i64>(0))?,1);
        assert_eq!(tx.query_row("SELECT checkpoint_revision FROM file_generations WHERE file_generation_id='replacement'",[],|r|r.get::<_,i64>(0))?,1);
        assert_eq!(tx.query_row("SELECT created_at_ms FROM file_rebuild_sessions WHERE generation_id='replacement'",[],|r|r.get::<_,i64>(0))?,42);
        Ok(())
    }).unwrap();
    assert_eq!(
        db.latest_context("new-session").unwrap_err().code,
        ErrorCode::InvalidQuery
    );
    assert!(
        !db.related_session_exists("new-provider", "session")
            .unwrap()
    );
    assert!(!db.session_has_usage("new-session").unwrap());
}
#[test]
fn claim_requires_ready_exact_revision_scoped_rebuild_and_single_owner() {
    let (_dir, db) = populated();
    staging(&db, "new-session", "new-provider", 1, false);
    queued(&db, "owner", JobScope::All {});
    assert_eq!(
        db.claim_file_candidate_for_rebuild("replacement".into(), "owner".into(), 1, 4)
            .unwrap_err()
            .code,
        ErrorCode::RevisionConflict
    );
    db.seal_file_read_candidate("replacement".into(), 1, 4)
        .unwrap();
    db.create_job(
        "not-rebuild".into(),
        JobRequest {
            kind: JobKind::Import,
            scope: JobScope::All {},
            request_key: "not-rebuild".into(),
        },
        4,
    )
    .unwrap();
    queued(
        &db,
        "wrong-source",
        JobScope::Sources {
            source_ids: vec!["another-source".into()],
        },
    );
    for id in ["not-rebuild", "wrong-source"] {
        assert_eq!(
            db.claim_file_candidate_for_rebuild("replacement".into(), id.into(), 1, 4)
                .unwrap_err()
                .code,
            ErrorCode::RevisionConflict
        );
    }
    assert_eq!(
        db.claim_file_candidate_for_rebuild("replacement".into(), "owner".into(), 0, 4)
            .unwrap_err()
            .code,
        ErrorCode::RevisionConflict
    );
    queued(
        &db,
        "wrong-scope",
        JobScope::Sessions {
            session_keys: vec!["session".into()],
        },
    );
    assert_eq!(
        db.claim_file_candidate_for_rebuild("replacement".into(), "wrong-scope".into(), 1, 4)
            .unwrap_err()
            .code,
        ErrorCode::RevisionConflict
    );
    let first = claim(&db, "owner");
    assert_eq!(claim(&db, "owner"), first);
    queued(&db, "other", JobScope::All {});
    assert_eq!(
        db.claim_file_candidate_for_rebuild("replacement".into(), "other".into(), 1, 4)
            .unwrap_err()
            .code,
        ErrorCode::RevisionConflict
    );
    assert_eq!(
        db.register_file_candidate_inputs("replacement".into(), "owner".into(), -1, 5)
            .unwrap_err()
            .code,
        ErrorCode::RevisionConflict
    );
    assert_eq!(
        db.reopen_file_read_candidate("replacement".into(), 1, 5)
            .unwrap_err()
            .code,
        ErrorCode::RevisionConflict
    );
    assert_eq!(
        db.fail_file_read_candidate("replacement".into(), ErrorCode::JobCancelled, 5)
            .unwrap_err()
            .code,
        ErrorCode::RevisionConflict
    );
}
#[test]
fn registration_failure_rolls_back_session_project_observations_bindings_and_cursor_together() {
    let (dir, db) = populated();
    let old = active(&db);
    staging(&db, "new-session", "new-provider", 1, true);
    queued(&db, "owner", JobScope::All {});
    claim(&db, "owner");
    advance(&db, "owner", JobState::Queued, JobState::Running);
    db.write(|conn| {conn.execute_batch("CREATE TRIGGER fail_candidate_usage BEFORE INSERT ON observations WHEN NEW.file_generation_id='replacement' AND NEW.kind='usage' BEGIN SELECT RAISE(ABORT,'synthetic writer failure'); END;")?;Ok(())}).unwrap();
    assert_eq!(
        db.register_file_candidate_inputs("replacement".into(), "owner".into(), -1, 5)
            .unwrap_err()
            .code,
        ErrorCode::DbWriteFailed
    );
    assert_eq!(active(&db), old);
    db.snapshot(|tx, _| {
        for (table, condition) in [
            ("sessions", "session_key='new-session'"),
            ("projects", "1=1"),
            ("observations", "file_generation_id='replacement'"),
            ("file_session_bindings", "file_generation_id='replacement'"),
            ("file_rebuild_sessions", "generation_id='replacement'"),
        ] {
            assert_eq!(
                tx.query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE {condition}"),
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                0
            );
        }
        assert_eq!(registration(tx, "replacement")?.after_offset, -1);
        Ok(())
    })
    .unwrap();
    db.write(|conn| {
        conn.execute_batch("DROP TRIGGER fail_candidate_usage;")?;
        Ok(())
    })
    .unwrap();
    drop(db);
    let db = Database::open(dir.path()).unwrap();
    assert!(
        db.register_file_candidate_inputs("replacement".into(), "owner".into(), -1, 6)
            .unwrap()
            .complete
    );
    assert_eq!(active(&db), old);
}
#[test]
fn proposed_header_does_not_change_published_identity_and_mismatch_cannot_register() {
    for provider in ["provider-session", "different-provider"] {
        let (_dir, db) = populated();
        let old = active(&db);
        staging(&db, "session", provider, 1, true);
        queued(&db, "owner", JobScope::All {});
        claim(&db, "owner");
        advance(&db, "owner", JobState::Queued, JobState::Running);
        let result = db.register_file_candidate_inputs("replacement".into(), "owner".into(), -1, 5);
        if provider == "provider-session" {
            assert!(result.unwrap().complete);
        } else {
            assert_eq!(result.unwrap_err().code, ErrorCode::CheckpointConflict);
        }
        db.snapshot(|tx,_| {
            assert!(tx.query_row("SELECT created_at_ms IS NULL AND parent_provider_id IS NULL FROM sessions WHERE session_key='session'",[],|r|r.get::<_,bool>(0))?);
            Ok(())
        }).unwrap();
        assert_eq!(active(&db), old);
    }
}
#[test]
fn foreign_usage_without_its_generation_header_and_obsolete_base_cannot_register() {
    for mutation in ["foreign", "paused", "checkpoint"] {
        let (_dir, db) = populated();
        staging(&db, "new-session", "new-provider", 1, true);
        queued(&db, "owner", JobScope::All {});
        claim(&db, "owner");
        advance(&db, "owner", JobState::Queued, JobState::Running);
        db.write(move|conn| {
            match mutation {
                "foreign"=>{conn.execute("UPDATE file_candidate_observations SET session_key='session',normalized_json=json_set(normalized_json,'$.session_key','session') WHERE observation_id='candidate-0'",[])?;},
                "paused"=>{conn.execute("UPDATE sources SET enabled=0 WHERE source_id='source'",[])?;},
                _=>{conn.execute("UPDATE file_generations SET checkpoint_revision=checkpoint_revision+1 WHERE file_generation_id='generation'",[])?;},
            }Ok(())
        }).unwrap();
        let old = active(&db);
        assert_eq!(
            db.register_file_candidate_inputs("replacement".into(), "owner".into(), -1, 5)
                .unwrap_err()
                .code,
            if mutation == "foreign" {
                ErrorCode::CheckpointConflict
            } else {
                ErrorCode::CandidateObsolete
            }
        );
        assert_eq!(active(&db), old);
        assert_eq!(
            db.rebuild_file_candidates("owner").unwrap()[0].after_offset,
            -1
        );
    }
}
#[test]
fn cancellation_failure_and_startup_interruption_release_only_owned_candidate_and_keep_old_results()
{
    for action in ["queued-cancel", "running-cancel", "failure", "startup"] {
        let (dir, db) = populated();
        let old = active(&db);
        staging(&db, "new-session", "new-provider", 1, true);
        queued(&db, "owner", JobScope::All {});
        claim(&db, "owner");
        if action != "queued-cancel" {
            advance(&db, "owner", JobState::Queued, JobState::Running);
            db.register_file_candidate_inputs("replacement".into(), "owner".into(), -1, 5)
                .unwrap();
        }
        match action {
            "queued-cancel" => {
                db.cancel_job("owner".into(), 6).unwrap();
            }
            "running-cancel" => {
                db.cancel_job("owner".into(), 6).unwrap();
                assert_eq!(
                    db.register_file_candidate_inputs("replacement".into(), "owner".into(), 10, 7)
                        .unwrap_err()
                        .code,
                    ErrorCode::JobCancelled
                );
                db.fail_rebuild("owner".into(), ErrorCode::JobCancelled, 7)
                    .unwrap();
            }
            "failure" => db
                .fail_rebuild("owner".into(), ErrorCode::InvalidUsage, 6)
                .unwrap(),
            _ => {
                assert_eq!(db.interrupt_unfinished_jobs(6).unwrap(), 1);
            }
        }
        drop(db);
        let db = Database::open(dir.path()).unwrap();
        assert_eq!(
            db.file_read_candidate("replacement").unwrap().state,
            "failed"
        );
        assert!(db.active_file_read_candidate("file").unwrap().is_none());
        assert_eq!(active(&db), old);
        db.snapshot(|tx, _| {
            assert_eq!(
                tx.query_row(
                    "SELECT state FROM file_generations WHERE file_generation_id='replacement'",
                    [],
                    |r| r.get::<_, String>(0)
                )?,
                "invalid"
            );
            Ok(())
        })
        .unwrap();
    }
}
#[test]
fn manifest_selects_owned_replacement_and_direct_success_cannot_bypass_publication() {
    let (_dir, db) = populated();
    let old = active(&db);
    staging(&db, "new-session", "new-provider", 1, true);
    queued(&db, "owner", JobScope::All {});
    claim(&db, "owner");
    advance(&db, "owner", JobState::Queued, JobState::Running);
    db.register_file_candidate_inputs("replacement".into(), "owner".into(), -1, 5)
        .unwrap();
    let manifest = db.prepare_rebuild("owner".into(), 6).unwrap();
    assert_eq!(manifest.replacements.len(), 1);
    assert_eq!(manifest.files.len(), 1);
    assert_eq!(manifest.files[0].generation_id, "replacement");
    assert!(
        manifest
            .ledgers
            .iter()
            .find(|l| l.session_key == "new-session")
            .unwrap()
            .old_ledger_id
            .is_none()
    );
    advance(&db, "owner", JobState::Running, JobState::Validating);
    advance(&db, "owner", JobState::Validating, JobState::Publishing);
    let stored = db.get_job("owner").unwrap();
    assert_eq!(
        db.advance_job(
            "owner".into(),
            JobAdvance {
                expected: JobState::Publishing,
                next: JobState::Succeeded,
                progress: JobProgress::default(),
                checkpoint: stored.checkpoint,
                error: None,
                at_ms: 7
            }
        )
        .unwrap_err()
        .code,
        ErrorCode::InvalidQuery
    );
    assert_eq!(db.get_job("owner").unwrap().job.state, JobState::Publishing);
    assert_eq!(active(&db), old);
}

#[test]
fn failed_cancel_transaction_keeps_job_claim_and_generation_together() {
    let (_dir, db) = populated();
    let old = active(&db);
    staging(&db, "new-session", "new-provider", 1, true);
    queued(&db, "owner", JobScope::All {});
    claim(&db, "owner");
    db.write(|conn| {conn.execute_batch("CREATE TRIGGER fail_owner_release BEFORE UPDATE OF state ON file_generations WHEN NEW.file_generation_id='replacement' AND NEW.state='invalid' BEGIN SELECT RAISE(ABORT,'synthetic release failure'); END;")?;Ok(())}).unwrap();
    assert_eq!(
        db.cancel_job("owner".into(), 6).unwrap_err().code,
        ErrorCode::DbWriteFailed
    );
    assert_eq!(db.get_job("owner").unwrap().job.state, JobState::Queued);
    assert_eq!(
        db.file_read_candidate("replacement").unwrap().state,
        "claimed"
    );
    assert_eq!(active(&db), old);
    db.write(|conn| {
        conn.execute_batch("DROP TRIGGER fail_owner_release;")?;
        Ok(())
    })
    .unwrap();
    db.cancel_job("owner".into(), 7).unwrap();
    assert_eq!(db.get_job("owner").unwrap().job.state, JobState::Cancelled);
    assert_eq!(
        db.file_read_candidate("replacement").unwrap().state,
        "failed"
    );
    assert_eq!(active(&db), old);
}

#[test]
fn registration_budget_includes_fingerprints_and_can_finish_empty_candidate() {
    let (_dir, db) = populated();
    let old = active(&db);
    staging_with_fingerprint(
        &db,
        "new-session",
        "new-provider",
        130,
        true,
        Some(&"f".repeat(128 * 1024)),
    );
    queued(&db, "owner", JobScope::All {});
    claim(&db, "owner");
    advance(&db, "owner", JobState::Queued, JobState::Running);
    let first = db
        .register_file_candidate_inputs("replacement".into(), "owner".into(), -1, 5)
        .unwrap();
    assert!(first.registered_rows < 128);
    assert!(!first.complete);
    db.snapshot(|tx,_| {
        let stored:i64=tx.query_row("SELECT SUM(length(normalized_json)+length(payload_fingerprint)) FROM observations WHERE file_generation_id='replacement'",[],|r|r.get(0))?;
        assert!(stored<=16*1024*1024);Ok(())
    }).unwrap();
    let last = db
        .register_file_candidate_inputs("replacement".into(), "owner".into(), first.after_offset, 6)
        .unwrap();
    assert!(last.complete);
    assert_eq!(first.registered_rows + last.registered_rows, 131);
    assert_eq!(active(&db), old);
    let (_dir, empty) = populated();
    let before = active(&empty);
    empty
        .begin_file_read_candidate(BeginFileCandidate {
            generation_id: "replacement".into(),
            file_id: "file".into(),
            expected_generation_id: "generation".into(),
            expected_checkpoint_revision: 1,
            identity: "empty-generation".into(),
            observed_size: 0,
            at_ms: 2,
        })
        .unwrap();
    empty
        .seal_file_read_candidate("replacement".into(), 0, 3)
        .unwrap();
    queued(&empty, "owner", JobScope::All {});
    claim(&empty, "owner");
    advance(&empty, "owner", JobState::Queued, JobState::Running);
    assert_eq!(
        empty
            .register_file_candidate_inputs("replacement".into(), "owner".into(), -1, 5)
            .unwrap(),
        RegistrationReceipt {
            after_offset: -1,
            registered_rows: 0,
            complete: true
        }
    );
    assert_eq!(active(&empty), before);
}

#[test]
fn schema_eight_ready_candidate_upgrades_and_registers_without_reimporting_or_changing_old_usage() {
    let (dir, db) = populated();
    let old = active(&db);
    staging(&db, "new-session", "new-provider", 1, true);
    // A real v8 fixture has the original tables/checksums and no v9 ownership tables.
    db.write(|conn| {
        let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch("DROP TABLE rebuild_manifests; DROP TABLE file_rebuild_sessions; DROP TABLE file_rebuild_candidates; DROP TABLE conditional_price_rules; ALTER TABLE price_rules DROP COLUMN request_conditional; ALTER TABLE price_rules DROP COLUMN cache_write_rate_atoms; ALTER TABLE usage_events DROP COLUMN cache_write_input_tokens; DELETE FROM schema_migrations WHERE version>=9; UPDATE app_state SET schema_version=8;")?;
        tx.pragma_update(None,"user_version",8)?;tx.commit()?;Ok(())
    }).unwrap();
    drop(db);
    let db = Database::open(dir.path()).unwrap();
    assert_eq!(active(&db), old);
    assert_eq!(
        db.file_read_candidate("replacement").unwrap().state,
        "ready"
    );
    queued(&db, "owner", JobScope::All {});
    claim(&db, "owner");
    advance(&db, "owner", JobState::Queued, JobState::Running);
    assert!(
        db.register_file_candidate_inputs("replacement".into(), "owner".into(), -1, 5)
            .unwrap()
            .complete
    );
    assert_eq!(active(&db), old);
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))?,
            crate::migration::SCHEMA_VERSION
        );
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r
                .get::<_, i64>(
                0
            ))?,
            0
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn atomic_queue_claim_is_idempotent_under_competition_and_failed_claim_cannot_leave_a_queued_job() {
    let (_dir, db) = populated();
    let old = active(&db);
    staging(&db, "new-session", "new-provider", 1, true);
    assert_eq!(
        db.enqueue_file_candidate_rebuild("replacement".into(), 0, 4)
            .unwrap_err()
            .code,
        ErrorCode::RevisionConflict
    );
    db.write(|conn| {conn.execute_batch("CREATE TRIGGER fail_claim BEFORE UPDATE OF state ON file_read_candidates WHEN NEW.state='claimed' BEGIN SELECT RAISE(ABORT,'synthetic claim failure'); END;")?;Ok(())}).unwrap();
    assert_eq!(
        db.enqueue_file_candidate_rebuild("replacement".into(), 1, 4)
            .unwrap_err()
            .code,
        ErrorCode::DbWriteFailed
    );
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM jobs", [], |r| r.get::<_, i64>(0))?,
            0
        );
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM file_rebuild_candidates", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(
        db.file_read_candidate("replacement").unwrap().state,
        "ready"
    );
    db.write(|conn| {
        conn.execute_batch("DROP TRIGGER fail_claim;")?;
        Ok(())
    })
    .unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let workers = (0..2)
        .map(|_| {
            let worker = db.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                worker
                    .enqueue_file_candidate_rebuild("replacement".into(), 1, 5)
                    .unwrap()
            })
        })
        .collect::<Vec<_>>();
    let jobs = workers
        .into_iter()
        .map(|w| w.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(jobs[0].job_id, jobs[1].job_id);
    assert_eq!(
        db.get_job(&jobs[0].job_id).unwrap().request.scope,
        JobScope::Sources {
            source_ids: vec!["source".into()]
        }
    );
    db.snapshot(|tx, _| {
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM jobs", [], |r| r.get::<_, i64>(0))?,
            1
        );
        assert!(has_owned(tx, &jobs[0].job_id)?);
        Ok(())
    })
    .unwrap();
    db.cancel_job(jobs[0].job_id.clone(), 6).unwrap();
    let repeated = db
        .enqueue_file_candidate_rebuild("replacement".into(), 1, 7)
        .unwrap();
    assert_eq!(repeated.job_id, jobs[0].job_id);
    assert_eq!(repeated.state, JobState::Cancelled);
    assert_eq!(active(&db), old);
}
