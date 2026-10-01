use std::{
    fs::{self, OpenOptions},
    io::{Seek, SeekFrom, Write},
};
use token_pulse_core::{
    adapter::{AdaptedRecord, adapt},
    domain::*,
    reader::*,
};

fn limits() -> ReaderLimits {
    ReaderLimits {
        block_bytes: 3,
        line_bytes: 128,
        batch_bytes: 256,
        records: 500,
    }
}
fn checkpoint(batch: &ReadBatch) -> ReaderCheckpoint {
    ReaderCheckpoint {
        file_identity: Some(batch.file_identity.clone()),
        observed_size: Some(batch.observed_size),
        committed_offset: batch.next_offset,
        anchors: batch.anchors.clone(),
        oversized_line: batch.oversized_line.clone(),
        modified_at: batch.modified_at,
    }
}
fn complete(batch: &ReadBatch) -> Vec<(u64, u64, Vec<u8>)> {
    batch
        .lines
        .iter()
        .map(|line| match line {
            FramedLine::Complete { position, bytes } => {
                (position.byte_offset, position.byte_end, bytes.clone())
            }
            _ => panic!("expected complete line"),
        })
        .collect()
}
fn position() -> PhysicalPosition {
    PhysicalPosition {
        file_generation_id: "g".into(),
        byte_offset: 0,
        byte_end: 1,
    }
}

