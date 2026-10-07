-- Lead pending coverage reads from the authoritative observation date, including NULL.
CREATE INDEX observation_usage_time ON observations(
  observed_at_ms,observation_id,file_generation_id,model,project_id,
  json_extract(normalized_json,'$.effective_metadata.provider')
);
CREATE INDEX pending_observation_lookup ON pending_usage(observation_id,kind,ledger_id);
CREATE INDEX session_active_ledger_read ON sessions(active_ledger_id,session_key);
UPDATE app_state SET schema_version=17 WHERE singleton=1;
