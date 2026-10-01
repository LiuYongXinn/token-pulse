PRAGMA foreign_keys = ON;

CREATE TABLE app_state (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  schema_version INTEGER NOT NULL,
  data_revision INTEGER NOT NULL DEFAULT 0 CHECK (data_revision >= 0),
  price_revision INTEGER NOT NULL DEFAULT 0 CHECK (price_revision >= 0),
  settings_revision INTEGER NOT NULL DEFAULT 0 CHECK (settings_revision >= 0)
);
INSERT INTO app_state(singleton, schema_version) VALUES (1, 1);

CREATE TABLE settings (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  settings_version INTEGER NOT NULL,
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
  updated_at_ms INTEGER NOT NULL
);

CREATE TABLE sources (
  source_id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  root_path TEXT NOT NULL,
  directory_identity TEXT,
  kind TEXT NOT NULL CHECK (kind IN ('local','wsl','mirror')),
  enabled INTEGER NOT NULL CHECK (enabled IN (0,1)),
  retained INTEGER NOT NULL DEFAULT 1 CHECK (retained IN (0,1)),
  readability TEXT NOT NULL,
  capabilities_json TEXT NOT NULL CHECK (json_valid(capabilities_json)),
  created_at_ms INTEGER NOT NULL,
  last_scan_at_ms INTEGER,
  last_success_at_ms INTEGER
);
CREATE UNIQUE INDEX sources_physical_directory
  ON sources(provider, directory_identity) WHERE directory_identity IS NOT NULL;

CREATE TABLE projects (
  project_id TEXT PRIMARY KEY,
  canonical_cwd TEXT NOT NULL UNIQUE,
  display_name TEXT NOT NULL,
  user_alias TEXT,
  normalization_version INTEGER NOT NULL
);

CREATE TABLE sessions (
  session_key TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  provider_session_id TEXT,
  identity_status TEXT NOT NULL,
  parent_key TEXT REFERENCES sessions(session_key),
  parent_provider_id TEXT,
  created_at_ms INTEGER,
  last_activity_ms INTEGER,
  active_ledger_id TEXT,
  FOREIGN KEY(session_key, active_ledger_id)
    REFERENCES ledger_generations(session_key, ledger_id)
);
CREATE INDEX sessions_provider_id ON sessions(provider, provider_session_id);

CREATE TABLE ledger_generations (
  ledger_id TEXT PRIMARY KEY,
  session_key TEXT NOT NULL REFERENCES sessions(session_key),
  state TEXT NOT NULL CHECK (state IN ('candidate','active','retired','failed')),
  parser_version TEXT NOT NULL,
  accounting_version TEXT NOT NULL,
  base_data_revision INTEGER NOT NULL,
  created_at_ms INTEGER NOT NULL,
  activated_at_ms INTEGER,
  input_manifest_json TEXT NOT NULL CHECK (json_valid(input_manifest_json)),
  UNIQUE(session_key, ledger_id)
);
CREATE UNIQUE INDEX ledger_one_active
  ON ledger_generations(session_key) WHERE state = 'active';

CREATE TABLE source_files (
  file_id TEXT PRIMARY KEY,
  source_id TEXT NOT NULL REFERENCES sources(source_id),
  canonical_path TEXT NOT NULL,
  file_identity TEXT,
  current_generation_id TEXT,
  status TEXT NOT NULL,
  last_seen_at_ms INTEGER,
  FOREIGN KEY(file_id, current_generation_id)
    REFERENCES file_generations(file_id, file_generation_id),
  UNIQUE(source_id, canonical_path)
);

CREATE TABLE file_generations (
  file_generation_id TEXT PRIMARY KEY,
  file_id TEXT NOT NULL REFERENCES source_files(file_id),
  state TEXT NOT NULL CHECK (state IN ('current','candidate','retired','invalid')),
  identity_json TEXT NOT NULL CHECK (json_valid(identity_json)),
  observed_size INTEGER NOT NULL CHECK (observed_size >= 0),
  committed_offset INTEGER NOT NULL DEFAULT 0 CHECK (committed_offset >= 0),
  checkpoint_revision INTEGER NOT NULL DEFAULT 0,
  anchor_json TEXT NOT NULL CHECK (json_valid(anchor_json)),
  reader_context_json TEXT NOT NULL CHECK (json_valid(reader_context_json)),
  parser_version TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL,
  UNIQUE(file_id, file_generation_id)
);

