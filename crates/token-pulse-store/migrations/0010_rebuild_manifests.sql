-- A replacement may legitimately have no session/header. Its frozen manifest must still exist.
CREATE TABLE rebuild_manifests (
  job_id TEXT PRIMARY KEY REFERENCES jobs(job_id),
  manifest_json TEXT NOT NULL CHECK(json_valid(manifest_json)),
  created_at_ms INTEGER NOT NULL
);
UPDATE app_state SET schema_version=10 WHERE singleton=1;
