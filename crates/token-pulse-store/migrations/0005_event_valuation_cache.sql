-- Derived candidate metadata and exact input identities. Consumption facts are untouched.
CREATE TABLE valuation_cache_sets (
  valuation_set_id TEXT PRIMARY KEY REFERENCES valuation_sets(valuation_set_id) ON DELETE CASCADE,
  ledger_id TEXT NOT NULL REFERENCES ledger_generations(ledger_id) ON DELETE CASCADE,
  evidence_revision INTEGER NOT NULL CHECK(typeof(evidence_revision)='integer' AND evidence_revision>=0),
  cache_version INTEGER NOT NULL CHECK(typeof(cache_version)='integer' AND cache_version>0),
  parser_version TEXT NOT NULL,
  accounting_version TEXT NOT NULL,
  event_count INTEGER NOT NULL CHECK(typeof(event_count)='integer' AND event_count>=0),
  content_sha256 TEXT CHECK(content_sha256 IS NULL OR length(content_sha256)=64),
  published_at_ms INTEGER
);
CREATE INDEX valuation_cache_ledger ON valuation_cache_sets(ledger_id,evidence_revision,cache_version);
CREATE TABLE valuation_cache_inputs (
  valuation_set_id TEXT NOT NULL,
  event_id TEXT NOT NULL,
  input_sha256 TEXT NOT NULL CHECK(length(input_sha256)=64),
  PRIMARY KEY(valuation_set_id,event_id),
  FOREIGN KEY(valuation_set_id,event_id) REFERENCES event_valuations(valuation_set_id,event_id) ON DELETE CASCADE
);
CREATE INDEX valuation_cache_input_lookup ON valuation_cache_inputs(event_id,input_sha256,valuation_set_id);
CREATE INDEX valuation_sets_ready_price ON valuation_sets(price_revision,mode,specified_at_ms,state,valuation_set_id);
UPDATE app_state SET schema_version=5 WHERE singleton=1;