#[test]
fn utf8_crlf_and_partial_tail_survive_restart_without_source_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rollout.jsonl");
    let first = "{\"项目\":1}\r\n".as_bytes();
    let partial = b"{\"tail\":";
    let mut original = first.to_vec();
    original.extend(partial);
    fs::write(&path, &original).unwrap();
    let a = read_batch(&path, "g", &ReaderCheckpoint::default(), &limits()).unwrap();
    assert_eq!(
        complete(&a),
        vec![(0, first.len() as u64, first[..first.len() - 2].to_vec())]
    );
    assert_eq!(a.next_offset, first.len() as u64);
    assert!(!a.has_more);
    let b = read_batch(&path, "g", &checkpoint(&a), &limits()).unwrap();
    assert!(b.lines.is_empty());
    assert_eq!(fs::read(&path).unwrap(), original);
    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"2}\n")
        .unwrap();
    let c = read_batch(&path, "g", &checkpoint(&b), &limits()).unwrap();
    assert_eq!(
        complete(&c),
        vec![(
            first.len() as u64,
            (original.len() + 3) as u64,
            b"{\"tail\":2}".to_vec()
        )]
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}
#[test]
fn over_limit_line_is_bounded_and_resumable_until_newline() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large.jsonl");
    fs::write(&path, vec![b'x'; 700]).unwrap();
    let mut cp = ReaderCheckpoint::default();
    for expected in [256, 512, 700] {
        let batch = read_batch(&path, "g", &cp, &limits()).unwrap();
        assert!(batch.lines.is_empty());
        assert_eq!(batch.next_offset, 0);
        assert_eq!(batch.oversized_line.as_ref().unwrap().scan_offset, expected);
        cp = checkpoint(&batch);
    }
    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"\n{}\n")
        .unwrap();
    let batch = read_batch(&path, "g", &cp, &limits()).unwrap();
    assert!(
        matches!(&batch.lines[0],FramedLine::Oversized{position} if position.byte_offset==0 && position.byte_end==701)
    );
    assert!(
        matches!(&batch.lines[1],FramedLine::Complete{position,bytes} if position.byte_offset==701 && position.byte_end==704 && bytes==b"{}")
    );
    assert_eq!(batch.next_offset, 704);
    assert!(batch.oversized_line.is_none());
}
#[test]
fn record_cap_preserves_exact_byte_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cap.jsonl");
    fs::write(&path, b"a\nb\nc\n").unwrap();
    let mut cap = limits();
    cap.records = 2;
    let a = read_batch(&path, "g", &ReaderCheckpoint::default(), &cap).unwrap();
    assert_eq!(a.next_offset, 4);
    assert!(a.has_more);
    let b = read_batch(&path, "g", &checkpoint(&a), &cap).unwrap();
    assert_eq!(complete(&b), vec![(4, 6, b"c".to_vec())]);
}
#[test]
fn middle_overwrite_replacement_truncation_and_archive_move_are_distinguished() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.jsonl");
    let mut bytes = vec![b'a'; 20_000];
    bytes.push(b'\n');
    fs::write(&path, &bytes).unwrap();
    let a = read_batch(
        &path,
        "g",
        &ReaderCheckpoint::default(),
        &ReaderLimits::default(),
    )
    .unwrap();
    let cp = checkpoint(&a);
    let archive = dir.path().join("archive.jsonl");
    fs::rename(&path, &archive).unwrap();
    assert!(read_batch(&archive, "g", &cp, &ReaderLimits::default()).is_ok());
    let mut file = OpenOptions::new().write(true).open(&archive).unwrap();
    file.seek(SeekFrom::Start(10_000)).unwrap();
    file.write_all(b"z").unwrap();
    file.flush().unwrap();
    let mut verify = checkpoint(&a);
    verify.modified_at = None; // Force reconciliation independently of clock granularity.
    assert!(matches!(
        read_batch(&archive, "g", &verify, &ReaderLimits::default()),
        Err(ReadError::InvalidGeneration)
    ));
    file.set_len(10).unwrap();
    drop(file);
    assert!(matches!(
        read_batch(&archive, "g", &cp, &ReaderLimits::default()),
        Err(ReadError::InvalidGeneration)
    ));
    fs::write(&path, &bytes).unwrap();
    assert!(matches!(
        read_batch(&path, "g", &cp, &ReaderLimits::default()),
        Err(ReadError::InvalidGeneration)
    ));
}
#[test]
fn auth_and_non_jsonl_paths_are_rejected_before_open() {
    assert!(matches!(
        read_batch(
            std::path::Path::new("auth.json"),
            "g",
            &ReaderCheckpoint::default(),
            &limits()
        ),
        Err(ReadError::InvalidPath)
    ));
    let mut invalid = limits();
    invalid.block_bytes = usize::MAX;
    assert!(matches!(
        read_batch(
            std::path::Path::new("x.jsonl"),
            "g",
            &ReaderCheckpoint::default(),
            &invalid
        ),
        Err(ReadError::InvalidLimits)
    ));
}
#[test]
fn synthetic_fixture_has_independent_metadata_usage_and_privacy_expectations() {
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/codex-rollout-v1.expected.json"
    ))
    .unwrap();
    let input = include_bytes!("../../../fixtures/codex-rollout-v1.jsonl");
    let mut context = ReaderContext::default();
    let mut kinds = vec![];
    let mut observations = vec![];
    for line in input.split(|&b| b == b'\n').filter(|b| !b.is_empty()) {
        match adapt(line, position(), &mut context) {
            AdaptedRecord::Ignored => kinds.push("ignored"),
            AdaptedRecord::Diagnostic(d) => {
                kinds.push("diagnostic");
                assert_eq!(d.reason, expected["unsupported_reason"]);
            }
            AdaptedRecord::Observation(o) => {
                kinds.push(match &*o {
                    NormalizedObservation::SessionMetadata { .. } => "session_metadata",
                    NormalizedObservation::TurnMetadata { .. } => "turn_metadata",
                    NormalizedObservation::Usage(_) => "usage",
                    _ => panic!(),
                });
                let serialized = serde_json::to_string(&o).unwrap();
                assert!(!serialized.contains("SYNTHETIC_BODY_MUST_NOT_SURVIVE"));
                observations.push(*o);
            }
        }
    }
    assert_eq!(serde_json::json!(kinds), expected["record_kinds"]);
    assert!(
        matches!(&observations[0],NormalizedObservation::SessionMetadata {provider_session_id,metadata,created_at_ms,..} if provider_session_id==expected["session_id"].as_str().unwrap() && metadata.provider.is_none() && *created_at_ms==Some(1000))
    );
    let NormalizedObservation::Usage(first) = &observations[2] else {
        panic!()
    };
    assert_eq!(
        serde_json::to_value(first.last).unwrap(),
        expected["first_usage"]
    );
    assert_eq!(first.event_time_ms, Some(3000));
    assert_eq!(first.model_context_window, Some(1000));
    let NormalizedObservation::Usage(large) = &observations[4] else {
        panic!()
    };
    assert_eq!(
        large.last.unwrap().reported_total,
        Some(9_007_199_254_740_993)
    );
    assert_eq!(large.event_time_ms, None);
    assert_eq!(
        large.effective_metadata.cwd.as_deref(),
        expected["updated_cwd"].as_str()
    );
    assert_eq!(large.effective_metadata.model, None);
    let NormalizedObservation::Usage(negative) = &observations[5] else {
        panic!()
    };
    assert_eq!(negative.last.unwrap().input_total, Some(-1));
}
#[test]
fn damaged_unknown_or_invalid_fields_diagnose_without_mutating_context() {
    let mut context = ReaderContext {
        session_key: Some("session".into()),
        metadata: EffectiveMetadata {
            model: Some("previous".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    for bytes in [b"{bad".as_slice(), b"\xff", br#"{"type":"turn_context","payload":{"model":"new","cwd":2}}"#, br#"{"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":1.5}}}}"#, br#"{"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"cache_write_tokens":1}}}}"#] {
        assert!(matches!(adapt(bytes,position(),&mut context),AdaptedRecord::Diagnostic(_)));
        assert_eq!(context.metadata.model.as_deref(),Some("previous"));
    }
    assert!(matches!(adapt(br#"{"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"total_tokens":1}}}}"#,position(),&mut ReaderContext::default()),AdaptedRecord::Diagnostic(_)));
}

#[test]
fn chunking_changes_neither_normalized_records_nor_effective_context() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fixture.jsonl");
    fs::write(
        &path,
        include_bytes!("../../../fixtures/codex-rollout-v1.jsonl"),
    )
    .unwrap();
    let parse = |record_limit, block_bytes| {
        let limits = ReaderLimits {
            records: record_limit,
            block_bytes,
            ..Default::default()
        };
        let mut cp = ReaderCheckpoint::default();
        let mut context = ReaderContext::default();
        let mut records = vec![];
        loop {
            let batch = read_batch(&path, "g", &cp, &limits).unwrap();
            for line in &batch.lines {
                let FramedLine::Complete { position, bytes } = line else {
                    panic!()
                };
                records.push(match adapt(bytes, position.clone(), &mut context) {
                    AdaptedRecord::Observation(o) => serde_json::to_value(o).unwrap(),
                    AdaptedRecord::Ignored => serde_json::json!({"ignored":position}),
                    AdaptedRecord::Diagnostic(d) => {
                        serde_json::json!({"diagnostic":d.reason,"position":d.position})
                    }
                });
            }
            cp = checkpoint(&batch);
            // Model a persisted/reloaded context at every committed batch.
            context = serde_json::from_str(&serde_json::to_string(&context).unwrap()).unwrap();
            if !batch.has_more {
                break;
            }
        }
        (records, context)
    };
    assert_eq!(parse(500, 65536), parse(2, 1));
}
