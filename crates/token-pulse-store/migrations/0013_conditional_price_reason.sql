-- Extend derived estimate reasons while preserving all historical rows and input identities.
-- Drop the only dependent table before replacing its parent, with foreign keys still enabled.
CREATE TEMP TABLE valuation_inputs_v13 AS SELECT * FROM valuation_cache_inputs;
DROP TABLE valuation_cache_inputs;
CREATE TABLE event_valuations_v13 (
  valuation_set_id TEXT NOT NULL REFERENCES valuation_sets(valuation_set_id),
  event_id TEXT NOT NULL REFERENCES usage_events(event_id) ON DELETE CASCADE,
  rule_id TEXT REFERENCES price_rules(rule_id),
  currency TEXT,
  cost_atoms TEXT,
  status TEXT NOT NULL CHECK(status IN
    ('priced','unknown_model','missing_rule','ambiguous_rule','insufficient_usage',
     'incomplete_pricing_conditions','overflow')),
  PRIMARY KEY(valuation_set_id,event_id),
  CHECK((status='priced' AND cost_atoms IS NOT NULL AND currency IS NOT NULL)
     OR (status!='priced' AND cost_atoms IS NULL))
);
INSERT INTO event_valuations_v13 SELECT * FROM event_valuations;
DROP TABLE event_valuations;
ALTER TABLE event_valuations_v13 RENAME TO event_valuations;
CREATE TABLE valuation_cache_inputs (
  valuation_set_id TEXT NOT NULL,
  event_id TEXT NOT NULL,
  input_sha256 TEXT NOT NULL CHECK(length(input_sha256)=64),
  PRIMARY KEY(valuation_set_id,event_id),
  FOREIGN KEY(valuation_set_id,event_id) REFERENCES event_valuations(valuation_set_id,event_id) ON DELETE CASCADE
);
INSERT INTO valuation_cache_inputs SELECT * FROM valuation_inputs_v13;
DROP TABLE valuation_inputs_v13;
CREATE INDEX valuation_cache_input_lookup ON valuation_cache_inputs(event_id,input_sha256,valuation_set_id);
UPDATE app_state SET schema_version=13 WHERE singleton=1;
