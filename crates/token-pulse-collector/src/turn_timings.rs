//! Bounded supplemental scan of already imported bytes, including logs imported before timings existed.
use serde_json::Value;
use std::path::Path;
use token_pulse_core::{
    protocol::validate_request_id,
    reader::{self, FramedLine, ReaderCheckpoint, ReaderLimits},
};
use token_pulse_store::{
    Database, ErrorCode, StoreResult,
    turn_timings::{TimingCheckpoint, TurnTimingRecord},
};

fn nullable_ms(value: Option<&Value>) -> Option<Option<i64>> {
    match value {
        None | Some(Value::Null) => Some(None),
        Some(v) => v.as_i64().filter(|v| *v >= 0).map(Some),
    }
}

fn completion(
    bytes: &[u8],
    state: &mut TimingCheckpoint,
) -> Option<(String, Option<i64>, Option<i64>)> {
    let value: Value = serde_json::from_slice(bytes).ok()?;
    if value["type"] == "session_meta" {
        state.provider_session_id = value["payload"]["id"]
            .as_str()
            .filter(|id| validate_request_id(id).is_ok())
            .map(str::to_owned);
    }
    if value["type"] != "event_msg" || value["payload"]["type"] != "task_complete" {
        return None;
    }
    state.provider_session_id.as_ref()?;
    let payload = value.get("payload")?.as_object()?;
    let turn = payload
        .get("turn_id")?
        .as_str()
        .filter(|id| validate_request_id(id).is_ok())?;
    let duration = nullable_ms(payload.get("duration_ms"))?;
    let first = nullable_ms(payload.get("time_to_first_token_ms"))?;
    if duration.is_none() && first.is_none() || matches!((duration,first),(Some(d),Some(t)) if t>d)
    {
        return None;
    }
    Some((turn.to_owned(), duration, first))
}

pub(crate) fn collect(database: &Database, source: &str, path: &Path) -> StoreResult<(bool, bool)> {
    let saved = database
        .file_checkpoint(source, path.to_str().ok_or(ErrorCode::InvalidQuery)?, None)?
        .ok_or(ErrorCode::CheckpointConflict)?;
    let mut state = database.turn_timing_checkpoint(&saved.file_generation_id)?;
    if state.offset > saved.committed_offset as u64 {
        return Err(ErrorCode::DbCorrupt.into());
    }
    if state.offset == saved.committed_offset as u64 {
        return Ok((false, false));
    }
    let expected = state.offset;
    let base = ReaderCheckpoint {
        file_identity: saved.file_identity.clone(),
        observed_size: Some(saved.observed_size as u64),
        committed_offset: saved.committed_offset as u64,
        anchors: saved.anchors.clone(),
        oversized_line: saved.context.oversized_line.clone(),
        ..Default::default()
    };
    // Verify the committed prefix both sides of the supplemental read, using the source reader's anchors.
    let verify = || {
        reader::read_batch(
            path,
            &saved.file_generation_id,
            &base,
            &ReaderLimits {
                records: 1,
                ..Default::default()
            },
        )
        .map_err(|e| e.code())
    };
    verify()?;
    let batch = reader::read_batch(
        path,
        &saved.file_generation_id,
        &ReaderCheckpoint {
            file_identity: saved.file_identity.clone(),
            observed_size: Some(saved.observed_size as u64),
            committed_offset: state.offset,
            oversized_line: state.oversized_line.clone(),
            ..Default::default()
        },
        &ReaderLimits::default(),
    )
    .map_err(|e| e.code())?;
    let mut records = Vec::new();
    for line in batch.lines {
        match line {
            FramedLine::Complete { position, bytes }
                if position.byte_end <= saved.committed_offset as u64 =>
            {
                if let Some((turn_id, duration_ms, time_to_first_token_ms)) =
                    completion(&bytes, &mut state)
                {
                    records.push(TurnTimingRecord {
                        position,
                        provider_session_id: state.provider_session_id.clone().unwrap(),
                        turn_id,
                        duration_ms,
                        time_to_first_token_ms,
                    });
                }
            }
            FramedLine::Oversized { .. } => {}
            _ => break,
        }
    }
    state.offset = batch.next_offset.min(saved.committed_offset as u64);
    state.oversized_line = batch.oversized_line.filter(|s| {
        s.start_offset == state.offset && s.scan_offset <= saved.committed_offset as u64
    });
    verify()?;
    let more = state.offset < saved.committed_offset as u64;
    let changed = database.publish_turn_timings(saved, expected, state, records)?;
    Ok((changed, more))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_explicit_valid_completion_metrics_are_retained() {
        let mut state = TimingCheckpoint::default();
        completion(
            br#"{"type":"session_meta","payload":{"id":"thread"}}"#,
            &mut state,
        );
        for timing in [
            r#""duration_ms":-1"#,
            r#""duration_ms":1.5"#,
            r#""duration_ms":"12""#,
            r#""duration_ms":1,"time_to_first_token_ms":2"#,
        ] {
            assert!(completion(format!(r#"{{"type":"event_msg","payload":{{"type":"task_complete","turn_id":"turn",{timing}}}}}"#).as_bytes(),&mut state).is_none());
        }
        assert_eq!(completion(br#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"turn","duration_ms":37534,"time_to_first_token_ms":4801}}"#,&mut state),Some(("turn".into(),Some(37534),Some(4801))));
        assert_eq!(completion(br#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"turn","duration_ms":0}}"#,&mut state),Some(("turn".into(),Some(0),None)));
        assert!(completion(br#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"turn","started_at":1,"completed_at":2}}"#,&mut state).is_none());
        completion(
            br#"{"type":"session_meta","payload":{"id":""}}"#,
            &mut state,
        );
        assert!(state.provider_session_id.is_none());
    }
}
