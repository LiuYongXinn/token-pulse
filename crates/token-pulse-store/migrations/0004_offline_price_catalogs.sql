-- Immutable, factual reference publications. Only derived price revisions change.
CREATE TABLE offline_price_catalogs (
  catalog_id TEXT PRIMARY KEY,
  content_sha256 TEXT NOT NULL CHECK(length(content_sha256)=64),
  catalog_json TEXT NOT NULL CHECK(json_valid(catalog_json)),
  introduced_revision INTEGER NOT NULL UNIQUE CHECK(typeof(introduced_revision)='integer' AND introduced_revision>0),
  verified_at_ms INTEGER NOT NULL CHECK(typeof(verified_at_ms)='integer' AND verified_at_ms>=0),
  installed_at_ms INTEGER NOT NULL CHECK(typeof(installed_at_ms)='integer' AND installed_at_ms>=verified_at_ms)
);
UPDATE app_state SET schema_version=4 WHERE singleton=1;
