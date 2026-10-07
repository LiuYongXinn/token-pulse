-- Session rows must not rescan the full date range for each ledger.
CREATE INDEX pending_ledger_lookup ON pending_usage(ledger_id,kind,observation_id);
UPDATE app_state SET schema_version=18 WHERE singleton=1;
