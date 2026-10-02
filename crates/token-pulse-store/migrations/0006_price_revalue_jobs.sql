-- Independent price work; no collector checkpoints or generic file progress fields.
CREATE TABLE price_revalue_jobs (
  job_id TEXT PRIMARY KEY,
  request_key TEXT NOT NULL UNIQUE,
  request_json TEXT NOT NULL CHECK(json_valid(request_json)),
  status_json TEXT NOT NULL CHECK(json_valid(status_json)),
  state TEXT NOT NULL CHECK(state IN ('queued','running','cancelling','succeeded','cancelled','failed','interrupted')),
  automatic INTEGER NOT NULL CHECK(automatic IN (0,1)),
  price_revision INTEGER NOT NULL CHECK(typeof(price_revision)='integer' AND price_revision>=0),
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
);
CREATE INDEX price_revalue_queue ON price_revalue_jobs(state,automatic,created_at_ms,job_id);
CREATE TABLE price_revalue_plan (
  job_id TEXT NOT NULL REFERENCES price_revalue_jobs(job_id) ON DELETE CASCADE,
  position INTEGER NOT NULL CHECK(typeof(position)='integer' AND position>=0),
  ledger_id TEXT NOT NULL REFERENCES ledger_generations(ledger_id),
  event_count INTEGER NOT NULL CHECK(typeof(event_count)='integer' AND event_count>=0),
  processed_count INTEGER NOT NULL DEFAULT 0 CHECK(typeof(processed_count)='integer' AND processed_count>=0 AND processed_count<=event_count),
  completed INTEGER NOT NULL DEFAULT 0 CHECK(completed IN (0,1)),
  PRIMARY KEY(job_id,position),
  UNIQUE(job_id,ledger_id)
);
UPDATE app_state SET schema_version=6 WHERE singleton=1;
