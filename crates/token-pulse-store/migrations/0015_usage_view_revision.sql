-- All visible state participates in the same transaction. Rollback rolls back revisions.
ALTER TABLE app_state ADD COLUMN usage_view_revision INTEGER NOT NULL DEFAULT 0
  CHECK (typeof(usage_view_revision) = 'integer' AND usage_view_revision >= 0);
ALTER TABLE app_state ADD COLUMN database_instance_id TEXT NOT NULL DEFAULT '';
UPDATE app_state SET database_instance_id = lower(hex(randomblob(16))) WHERE singleton = 1;
CREATE TRIGGER usage_view_revisions AFTER UPDATE OF data_revision,price_revision ON app_state
WHEN OLD.data_revision IS NOT NEW.data_revision OR OLD.price_revision IS NOT NEW.price_revision
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_sources_insert AFTER INSERT ON sources
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_sources_update AFTER UPDATE ON sources
WHEN OLD.source_id IS NOT NEW.source_id OR OLD.provider IS NOT NEW.provider OR OLD.root_path IS NOT NEW.root_path OR OLD.directory_identity IS NOT NEW.directory_identity OR OLD.kind IS NOT NEW.kind OR OLD.enabled IS NOT NEW.enabled OR OLD.retained IS NOT NEW.retained OR OLD.readability IS NOT NEW.readability OR OLD.capabilities_json IS NOT NEW.capabilities_json OR OLD.created_at_ms IS NOT NEW.created_at_ms OR OLD.last_scan_at_ms IS NOT NEW.last_scan_at_ms OR OLD.last_success_at_ms IS NOT NEW.last_success_at_ms
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_sources_delete AFTER DELETE ON sources
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_projects_insert AFTER INSERT ON projects
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_projects_update AFTER UPDATE ON projects
WHEN OLD.project_id IS NOT NEW.project_id OR OLD.canonical_cwd IS NOT NEW.canonical_cwd OR OLD.display_name IS NOT NEW.display_name OR OLD.user_alias IS NOT NEW.user_alias OR OLD.normalization_version IS NOT NEW.normalization_version
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_projects_delete AFTER DELETE ON projects
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_sessions_insert AFTER INSERT ON sessions
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_sessions_update AFTER UPDATE ON sessions
WHEN OLD.session_key IS NOT NEW.session_key OR OLD.provider IS NOT NEW.provider OR OLD.provider_session_id IS NOT NEW.provider_session_id OR OLD.identity_status IS NOT NEW.identity_status OR OLD.parent_key IS NOT NEW.parent_key OR OLD.parent_provider_id IS NOT NEW.parent_provider_id OR OLD.created_at_ms IS NOT NEW.created_at_ms OR OLD.last_activity_ms IS NOT NEW.last_activity_ms OR OLD.active_ledger_id IS NOT NEW.active_ledger_id
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_sessions_delete AFTER DELETE ON sessions
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_ledger_generations_insert AFTER INSERT ON ledger_generations
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_ledger_generations_update AFTER UPDATE ON ledger_generations
WHEN OLD.ledger_id IS NOT NEW.ledger_id OR OLD.session_key IS NOT NEW.session_key OR OLD.state IS NOT NEW.state OR OLD.parser_version IS NOT NEW.parser_version OR OLD.accounting_version IS NOT NEW.accounting_version OR OLD.base_data_revision IS NOT NEW.base_data_revision OR OLD.created_at_ms IS NOT NEW.created_at_ms OR OLD.activated_at_ms IS NOT NEW.activated_at_ms OR OLD.input_manifest_json IS NOT NEW.input_manifest_json
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_ledger_generations_delete AFTER DELETE ON ledger_generations
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_source_files_insert AFTER INSERT ON source_files
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_source_files_update AFTER UPDATE ON source_files
WHEN OLD.file_id IS NOT NEW.file_id OR OLD.source_id IS NOT NEW.source_id OR OLD.canonical_path IS NOT NEW.canonical_path OR OLD.file_identity IS NOT NEW.file_identity OR OLD.current_generation_id IS NOT NEW.current_generation_id OR OLD.status IS NOT NEW.status OR OLD.last_seen_at_ms IS NOT NEW.last_seen_at_ms
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_source_files_delete AFTER DELETE ON source_files
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_file_generations_insert AFTER INSERT ON file_generations
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_file_generations_update AFTER UPDATE ON file_generations
WHEN OLD.file_generation_id IS NOT NEW.file_generation_id OR OLD.file_id IS NOT NEW.file_id OR OLD.state IS NOT NEW.state OR OLD.identity_json IS NOT NEW.identity_json OR OLD.observed_size IS NOT NEW.observed_size OR OLD.committed_offset IS NOT NEW.committed_offset OR OLD.checkpoint_revision IS NOT NEW.checkpoint_revision OR OLD.anchor_json IS NOT NEW.anchor_json OR OLD.reader_context_json IS NOT NEW.reader_context_json OR OLD.parser_version IS NOT NEW.parser_version OR OLD.created_at_ms IS NOT NEW.created_at_ms
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_file_generations_delete AFTER DELETE ON file_generations
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_observations_insert AFTER INSERT ON observations
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_observations_update AFTER UPDATE ON observations
WHEN OLD.observation_id IS NOT NEW.observation_id OR OLD.file_generation_id IS NOT NEW.file_generation_id OR OLD.byte_offset IS NOT NEW.byte_offset OR OLD.byte_end IS NOT NEW.byte_end OR OLD.session_key IS NOT NEW.session_key OR OLD.kind IS NOT NEW.kind OR OLD.observed_at_ms IS NOT NEW.observed_at_ms OR OLD.stable_record_id IS NOT NEW.stable_record_id OR OLD.turn_id IS NOT NEW.turn_id OR OLD.stream_hint IS NOT NEW.stream_hint OR OLD.model IS NOT NEW.model OR OLD.project_id IS NOT NEW.project_id OR OLD.normalized_json IS NOT NEW.normalized_json OR OLD.payload_fingerprint IS NOT NEW.payload_fingerprint OR OLD.format_version IS NOT NEW.format_version
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_observations_delete AFTER DELETE ON observations
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_usage_events_insert AFTER INSERT ON usage_events
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_usage_events_update AFTER UPDATE ON usage_events
WHEN OLD.event_id IS NOT NEW.event_id OR OLD.ledger_id IS NOT NEW.ledger_id OR OLD.origin_observation_id IS NOT NEW.origin_observation_id OR OLD.occurred_at_ms IS NOT NEW.occurred_at_ms OR OLD.semantic_key IS NOT NEW.semantic_key OR OLD.episode_id IS NOT NEW.episode_id OR OLD.model IS NOT NEW.model OR OLD.project_id IS NOT NEW.project_id OR OLD.turn_id IS NOT NEW.turn_id OR OLD.input_tokens_total IS NOT NEW.input_tokens_total OR OLD.cached_input_tokens IS NOT NEW.cached_input_tokens OR OLD.output_tokens_total IS NOT NEW.output_tokens_total OR OLD.reasoning_output_tokens IS NOT NEW.reasoning_output_tokens OR OLD.source_total_tokens IS NOT NEW.source_total_tokens OR OLD.total_tokens IS NOT NEW.total_tokens OR OLD.calculation_method IS NOT NEW.calculation_method OR OLD.quality_json IS NOT NEW.quality_json OR OLD.cache_write_input_tokens IS NOT NEW.cache_write_input_tokens
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_usage_events_delete AFTER DELETE ON usage_events
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_event_provenance_insert AFTER INSERT ON event_provenance
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_event_provenance_update AFTER UPDATE ON event_provenance
WHEN OLD.event_id IS NOT NEW.event_id OR OLD.observation_id IS NOT NEW.observation_id OR OLD.relation IS NOT NEW.relation
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_event_provenance_delete AFTER DELETE ON event_provenance
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_pending_usage_insert AFTER INSERT ON pending_usage
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_pending_usage_update AFTER UPDATE ON pending_usage
WHEN OLD.pending_id IS NOT NEW.pending_id OR OLD.ledger_id IS NOT NEW.ledger_id OR OLD.observation_id IS NOT NEW.observation_id OR OLD.kind IS NOT NEW.kind OR OLD.reason_code IS NOT NEW.reason_code OR OLD.vector_json IS NOT NEW.vector_json OR OLD.evidence_json IS NOT NEW.evidence_json
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_pending_usage_delete AFTER DELETE ON pending_usage
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_context_snapshots_insert AFTER INSERT ON context_snapshots
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_context_snapshots_update AFTER UPDATE ON context_snapshots
WHEN OLD.context_id IS NOT NEW.context_id OR OLD.ledger_id IS NOT NEW.ledger_id OR OLD.observation_id IS NOT NEW.observation_id OR OLD.observed_at_ms IS NOT NEW.observed_at_ms OR OLD.model IS NOT NEW.model OR OLD.context_tokens IS NOT NEW.context_tokens OR OLD.model_context_window IS NOT NEW.model_context_window OR OLD.usage_json IS NOT NEW.usage_json OR OLD.quality_json IS NOT NEW.quality_json
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_context_snapshots_delete AFTER DELETE ON context_snapshots
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_diagnostics_insert AFTER INSERT ON diagnostics
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_diagnostics_update AFTER UPDATE ON diagnostics
WHEN OLD.diagnostic_id IS NOT NEW.diagnostic_id OR OLD.source_id IS NOT NEW.source_id OR OLD.file_generation_id IS NOT NEW.file_generation_id OR OLD.byte_offset IS NOT NEW.byte_offset OR OLD.session_key IS NOT NEW.session_key OR OLD.code IS NOT NEW.code OR OLD.severity IS NOT NEW.severity OR OLD.metadata_json IS NOT NEW.metadata_json OR OLD.dedup_key IS NOT NEW.dedup_key OR OLD.occurrences IS NOT NEW.occurrences OR OLD.first_seen_at_ms IS NOT NEW.first_seen_at_ms OR OLD.last_seen_at_ms IS NOT NEW.last_seen_at_ms OR OLD.resolved_at_ms IS NOT NEW.resolved_at_ms
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_diagnostics_delete AFTER DELETE ON diagnostics
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_source_scan_state_insert AFTER INSERT ON source_scan_state
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_source_scan_state_update AFTER UPDATE ON source_scan_state
WHEN OLD.source_id IS NOT NEW.source_id OR OLD.scan_revision IS NOT NEW.scan_revision OR OLD.source_root IS NOT NEW.source_root OR OLD.state IS NOT NEW.state OR OLD.discovery_complete IS NOT NEW.discovery_complete OR OLD.invalidated IS NOT NEW.invalidated OR OLD.issue_code IS NOT NEW.issue_code OR OLD.started_at_ms IS NOT NEW.started_at_ms OR OLD.completed_at_ms IS NOT NEW.completed_at_ms
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_source_scan_state_delete AFTER DELETE ON source_scan_state
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_source_scan_files_insert AFTER INSERT ON source_scan_files
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_source_scan_files_update AFTER UPDATE ON source_scan_files
WHEN OLD.source_id IS NOT NEW.source_id OR OLD.scan_revision IS NOT NEW.scan_revision OR OLD.canonical_path IS NOT NEW.canonical_path OR OLD.upper_bound IS NOT NEW.upper_bound OR OLD.file_id IS NOT NEW.file_id OR OLD.file_generation_id IS NOT NEW.file_generation_id OR OLD.checkpoint_revision IS NOT NEW.checkpoint_revision OR OLD.checked_at_ms IS NOT NEW.checked_at_ms
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_source_scan_files_delete AFTER DELETE ON source_scan_files
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_session_aliases_insert AFTER INSERT ON session_aliases
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_session_aliases_update AFTER UPDATE ON session_aliases
WHEN OLD.alias_session_key IS NOT NEW.alias_session_key OR OLD.canonical_session_key IS NOT NEW.canonical_session_key OR OLD.evidence_job_id IS NOT NEW.evidence_job_id
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_session_aliases_delete AFTER DELETE ON session_aliases
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_file_session_bindings_insert AFTER INSERT ON file_session_bindings
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_file_session_bindings_update AFTER UPDATE ON file_session_bindings
WHEN OLD.file_generation_id IS NOT NEW.file_generation_id OR OLD.session_key IS NOT NEW.session_key OR OLD.first_offset IS NOT NEW.first_offset OR OLD.identity_evidence IS NOT NEW.identity_evidence
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_file_session_bindings_delete AFTER DELETE ON file_session_bindings
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_notify_integrations_insert AFTER INSERT ON notify_integrations
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_notify_integrations_update AFTER UPDATE ON notify_integrations
WHEN OLD.integration_id IS NOT NEW.integration_id OR OLD.source_id IS NOT NEW.source_id OR OLD.config_path IS NOT NEW.config_path OR OLD.original_notify_json IS NOT NEW.original_notify_json OR OLD.owned_notify_json IS NOT NEW.owned_notify_json OR OLD.current_value_hash IS NOT NEW.current_value_hash OR OLD.state IS NOT NEW.state OR OLD.updated_at_ms IS NOT NEW.updated_at_ms
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;

CREATE TRIGGER usage_view_notify_integrations_delete AFTER DELETE ON notify_integrations
BEGIN UPDATE app_state SET usage_view_revision=usage_view_revision+1 WHERE singleton=1; END;
UPDATE app_state SET schema_version=15 WHERE singleton=1;