CREATE TABLE observations (
  observation_id TEXT PRIMARY KEY,
  file_generation_id TEXT NOT NULL REFERENCES file_generations(file_generation_id),
  byte_offset INTEGER NOT NULL CHECK (byte_offset >= 0),
  byte_end INTEGER NOT NULL CHECK (byte_end > byte_offset),
  session_key TEXT REFERENCES sessions(session_key),
  kind TEXT NOT NULL CHECK (kind IN ('session_meta','turn_meta','usage','context')),
  observed_at_ms INTEGER,
  stable_record_id TEXT,
  turn_id TEXT,
  stream_hint TEXT,
  model TEXT,
  project_id TEXT REFERENCES projects(project_id),
  normalized_json TEXT NOT NULL CHECK (json_valid(normalized_json)),
  payload_fingerprint TEXT NOT NULL,
  format_version TEXT NOT NULL,
  UNIQUE(file_generation_id, byte_offset)
);
CREATE INDEX observations_session_order
  ON observations(session_key, observed_at_ms, file_generation_id, byte_offset);
CREATE INDEX observations_fingerprint ON observations(session_key, payload_fingerprint);

CREATE TABLE stream_states (
  ledger_id TEXT NOT NULL REFERENCES ledger_generations(ledger_id),
  stream_key TEXT NOT NULL,
  episode_id TEXT NOT NULL,
  baseline_json TEXT NOT NULL CHECK (json_valid(baseline_json)),
  last_observation_id TEXT REFERENCES observations(observation_id),
  lineage_quality TEXT NOT NULL,
  state_revision INTEGER NOT NULL,
  PRIMARY KEY(ledger_id, stream_key, episode_id)
);

CREATE TABLE usage_events (
  event_id TEXT PRIMARY KEY,
  ledger_id TEXT NOT NULL REFERENCES ledger_generations(ledger_id),
  origin_observation_id TEXT NOT NULL REFERENCES observations(observation_id),
  occurred_at_ms INTEGER NOT NULL,
  semantic_key TEXT,
  episode_id TEXT NOT NULL,
  model TEXT,
  project_id TEXT REFERENCES projects(project_id),
  turn_id TEXT,
  input_tokens_total INTEGER CHECK (input_tokens_total >= 0),
  cached_input_tokens INTEGER CHECK (cached_input_tokens >= 0),
  output_tokens_total INTEGER CHECK (output_tokens_total >= 0),
  reasoning_output_tokens INTEGER CHECK (reasoning_output_tokens >= 0),
  source_total_tokens INTEGER CHECK (source_total_tokens >= 0),
  total_tokens INTEGER NOT NULL CHECK (total_tokens >= 0),
  calculation_method TEXT NOT NULL,
  quality_json TEXT NOT NULL CHECK (json_valid(quality_json)),
  CHECK (cached_input_tokens IS NULL OR input_tokens_total IS NULL
         OR cached_input_tokens <= input_tokens_total),
  CHECK (reasoning_output_tokens IS NULL OR output_tokens_total IS NULL
         OR reasoning_output_tokens <= output_tokens_total),
  CHECK (input_tokens_total IS NULL OR output_tokens_total IS NULL
         OR total_tokens = input_tokens_total + output_tokens_total),
  CHECK (source_total_tokens IS NULL OR source_total_tokens = total_tokens),
  UNIQUE(ledger_id, origin_observation_id)
);
CREATE UNIQUE INDEX events_stable_identity
  ON usage_events(ledger_id, semantic_key) WHERE semantic_key IS NOT NULL;
CREATE INDEX events_time ON usage_events(occurred_at_ms, ledger_id);
CREATE INDEX events_ledger_time ON usage_events(ledger_id, occurred_at_ms, event_id);
CREATE INDEX events_model_time ON usage_events(model, occurred_at_ms);
CREATE INDEX events_project_time ON usage_events(project_id, occurred_at_ms);

CREATE TABLE event_provenance (
  event_id TEXT NOT NULL REFERENCES usage_events(event_id) ON DELETE CASCADE,
  observation_id TEXT NOT NULL REFERENCES observations(observation_id),
  relation TEXT NOT NULL CHECK (relation IN ('origin','mirror','replay')),
  PRIMARY KEY(event_id, observation_id)
);
CREATE INDEX provenance_observation ON event_provenance(observation_id, event_id);

