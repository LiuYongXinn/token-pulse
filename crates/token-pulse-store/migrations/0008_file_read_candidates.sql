-- Isolated necessary observations for a replacement generation. No active facts or session rows.
CREATE TABLE file_read_candidates (
  generation_id TEXT PRIMARY KEY REFERENCES file_generations(file_generation_id),
  file_id TEXT NOT NULL REFERENCES source_files(file_id),
  base_json TEXT NOT NULL CHECK(json_valid(base_json)),
  initial_size INTEGER NOT NULL CHECK(typeof(initial_size)='integer' AND initial_size>=0),
  state TEXT NOT NULL CHECK(state IN ('reading','ready','claimed','published','failed')),
  error_code TEXT,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
);
CREATE UNIQUE INDEX file_read_candidate_owner ON file_read_candidates(file_id)
  WHERE state IN ('reading','ready','claimed');
CREATE TABLE file_candidate_observations (
  generation_id TEXT NOT NULL REFERENCES file_read_candidates(generation_id),
  observation_id TEXT NOT NULL UNIQUE,
  byte_offset INTEGER NOT NULL CHECK(typeof(byte_offset)='integer' AND byte_offset>=0),
  byte_end INTEGER NOT NULL CHECK(typeof(byte_end)='integer' AND byte_end>byte_offset),
  session_key TEXT,
  normalized_json TEXT NOT NULL CHECK(json_valid(normalized_json)),
  payload_fingerprint TEXT NOT NULL,
  PRIMARY KEY(generation_id,byte_offset)
);
CREATE TABLE file_candidate_diagnostics (
  generation_id TEXT NOT NULL REFERENCES file_read_candidates(generation_id),
  diagnostic_id TEXT NOT NULL UNIQUE,
  byte_offset INTEGER,
  diagnostic_json TEXT NOT NULL CHECK(json_valid(diagnostic_json)),
  PRIMARY KEY(generation_id,diagnostic_id)
);
UPDATE app_state SET schema_version=8 WHERE singleton=1;
