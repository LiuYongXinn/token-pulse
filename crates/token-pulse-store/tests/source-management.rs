use token_pulse_core::sources::*;
use token_pulse_store::{Database, ErrorCode, source_management::SourceMutation};
#[test]
fn source_configuration_revisions_capabilities_and_retained_history_are_independent() {
    let data = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let db = Database::open(data.path()).unwrap();
    let initial = db.sources_snapshot().unwrap();
    assert!(initial.sources.is_empty());
    assert_eq!(
        db.mutate_sources(SourceMutation::Pause("missing".into()), 0, 1)
            .unwrap_err()
            .code,
        ErrorCode::InvalidQuery
    );
    assert_eq!(
        db.sources_snapshot().unwrap().settings_revision,
        initial.settings_revision
    );
    let candidate = SourceCandidate {
        root: logs.path().into(),
        origin: SourceOrigin::Environment,
    };
    db.mutate_sources(
        SourceMutation::Add(vec![candidate.clone()]),
        initial.settings_revision.value() as i64,
        1,
    )
    .unwrap();
    let added = db.sources_snapshot().unwrap();
    assert_eq!(added.sources.len(), 1);
    let source = &added.sources[0];
    assert_eq!(source.origin, SourceOrigin::Environment);
    assert_eq!(
        source.capabilities.physical_identity,
        CapabilityState::NotProbed
    );
    assert_eq!(source.last_success_at_ms, None);
    let revision = added.settings_revision.value() as i64;
    let id = source.source_id.clone();
    assert_eq!(
        db.mutate_sources(SourceMutation::Pause(id.clone()), revision - 1, 2)
            .err()
            .unwrap()
            .code,
        ErrorCode::RevisionConflict
    );
    db.mutate_sources(SourceMutation::Add(vec![candidate]), revision, 3)
        .unwrap();
    assert_eq!(
        db.sources_snapshot().unwrap().settings_revision,
        added.settings_revision
    );
    db.mutate_sources(SourceMutation::RetainRemove(id.clone()), revision, 4)
        .unwrap();
    let removed = db.sources_snapshot().unwrap();
    assert!(!removed.sources[0].enabled);
    assert!(removed.sources[0].removed);
    assert_eq!(removed.sources[0].readability, SourceReadability::Disabled);
    db.update_source_runtime(
        id.clone(),
        SourceReadability::Readable,
        SourceCapabilities {
            byte_seek: CapabilityState::Available,
            ..Default::default()
        },
        Some(5),
        Some(5),
    )
    .unwrap();
    let after_probe = db.sources_snapshot().unwrap();
    assert!(after_probe.sources[0].removed);
    assert_eq!(after_probe.sources[0].origin, SourceOrigin::Environment);
    assert_eq!(after_probe.settings_revision, removed.settings_revision);
    db.mutate_sources(
        SourceMutation::Resume(id.clone()),
        after_probe.settings_revision.value() as i64,
        6,
    )
    .unwrap();
    let resumed = db.sources_snapshot().unwrap();
    assert!(resumed.sources[0].enabled);
    assert!(!resumed.sources[0].removed);
    db.snapshot(|tx, _| {
        let retained: bool = tx.query_row(
            "SELECT retained FROM sources WHERE source_id=?1",
            [id],
            |r| r.get(0),
        )?;
        assert!(retained);
        Ok(())
    })
    .unwrap();
    assert_eq!(std::fs::read_dir(logs.path()).unwrap().count(), 0);
}