CREATE TABLE pending_usage (
  pending_id TEXT PRIMARY KEY,
  ledger_id TEXT NOT NULL REFERENCES ledger_generations(ledger_id),
  observation_id TEXT NOT NULL REFERENCES observations(observation_id),
  kind TEXT NOT NULL CHECK (kind IN ('pending','inherited','duplicate','unattributed')),
  reason_code TEXT NOT NULL,
  vector_json TEXT CHECK (vector_json IS NULL OR json_valid(vector_json)),
  evidence_json TEXT NOT NULL CHECK (json_valid(evidence_json)),
  UNIQUE(ledger_id, observation_id, kind)
);

CREATE TABLE context_snapshots (
  context_id TEXT PRIMARY KEY,
  ledger_id TEXT NOT NULL REFERENCES ledger_generations(ledger_id),
  observation_id TEXT NOT NULL REFERENCES observations(observation_id),
  observed_at_ms INTEGER NOT NULL,
  model TEXT,
  context_tokens INTEGER CHECK (context_tokens >= 0),
  model_context_window INTEGER CHECK (model_context_window > 0),
  usage_json TEXT NOT NULL CHECK (json_valid(usage_json)),
  quality_json TEXT NOT NULL CHECK (json_valid(quality_json))
);
CREATE INDEX context_latest ON context_snapshots(ledger_id, observed_at_ms DESC);

CREATE VIEW active_usage_events AS
  SELECT e.*, s.session_key
  FROM usage_events e JOIN sessions s ON s.active_ledger_id = e.ledger_id;

CREATE TABLE price_rules (
  rule_id TEXT PRIMARY KEY,
  introduced_revision INTEGER NOT NULL,
  retired_revision INTEGER,
  provider TEXT NOT NULL,
  model_exact TEXT NOT NULL,
  source_id TEXT REFERENCES sources(source_id),
  currency TEXT NOT NULL CHECK (length(currency) = 3),
  effective_from_ms INTEGER NOT NULL,
  effective_to_ms INTEGER,
  priority INTEGER NOT NULL,
  input_rate_atoms TEXT NOT NULL,
  cached_rate_atoms TEXT,
  output_rate_atoms TEXT NOT NULL,
  origin TEXT NOT NULL,
  origin_reference TEXT,
  created_at_ms INTEGER NOT NULL,
  CHECK (effective_to_ms IS NULL OR effective_to_ms > effective_from_ms)
);
CREATE INDEX prices_lookup ON price_rules(provider, model_exact, effective_from_ms);

CREATE TABLE model_aliases (
  alias_id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  alias TEXT NOT NULL,
  canonical_model TEXT NOT NULL,
  introduced_revision INTEGER NOT NULL,
  retired_revision INTEGER
);

CREATE TABLE valuation_sets (
  valuation_set_id TEXT PRIMARY KEY,
  price_revision INTEGER NOT NULL,
  mode TEXT NOT NULL CHECK (mode IN ('event_time','specified_time')),
  specified_at_ms INTEGER,
  state TEXT NOT NULL CHECK (state IN ('building','ready','retired','failed')),
  created_at_ms INTEGER NOT NULL,
  CHECK ((mode = 'event_time' AND specified_at_ms IS NULL)
      OR (mode = 'specified_time' AND specified_at_ms IS NOT NULL))
);

CREATE TABLE event_valuations (
  valuation_set_id TEXT NOT NULL REFERENCES valuation_sets(valuation_set_id),
  event_id TEXT NOT NULL REFERENCES usage_events(event_id) ON DELETE CASCADE,
  rule_id TEXT REFERENCES price_rules(rule_id),
  currency TEXT,
  cost_atoms TEXT,
  status TEXT NOT NULL CHECK (status IN
    ('priced','unknown_model','missing_rule','ambiguous_rule','insufficient_usage','overflow')),
  PRIMARY KEY(valuation_set_id, event_id),
  CHECK ((status = 'priced' AND cost_atoms IS NOT NULL AND currency IS NOT NULL)
      OR (status != 'priced' AND cost_atoms IS NULL))
);

