-- Optional completion metadata has its own bounded reader, independent of token accounting.
CREATE TABLE turn_timing_scans (
  file_generation_id TEXT PRIMARY KEY REFERENCES file_generations(file_generation_id),
  scanned_offset INTEGER NOT NULL CHECK(scanned_offset>=0),
  context_json TEXT NOT NULL CHECK(json_valid(context_json))
);
CREATE TABLE turn_timing_records (
  file_generation_id TEXT NOT NULL REFERENCES file_generations(file_generation_id),
  byte_offset INTEGER NOT NULL CHECK(byte_offset>=0),
  byte_end INTEGER NOT NULL CHECK(byte_end>byte_offset),
  provider_session_id TEXT NOT NULL,
  turn_id TEXT NOT NULL,
  duration_ms INTEGER CHECK(duration_ms IS NULL OR (typeof(duration_ms)='integer' AND duration_ms>=0)),
  time_to_first_token_ms INTEGER CHECK(time_to_first_token_ms IS NULL OR (typeof(time_to_first_token_ms)='integer' AND time_to_first_token_ms>=0)),
  CHECK(duration_ms IS NOT NULL OR time_to_first_token_ms IS NOT NULL),
  CHECK(duration_ms IS NULL OR time_to_first_token_ms IS NULL OR time_to_first_token_ms<=duration_ms),
  PRIMARY KEY(file_generation_id,byte_offset)
);
CREATE INDEX turn_timing_lookup ON turn_timing_records(turn_id,file_generation_id);
CREATE TRIGGER usage_view_turn_timings_insert AFTER INSERT ON turn_timing_records
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;
UPDATE app_state SET schema_version=20 WHERE singleton=1;
