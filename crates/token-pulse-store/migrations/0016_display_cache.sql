CREATE TABLE usage_display_cache (
    scope_key TEXT PRIMARY KEY NOT NULL,
    format_version INTEGER NOT NULL,
    stamp_json TEXT NOT NULL,
    result_json TEXT NOT NULL,
    byte_length INTEGER NOT NULL CHECK(byte_length >= 0 AND byte_length <= 16777216),
    used_sequence INTEGER NOT NULL
) STRICT;
CREATE TRIGGER clear_private_usage_cache_insert AFTER INSERT ON settings
WHEN COALESCE(json_extract(NEW.payload_json,'$.privacy'),0) <> 0
BEGIN DELETE FROM usage_display_cache; END;
CREATE TRIGGER clear_private_usage_cache_update AFTER UPDATE OF payload_json ON settings
WHEN COALESCE(json_extract(NEW.payload_json,'$.privacy'),0) <> 0
BEGIN DELETE FROM usage_display_cache; END;
UPDATE app_state SET schema_version=16 WHERE singleton=1;
