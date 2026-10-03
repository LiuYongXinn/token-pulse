-- Evidence-gated quote identities stay separate from ordinary rule matching.
-- Their immutable parent rows support the existing event valuation foreign key.
ALTER TABLE price_rules ADD COLUMN request_conditional INTEGER NOT NULL DEFAULT 0
  CHECK(typeof(request_conditional)='integer' AND request_conditional IN (0,1));
CREATE TABLE conditional_price_rules (
  rule_id TEXT PRIMARY KEY REFERENCES price_rules(rule_id),
  catalog_id TEXT NOT NULL REFERENCES offline_price_catalogs(catalog_id),
  model_exact TEXT NOT NULL,
  tier TEXT NOT NULL CHECK(tier IN ('standard','fast','batch','flex','ultrafast')),
  context_band TEXT NOT NULL CHECK(context_band IN ('all','short','long')),
  UNIQUE(catalog_id,model_exact,tier,context_band)
);
UPDATE app_state SET schema_version=14 WHERE singleton=1;
