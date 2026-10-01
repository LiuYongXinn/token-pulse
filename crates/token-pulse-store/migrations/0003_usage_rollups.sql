-- Rebuildable projections are separate from the immutable consumption facts.
CREATE TABLE ledger_usage_versions (
  ledger_id TEXT PRIMARY KEY REFERENCES ledger_generations(ledger_id) ON DELETE CASCADE,
  revision INTEGER NOT NULL DEFAULT 0 CHECK(typeof(revision)='integer' AND revision>=0)
);
INSERT INTO ledger_usage_versions(ledger_id,revision) SELECT ledger_id,0 FROM ledger_generations;

CREATE TRIGGER ledger_usage_version_init AFTER INSERT ON ledger_generations
BEGIN INSERT INTO ledger_usage_versions(ledger_id,revision) VALUES(NEW.ledger_id,0); END;
CREATE TRIGGER ledger_usage_event_insert AFTER INSERT ON usage_events
BEGIN UPDATE ledger_usage_versions SET revision=revision+1 WHERE ledger_id=NEW.ledger_id; END;
CREATE TRIGGER ledger_usage_event_delete AFTER DELETE ON usage_events
BEGIN UPDATE ledger_usage_versions SET revision=revision+1 WHERE ledger_id=OLD.ledger_id; END;
CREATE TRIGGER ledger_usage_event_update AFTER UPDATE ON usage_events
BEGIN UPDATE ledger_usage_versions SET revision=revision+1 WHERE ledger_id IN (OLD.ledger_id,NEW.ledger_id); END;
CREATE TRIGGER ledger_usage_provenance_insert AFTER INSERT ON event_provenance
BEGIN UPDATE ledger_usage_versions SET revision=revision+1 WHERE ledger_id=(SELECT ledger_id FROM usage_events WHERE event_id=NEW.event_id); END;
CREATE TRIGGER ledger_usage_provenance_delete AFTER DELETE ON event_provenance
BEGIN UPDATE ledger_usage_versions SET revision=revision+1 WHERE ledger_id=(SELECT ledger_id FROM usage_events WHERE event_id=OLD.event_id); END;
CREATE TRIGGER ledger_usage_provenance_update AFTER UPDATE ON event_provenance
BEGIN UPDATE ledger_usage_versions SET revision=revision+1 WHERE ledger_id IN (SELECT ledger_id FROM usage_events WHERE event_id IN (OLD.event_id,NEW.event_id)); END;
CREATE TRIGGER ledger_usage_provider_update AFTER UPDATE OF normalized_json ON observations
WHEN json_extract(OLD.normalized_json,'$.effective_metadata.provider') IS NOT json_extract(NEW.normalized_json,'$.effective_metadata.provider')
BEGIN UPDATE ledger_usage_versions SET revision=revision+1 WHERE ledger_id IN (SELECT ledger_id FROM usage_events WHERE origin_observation_id=NEW.observation_id); END;

CREATE TABLE usage_rollup_sets (
  set_id TEXT PRIMARY KEY,
  ledger_id TEXT NOT NULL REFERENCES ledger_generations(ledger_id) ON DELETE CASCADE,
  evidence_revision INTEGER NOT NULL CHECK(typeof(evidence_revision)='integer' AND evidence_revision>=0),
  cache_version INTEGER NOT NULL CHECK(cache_version>0),
  parser_version TEXT NOT NULL,
  accounting_version TEXT NOT NULL,
  state TEXT NOT NULL CHECK(state IN ('building','ready','obsolete','failed')),
  created_at_ms INTEGER NOT NULL,
  published_at_ms INTEGER,
  UNIQUE(ledger_id,evidence_revision,cache_version),
  CHECK(state<>'ready' OR published_at_ms IS NOT NULL),
  CHECK(state<>'building' OR published_at_ms IS NULL)
);
CREATE INDEX rollup_sets_lookup ON usage_rollup_sets(ledger_id,evidence_revision,cache_version,state);
CREATE TABLE utc_hour_usage_rollups (
  set_id TEXT NOT NULL REFERENCES usage_rollup_sets(set_id) ON DELETE CASCADE,
  hour_start_ms INTEGER NOT NULL CHECK(hour_start_ms%3600000=0),
  cohort_key TEXT NOT NULL,
  model_provider TEXT,
  model TEXT,
  project_id TEXT REFERENCES projects(project_id),
  source_ids_json TEXT NOT NULL CHECK(json_valid(source_ids_json) AND json_type(source_ids_json)='array'),
  token_sums_json TEXT NOT NULL CHECK(json_valid(token_sums_json) AND json_type(token_sums_json)='object'),
  usage_event_count INTEGER NOT NULL CHECK(usage_event_count>0),
  known_turn_event_count INTEGER NOT NULL CHECK(known_turn_event_count>=0 AND known_turn_event_count<=usage_event_count),
  PRIMARY KEY(set_id,hour_start_ms,cohort_key)
);
CREATE INDEX rollup_hour_lookup ON utc_hour_usage_rollups(hour_start_ms,set_id);
CREATE TABLE utc_hour_rollup_turns (
  set_id TEXT NOT NULL,
  hour_start_ms INTEGER NOT NULL,
  cohort_key TEXT NOT NULL,
  turn_id TEXT NOT NULL CHECK(length(turn_id)>0),
  PRIMARY KEY(set_id,hour_start_ms,cohort_key,turn_id),
  FOREIGN KEY(set_id,hour_start_ms,cohort_key) REFERENCES utc_hour_usage_rollups(set_id,hour_start_ms,cohort_key) ON DELETE CASCADE
);
UPDATE app_state SET schema_version=3 WHERE singleton=1;
