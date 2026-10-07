-- Titles are optional display metadata; they never replace accounting identities.
CREATE TABLE session_titles (
  source_id TEXT NOT NULL REFERENCES sources(source_id),
  provider_session_id TEXT NOT NULL,
  title TEXT NOT NULL CHECK(length(title)>0),
  updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>=0),
  PRIMARY KEY(source_id,provider_session_id)
);
CREATE INDEX session_title_lookup ON session_titles(provider_session_id,updated_at_ms DESC,source_id);
CREATE VIEW session_labels AS
SELECT s.*,COALESCE((SELECT t.title FROM session_titles t
  WHERE t.provider_session_id=s.provider_session_id
  ORDER BY t.updated_at_ms DESC,t.source_id LIMIT 1),s.provider_session_id,s.session_key) AS display_name
FROM sessions s;
CREATE TRIGGER usage_view_session_titles_insert AFTER INSERT ON session_titles
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;
CREATE TRIGGER usage_view_session_titles_update AFTER UPDATE ON session_titles
WHEN OLD.title IS NOT NEW.title OR OLD.updated_at_ms IS NOT NEW.updated_at_ms
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;
CREATE TRIGGER usage_view_session_titles_delete AFTER DELETE ON session_titles
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;
UPDATE app_state SET schema_version=19 WHERE singleton=1;
