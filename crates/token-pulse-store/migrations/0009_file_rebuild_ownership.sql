-- A replacement's necessary observations can be registered only by its owning rebuild.
CREATE TABLE file_rebuild_candidates (
  generation_id TEXT PRIMARY KEY REFERENCES file_read_candidates(generation_id),
  job_id TEXT NOT NULL REFERENCES jobs(job_id),
  checkpoint_revision INTEGER NOT NULL CHECK(typeof(checkpoint_revision)='integer' AND checkpoint_revision>=0),
  after_offset INTEGER NOT NULL DEFAULT -1 CHECK(typeof(after_offset)='integer' AND after_offset>=-1),
  materialized INTEGER NOT NULL DEFAULT 0 CHECK(materialized IN (0,1)),
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
);
CREATE INDEX file_rebuild_candidates_job ON file_rebuild_candidates(job_id,generation_id);
-- Keep proposed header evidence separate from a published session's existing identity.
CREATE TABLE file_rebuild_sessions (
  generation_id TEXT NOT NULL REFERENCES file_rebuild_candidates(generation_id),
  session_key TEXT NOT NULL REFERENCES sessions(session_key),
  provider_session_id TEXT NOT NULL,
  parent_provider_id TEXT,
  created_at_ms INTEGER,
  PRIMARY KEY(generation_id,session_key)
);
UPDATE app_state SET schema_version=9 WHERE singleton=1;