CREATE TABLE jobs (
  job_id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  state TEXT NOT NULL,
  request_key TEXT UNIQUE,
  scope_json TEXT NOT NULL CHECK (json_valid(scope_json)),
  progress_json TEXT NOT NULL CHECK (json_valid(progress_json)),
  resume_json TEXT NOT NULL CHECK (json_valid(resume_json)),
  cancel_requested INTEGER NOT NULL DEFAULT 0 CHECK (cancel_requested IN (0,1)),
  error_code TEXT,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
);
CREATE INDEX jobs_state ON jobs(state, updated_at_ms);

CREATE TABLE diagnostics (
  diagnostic_id TEXT PRIMARY KEY,
  source_id TEXT REFERENCES sources(source_id),
  file_generation_id TEXT REFERENCES file_generations(file_generation_id),
  byte_offset INTEGER,
  session_key TEXT REFERENCES sessions(session_key),
  code TEXT NOT NULL,
  severity TEXT NOT NULL,
  metadata_json TEXT NOT NULL CHECK (json_valid(metadata_json)),
  dedup_key TEXT NOT NULL UNIQUE,
  occurrences INTEGER NOT NULL DEFAULT 1,
  first_seen_at_ms INTEGER NOT NULL,
  last_seen_at_ms INTEGER NOT NULL,
  resolved_at_ms INTEGER
);

CREATE TABLE rebuild_audits (
  audit_id TEXT PRIMARY KEY,
  job_id TEXT REFERENCES jobs(job_id),
  session_key TEXT NOT NULL REFERENCES sessions(session_key),
  old_ledger_id TEXT REFERENCES ledger_generations(ledger_id),
  new_ledger_id TEXT REFERENCES ledger_generations(ledger_id),
  reason TEXT NOT NULL,
  difference_json TEXT NOT NULL CHECK (json_valid(difference_json)),
  committed_data_revision INTEGER NOT NULL,
  created_at_ms INTEGER NOT NULL
);

CREATE TABLE source_scan_runs (
  scan_id TEXT PRIMARY KEY,
  source_id TEXT NOT NULL REFERENCES sources(source_id),
  job_id TEXT REFERENCES jobs(job_id),
  scope TEXT NOT NULL CHECK (scope IN ('recent','full','manual')),
  state TEXT NOT NULL CHECK (state IN ('running','complete','partial','failed','interrupted')),
  discovery_complete INTEGER NOT NULL DEFAULT 0 CHECK (discovery_complete IN (0,1)),
  discovered_files INTEGER NOT NULL DEFAULT 0 CHECK (discovered_files >= 0),
  processed_files INTEGER NOT NULL DEFAULT 0 CHECK (processed_files >= 0),
  unreadable_files INTEGER NOT NULL DEFAULT 0 CHECK (unreadable_files >= 0),
  started_at_ms INTEGER NOT NULL,
  finished_at_ms INTEGER,
  manifest_hash TEXT
);
CREATE INDEX scans_source_time ON source_scan_runs(source_id, started_at_ms DESC);

CREATE TABLE scan_file_entries (
  scan_id TEXT NOT NULL REFERENCES source_scan_runs(scan_id) ON DELETE CASCADE,
  file_id TEXT NOT NULL REFERENCES source_files(file_id),
  file_generation_id TEXT REFERENCES file_generations(file_generation_id),
  state TEXT NOT NULL CHECK (state IN ('discovered','processed','unreadable','changed','missing')),
  target_size INTEGER CHECK (target_size >= 0),
  error_code TEXT,
  PRIMARY KEY(scan_id, file_id)
);

CREATE TABLE file_session_bindings (
  file_generation_id TEXT NOT NULL REFERENCES file_generations(file_generation_id),
  session_key TEXT NOT NULL REFERENCES sessions(session_key),
  first_offset INTEGER NOT NULL CHECK (first_offset >= 0),
  identity_evidence TEXT NOT NULL,
  PRIMARY KEY(file_generation_id, session_key)
);

CREATE TABLE notify_integrations (
  integration_id TEXT PRIMARY KEY,
  source_id TEXT NOT NULL REFERENCES sources(source_id),
  config_path TEXT NOT NULL,
  original_notify_json TEXT CHECK (original_notify_json IS NULL OR json_valid(original_notify_json)),
  owned_notify_json TEXT NOT NULL CHECK (json_valid(owned_notify_json)),
  current_value_hash TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('prepared','applied','conflict','restored')),
  updated_at_ms INTEGER NOT NULL
);
