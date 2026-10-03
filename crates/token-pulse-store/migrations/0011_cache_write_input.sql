-- Retain a nullable included input dimension; old rows remain unknown.
ALTER TABLE usage_events ADD COLUMN cache_write_input_tokens INTEGER
  CHECK(cache_write_input_tokens IS NULL OR
    (typeof(cache_write_input_tokens)='integer' AND cache_write_input_tokens>=0 AND
      (input_tokens_total IS NULL OR cache_write_input_tokens<=input_tokens_total) AND
      (input_tokens_total IS NULL OR cached_input_tokens IS NULL OR
        cache_write_input_tokens<=input_tokens_total-cached_input_tokens)));
UPDATE app_state SET schema_version=11 WHERE singleton=1;
