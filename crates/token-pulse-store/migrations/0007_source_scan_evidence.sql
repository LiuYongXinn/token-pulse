-- Current bounded scan evidence. Old readers retain the previous run in their real snapshot.
CREATE TABLE source_scan_state (
  source_id TEXT PRIMARY KEY REFERENCES sources(source_id),
  scan_revision INTEGER NOT NULL CHECK(typeof(scan_revision)='integer' AND scan_revision>=1),
  source_root TEXT NOT NULL,
  state TEXT NOT NULL CHECK(state IN ('scanning','incomplete','ready','interrupted')),
  discovery_complete INTEGER NOT NULL DEFAULT 0 CHECK(discovery_complete IN (0,1)),
  invalidated INTEGER NOT NULL DEFAULT 0 CHECK(invalidated IN (0,1)),
  issue_code TEXT,
  started_at_ms INTEGER NOT NULL,
  completed_at_ms INTEGER,
  UNIQUE(source_id,scan_revision)
);
CREATE TABLE source_scan_files (
  source_id TEXT NOT NULL,
  scan_revision INTEGER NOT NULL,
  canonical_path TEXT NOT NULL,
  upper_bound INTEGER NOT NULL CHECK(typeof(upper_bound)='integer' AND upper_bound>=0),
  file_id TEXT REFERENCES source_files(file_id),
  file_generation_id TEXT REFERENCES file_generations(file_generation_id),
  checkpoint_revision INTEGER CHECK(checkpoint_revision IS NULL OR (typeof(checkpoint_revision)='integer' AND checkpoint_revision>=0)),
  checked_at_ms INTEGER,
  PRIMARY KEY(source_id,scan_revision,canonical_path),
  FOREIGN KEY(source_id,scan_revision) REFERENCES source_scan_state(source_id,scan_revision) ON DELETE CASCADE,
  CHECK((file_generation_id IS NULL AND checkpoint_revision IS NULL) OR (file_generation_id IS NOT NULL AND checkpoint_revision IS NOT NULL AND file_id IS NOT NULL))
);
CREATE INDEX source_scan_file_identity ON source_scan_files(source_id,file_id);
UPDATE app_state SET schema_version=7 WHERE singleton=1;
