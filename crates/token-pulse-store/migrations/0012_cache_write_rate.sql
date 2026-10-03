-- Published history keeps its original three-rate reference semantics.
ALTER TABLE price_rules ADD COLUMN cache_write_rate_atoms TEXT
  CHECK (cache_write_rate_atoms IS NULL OR (
    typeof(cache_write_rate_atoms) = 'text'
    AND length(cache_write_rate_atoms) BETWEEN 1 AND 16
    AND cache_write_rate_atoms NOT GLOB '*[^0-9]*'
    AND (cache_write_rate_atoms = '0' OR substr(cache_write_rate_atoms, 1, 1) <> '0')
    AND (length(cache_write_rate_atoms) < 16 OR cache_write_rate_atoms <= '1000000000000000')
  ));
UPDATE app_state SET schema_version=12 WHERE singleton=1;
