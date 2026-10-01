CREATE TABLE stream_frontiers (
  ledger_id TEXT NOT NULL,
  stream_key TEXT NOT NULL,
  episode_id TEXT NOT NULL,
  PRIMARY KEY(ledger_id, stream_key),
  FOREIGN KEY(ledger_id, stream_key, episode_id)
    REFERENCES stream_states(ledger_id, stream_key, episode_id)
);
INSERT INTO stream_frontiers(ledger_id,stream_key,episode_id)
SELECT ledger_id,stream_key,episode_id FROM (
  SELECT st.ledger_id,st.stream_key,st.episode_id,
    ROW_NUMBER() OVER(PARTITION BY st.ledger_id,st.stream_key ORDER BY o.rowid DESC,st.state_revision DESC,st.episode_id) AS rank
  FROM stream_states st JOIN observations o ON o.observation_id=st.last_observation_id
) WHERE rank=1;

CREATE TABLE session_aliases (
  alias_session_key TEXT PRIMARY KEY REFERENCES sessions(session_key),
  canonical_session_key TEXT NOT NULL REFERENCES sessions(session_key),
  evidence_job_id TEXT NOT NULL REFERENCES jobs(job_id),
  CHECK(alias_session_key <> canonical_session_key)
);
CREATE INDEX aliases_canonical ON session_aliases(canonical_session_key);
CREATE TABLE candidate_session_aliases (
  job_id TEXT NOT NULL REFERENCES jobs(job_id),
  alias_session_key TEXT NOT NULL REFERENCES sessions(session_key),
  canonical_session_key TEXT NOT NULL REFERENCES sessions(session_key),
  evidence_json TEXT NOT NULL CHECK(json_valid(evidence_json)),
  PRIMARY KEY(job_id,alias_session_key),
  CHECK(alias_session_key <> canonical_session_key)
);
CREATE TABLE canonical_usage_sequence (
  ledger_id TEXT NOT NULL REFERENCES ledger_generations(ledger_id),
  ordinal INTEGER NOT NULL CHECK(ordinal>=0),
  observation_id TEXT NOT NULL REFERENCES observations(observation_id),
  event_id TEXT REFERENCES usage_events(event_id),
  PRIMARY KEY(ledger_id,ordinal),
  UNIQUE(ledger_id,observation_id)
);
CREATE TABLE file_usage_cursors (
  ledger_id TEXT NOT NULL REFERENCES ledger_generations(ledger_id),
  file_generation_id TEXT NOT NULL REFERENCES file_generations(file_generation_id),
  next_ordinal INTEGER NOT NULL CHECK(next_ordinal>=0),
  state TEXT NOT NULL CHECK(state IN ('aligned','rebuild_required')),
  PRIMARY KEY(ledger_id,file_generation_id)
);
CREATE INDEX sessions_parent_provider ON sessions(provider,parent_provider_id);
CREATE INDEX sessions_parent_key ON sessions(parent_key);
CREATE INDEX bindings_session ON file_session_bindings(session_key,file_generation_id);
UPDATE app_state SET schema_version=2 WHERE singleton=1;
