// Generated from token-pulse-core Rust DTOs. Run npm run contracts; do not edit.

export type DecimalInt = string;

export type DecimalMoney = string;

export type EpochMs = number;

export type ServiceState = "not_configured" | "not_implemented" | "ready" | "error";

export type UpdatePhase = "unavailable" | "idle" | "checking" | "current" | "available" | "downloading" | "verifying" | "ready_to_install" | "installing" | "error";

export type UpdateIssue = "publication_not_configured" | "unsupported_platform" | "network" | "invalid_release" | "download_failed" | "signature_invalid" | "installer_unavailable" | "install_failed";

export type UpdateRelease = { version: string, notes: string | null, published_at_ms: EpochMs | null, };

export type UpdateSnapshot = { update_revision: DecimalInt, phase: UpdatePhase, current_version: string, release: UpdateRelease | null, last_checked_at_ms: EpochMs | null, downloaded_bytes: DecimalInt | null, total_bytes: DecimalInt | null, issue: UpdateIssue | null, };

export type UpdateActionRequest = { expected_update_revision: DecimalInt, };

export type NotifyIssue = "unavailable" | "transaction_unavailable" | "busy" | "permission_denied" | "unsafe_path" | "unsafe_file" | "unsafe_permissions" | "invalid_config" | "invalid_notify" | "already_managed" | "config_changed" | "ownership_changed" | "invalid_registration" | "invalid_marker" | "limit_reached" | "already_exists" | "not_found" | "no_original_command" | "plan_not_found" | "plan_expired" | "plan_limit" | "active_configuration" | "cleanup_failed" | "wrong_executable" | "channel_unavailable" | "worker_unavailable";

export type NotifyConfigOperation = "enable" | "disable";

export type NotifyPrepareAction = { "kind": "enable_source", source_id: string, chain_original: boolean | null, } | { "kind": "choose_home", chain_original: boolean | null, } | { "kind": "disable", registration_id: string, };

export type NotifyIntegrationRow = { registration_id: string, home_path: string | null, configured: boolean | null, current_executable: boolean | null, chain_original: boolean | null, issue: NotifyIssue | null, };

export type NotifyIntegrationsSnapshot = { ready: boolean, listener_count: number | null, service_issue: NotifyIssue | null,
/**
 * null means enumeration failed, distinct from no registrations.
 */
registrations: Array<NotifyIntegrationRow> | null, registry_issue: NotifyIssue | null, redacted: boolean, };

export type NotifyConfigPreview = { plan_id: string, registration_id: string, operation: NotifyConfigOperation, home_path: string | null, before_notify: string | null, after_notify: string | null, creates_config: boolean, can_chain_original: boolean, chain_original: boolean, settings_revision: DecimalInt, expires_in_seconds: number, redacted: boolean, };

export type NotifyApplyResult = { registration_id: string, configured: boolean | null, retired: boolean, cleanup_issue: NotifyIssue | null, };

export type DisplayPolicyStamp = { settings_revision: DecimalInt, privacy: boolean, };

export type DisplayPreferences = {
/**
 * None means not initialized, never an implicit UTC/system fallback.
 */
display_timezone: string | null, privacy: boolean, theme: AppTheme, };

export type DisplaySettingsSnapshot = { settings_version: number, settings_revision: DecimalInt, preferences: DisplayPreferences, };

export type TaskbarDisplayLayout = "two_rows" | "single_row";

export type TaskbarDisplayPreferences = { layout: TaskbarDisplayLayout, show_tokens: boolean, show_costs: boolean, show_quota: boolean, show_weekly_reset: boolean, };

export type TaskbarPosition = "notification_left" | "application_right";

export type TaskbarPreferences = { enabled: boolean, display: TaskbarDisplayPreferences, position: TaskbarPosition, fallback_to_mini: boolean, };

export type TaskbarPreferencesSnapshot = { preferences: TaskbarPreferences, settings_revision: DecimalInt, };

export type TaskbarPreferencesMutation = { preferences: TaskbarPreferences, expected_settings_revision: DecimalInt, };

export type TaskbarRuntimeState = "disabled" | "probing" | "waiting_snapshot" | "embedded" | "unavailable" | "recovering" | "suspended";

export type TaskbarRuntimeIssue = "unsupported_version" | "missing_taskbar" | "unexpected_structure" | "unsafe_geometry" | "insufficient_space" | "background_unavailable" | "host_unavailable" | "host_timeout" | "protocol_error" | "unsupported_position" | "input_unavailable" | "cleanup_uncertain" | "cleanup_failed" | "external_layout_change";

export type TaskbarRuntimeSnapshot = { revision: DecimalInt, state: TaskbarRuntimeState, applied_settings_revision: DecimalInt | null, issue: TaskbarRuntimeIssue | null, error: ErrorCode | null, compact: boolean | null, fallback_visible: boolean | null, fallback_error: ErrorCode | null, action_error: ErrorCode | null, last_cleanup: TaskbarCleanupOutcome | null, last_snapshot_at_ms: EpochMs | null, };

export type TaskbarCleanupOutcome = "no_record" | "restored" | "already_restored" | "external_change" | "identity_lost" | "failed" | "uncertain" | "timeout" | "unavailable";

export type TimezoneMutation = { "kind": "initialize", system_timezone: string, } | { "kind": "set", display_timezone: string, expected_settings_revision: DecimalInt, };

export type DisplayPrivacyMutation = { privacy: boolean, expected_settings_revision: DecimalInt, };

export type AppTheme = "dark" | "light" | "system";

export type DisplayThemeMutation = { theme: AppTheme, expected_settings_revision: DecimalInt, };

export type SettingsChanged = { settings_revision: DecimalInt, };

export type MiniScopeMutation = { mini_scope: MiniScope, expected_settings_revision: DecimalInt, };

export type MiniScopeSnapshot = { settings_revision: DecimalInt, mini_scope: MiniScope, };

export type MiniUsageSnapshot = { meta: SnapshotMeta, settings_revision: DecimalInt, mini_scope: MiniScope, scope_display_name: string | null, range: DateRange, usage: TokenTotals, pricing: PricingSummary, coverage: Coverage, };

export type MiniWindowState = { expanded: boolean, pinned: boolean, };

export type MiniWindowAction = { "kind": "read", } | { "kind": "set_expanded", expanded: boolean, } | { "kind": "set_pinned", pinned: boolean, } | { "kind": "drag", } | { "kind": "hide", };

export type MiniOpacitySnapshot = { opacity_percent: number, supported: boolean, settings_revision: DecimalInt, };

export type MiniOpacityMutation = { opacity_percent: number, expected_settings_revision: DecimalInt, };

export type MiniPassthroughSnapshot = { enabled: boolean, persisted_enabled: boolean, window_present: boolean, supported: boolean, recovery_shortcut: RecoveryShortcut, recovery_registration: ShortcutRegistration, settings_revision: DecimalInt, };

export type MiniPassthroughMutation = { enabled: boolean, acknowledged_recovery: RecoveryShortcut | null, expected_settings_revision: DecimalInt, };

export type RecoveryShortcut = { control: boolean, alt: boolean, shift: boolean, key: string, };

export type ShortcutRegistration = "ready" | "conflict" | "unsupported" | "unavailable";

export type RecoveryShortcutSnapshot = { shortcut: RecoveryShortcut, registration: ShortcutRegistration, settings_revision: DecimalInt, };

export type RecoveryShortcutMutation = { shortcut: RecoveryShortcut, expected_settings_revision: DecimalInt, };

export type MiniStatsRequest = { request_id: string, mini_scope: MiniScope, calendar: CalendarSelectionResult, };

export type MiniStatsOpenRequest = { expected_settings_revision: DecimalInt, };

export type MainNavigationIntent = { "kind": "mini_stats", request: MiniStatsRequest, } | { "kind": "taskbar_settings", };

export type MainNavigationSnapshot = { revision: DecimalInt, intent: MainNavigationIntent | null, };

export type MiniSessionsQuery = { search: string, page_size: number, };

export type MiniSessionsRequest = { query: MiniSessionsQuery, cursor: string | null, };

export type MiniSessionOption = { session_key: string, display_name: string, };

export type MiniSessionsPage = { meta: SnapshotMeta, options: Array<MiniSessionOption>, next_cursor: string | null, };

export type ErrorCode = "INVALID_QUERY" | "UNSUPPORTED_API" | "SOURCE_UNREADABLE" | "UNSUPPORTED_FORMAT" | "UNSUPPORTED_SETTINGS_VERSION" | "AMBIGUOUS_USAGE" | "CHECKPOINT_CONFLICT" | "CANDIDATE_OBSOLETE" | "DB_WRITE_FAILED" | "DISK_FULL" | "DB_CORRUPT" | "MIGRATION_FAILED" | "SNAPSHOT_EXPIRED" | "CURSOR_INVALID" | "REVISION_CONFLICT" | "STALE_CONFIRMATION" | "REQUEST_KEY_CONFLICT" | "PRICE_RULE_CONFLICT" | "JOB_CANCELLED" | "JOB_INTERRUPTED" | "QUOTA_DISCONNECTED" | "QUOTA_UNSUPPORTED" | "QUOTA_TIMEOUT" | "QUOTA_AUTH_REQUIRED" | "QUOTA_PROTOCOL_ERROR" | "QUOTA_SERVICE_UNAVAILABLE" | "TASKBAR_UNSUPPORTED" | "TASKBAR_NO_SPACE" | "TASKBAR_EMBED_FAILED" | "NUMERIC_OVERFLOW" | "PERMISSION_DENIED" | "INVALID_USAGE" | "WINDOW_UNAVAILABLE" | "SHORTCUT_CONFLICT" | "SHORTCUT_UNAVAILABLE" | "NOTIFY_INTEGRATION_FAILED" | "UPDATE_UNAVAILABLE" | "UPDATE_BUSY" | "UPDATE_FAILED";

export type ErrorDetail = string | number | boolean | null;

export type AppError = { code: ErrorCode, message_key: string, retryable: boolean, correlation_id: string, source_id: string | null, job_id: string | null, details: { [key in string]: ErrorDetail }, };

export type AppStatus = { version: string, development: boolean, data_directory: string, collector: ServiceState, storage: ServiceState, storage_error: ErrorCode | null, quota: ServiceState, taskbar: ServiceState, };

export type SnapshotMeta = { snapshot_id: string, data_revision: DecimalInt, price_revision: DecimalInt, generated_at_ms: EpochMs, parser_versions: Array<string>, accounting_versions: Array<string>, display_timezone: string, };

export type DateRange = { start_ms: EpochMs, end_ms: EpochMs, timezone: string, };

export type Grain = "hour" | "day" | "month";

export type CalendarBucket = { start_ms: EpochMs, end_ms: EpochMs, display_label: string, utc_offset: string, };

export type CalendarSelection = { "kind": "today", } | { "kind": "last7", } | { "kind": "last30", } | { "kind": "custom", start_date: string, end_date_inclusive: string, };

export type CalendarSelectionRequest = { timezone: string, selection: CalendarSelection, };

export type CalendarSelectionResult = { range: DateRange, heatmap_range: DateRange, local_today: string, };

export type DimensionSelection = { "kind": "all", } | { "kind": "ids", ids: Array<string>, include_unknown: boolean, };

export type UsageFilter = { range: DateRange, sources: DimensionSelection, models: DimensionSelection, projects: DimensionSelection, sessions: DimensionSelection, };

export type PriceBasis = { "mode": "event_time", } | { "mode": "specified_time", specified_at_ms: EpochMs, };

export type TokenMeasure = { value: DecimalInt | null, covered_total_tokens: DecimalInt, complete: boolean, };

export type TokenTotals = { total_tokens: DecimalInt, input_total: TokenMeasure, cached_input: TokenMeasure, noncached_input: TokenMeasure, output_total: TokenMeasure, reasoning_output: TokenMeasure,
/**
 * Included in input_total and noncached_input, never an extra total term.
 */
cache_write_input: TokenMeasure, session_count: DecimalInt, usage_event_count: DecimalInt, reliable_turn_count: DecimalInt | null, reliable_turns_complete: boolean, };

export type GroupDimension = "models" | "projects";

export type GroupSort = "total_desc" | "name_asc";

export type GroupedUsage = { key: string | null, display_name: string, totals: TokenTotals, };

export type GroupedUsageRequest = { filter: UsageFilter, price_basis: PriceBasis, dimension: GroupDimension, sort: GroupSort, limit: number, };

export type PricedUsageGroup = { key: string | null, display_name: string, totals: TokenTotals, pricing: PricingSummary, coverage: Coverage, };

export type GroupedUsageBundle = { meta: SnapshotMeta, summary: TokenTotals, pricing: PricingSummary, coverage: Coverage, total_group_count: DecimalInt, truncated: boolean, groups: Array<PricedUsageGroup>, };

export type FacetDimension = "sources" | "models" | "projects" | "sessions";

export type FilterOptionsQuery = { filter: UsageFilter, dimension: FacetDimension, search: string, page_size: number, };

export type FilterOptionsRequest = { query: FilterOptionsQuery, cursor: string | null, };

export type FilterOption = { key: string | null, display_name: string,
/**
 * Confirmed selected usage events, not an import-completeness assertion.
 */
count: DecimalInt, };

export type FilterOptionsPage = { meta: SnapshotMeta, dimension: FacetDimension, options: Array<FilterOption>, next_cursor: string | null, };

export type SessionRow = { session_key: string, display_name: string,
/**
 * Latest selected usage event, not lifetime activity or latest context time.
 */
latest_at_ms: EpochMs, latest_model: string | null, latest_project_id: string | null, latest_project_name: string | null, parent_key: string | null, parent_display_name: string | null, parent_provider_id: string | null,
/**
 * Registered resolved children across dates, not inferred from request counts.
 */
child_count: DecimalInt, summary: TokenTotals, pricing: PricingSummary, coverage: Coverage, latest_context: ContextSnapshot, };

export type SessionSort = "latest_desc" | "total_desc";

export type SessionsQuery = { filter: UsageFilter, price_basis: PriceBasis, sort: SessionSort, page_size: number, };

export type SessionsRequest = { query: SessionsQuery, cursor: string | null, };

export type SessionsPage = { meta: SnapshotMeta,
/**
 * Whole filter totals, independent of the current page.
 */
summary: TokenTotals, pricing: PricingSummary, coverage: Coverage, sessions: Array<SessionRow>, next_cursor: string | null, };

export type TurnsQuery = { session_key: string, filter: UsageFilter, price_basis: PriceBasis, page_size: number, };

export type TurnsRequest = { query: TurnsQuery, cursor: string | null, };

export type TurnsPage = { meta: SnapshotMeta, session_key: string,
/**
 * All selected session consumption, including events without turn identity.
 */
summary: TokenTotals, pricing: PricingSummary, coverage: Coverage, unidentified_usage_event_count: DecimalInt, turns: Array<TurnRow>, next_cursor: string | null, };

export type TurnRow = { turn_id: string, first_at_ms: EpochMs, last_at_ms: EpochMs,
/**
 * Only selected events in this turn, not its lifetime consumption.
 */
summary: TokenTotals, pricing: PricingSummary, };

export type SessionBundleRequest = { session_key: string, filter: UsageFilter, price_basis: PriceBasis, };

export type SessionBundle = { meta: SnapshotMeta, identity: SessionIdentity, summary: TokenTotals, pricing: PricingSummary, coverage: Coverage, latest_selected_activity: SessionActivity | null, latest_context: ContextSnapshot, child_count: DecimalInt, children: Array<SessionIdentity>, children_truncated: boolean, classifications: Array<SessionClassification>, };

export type SessionIdentity = { session_key: string, display_name: string, parent_key: string | null, parent_display_name: string | null, parent_provider_id: string | null, };

export type SessionActivity = { occurred_at_ms: EpochMs, model: string | null, project_id: string | null, project_display_name: string | null, };

export type SessionClassification = { kind: ClassificationKind, reason_code: string,
/**
 * Observation classifications across the active ledger, never consumption.
 */
observation_count: DecimalInt, };

export type ClassificationKind = "pending" | "inherited" | "duplicate" | "unattributed";

export type RawTokenCount = string;

export type RawUsageVector = { input_total: RawTokenCount | null, cached_input: RawTokenCount | null, cache_write_input: RawTokenCount | null, output_total: RawTokenCount | null, reasoning_output: RawTokenCount | null, reported_total: RawTokenCount | null, };

export type UsageEventSort = "time_desc" | "total_desc";

export type UsageEventsQuery = { filter: UsageFilter, price_basis: PriceBasis, sort: UsageEventSort, page_size: number, };

export type UsageEventsRequest = { query: UsageEventsQuery, cursor: string | null, };

export type UsageEventRow = { event_id: string, session_key: string, session_display_name: string, occurred_at_ms: EpochMs, model: string | null, provider: string | null, project_id: string | null, project_display_name: string | null, source_ids: Array<string>, turn_id: string | null, total_tokens: DecimalInt,
/**
 * Published increment; raw_last/cumulative retain original source vectors.
 */
usage: RawUsageVector, raw_last: RawUsageVector | null, raw_cumulative: RawUsageVector | null, calculation_method: string, quality_flags: Array<string>, price: PriceOutcome, parser_version: string, accounting_version: string, };

export type UsageEventsPage = { meta: SnapshotMeta, summary: TokenTotals, pricing: PricingSummary, coverage: Coverage, events: Array<UsageEventRow>, next_cursor: string | null, };

export type CloseQuerySnapshotRequest = { "kind": "mini_sessions", request: MiniSessionsRequest, } | { "kind": "filter_options", request: FilterOptionsRequest, } | { "kind": "sessions", request: SessionsRequest, } | { "kind": "usage_events", request: UsageEventsRequest, } | { "kind": "turns", request: TurnsRequest, };

export type DashboardRequest = { filter: UsageFilter, price_basis: PriceBasis, grain: Grain, heatmap_range: DateRange, };

export type UsageSeriesBucket = { start_ms: EpochMs, end_ms: EpochMs, display_label: string, utc_offset: string, totals: TokenTotals, coverage: Coverage, };

export type RecentSession = { session_key: string, display_name: string, latest_at_ms: EpochMs, latest_model: string | null, latest_project_id: string | null, latest_project_name: string | null, summary: TokenTotals, pricing: PricingSummary, };

export type DashboardBundle = { meta: SnapshotMeta, summary: TokenTotals, pricing: PricingSummary, coverage: Coverage, series: Array<UsageSeriesBucket>, heatmap: Array<UsageSeriesBucket>, recent_sessions: Array<RecentSession>, };

export type CurrencyEstimate = { currency: string, estimated_cost: DecimalMoney | null, priced_total_tokens: DecimalInt, };

export type UnpricedReason = { code: string, total_tokens: DecimalInt, event_count: DecimalInt, };

export type PricingSummary = { redacted: boolean, basis: PriceBasis, currencies: Array<CurrencyEstimate>, priced_total_tokens: DecimalInt, unpriced_total_tokens: DecimalInt, reasons: Array<UnpricedReason>, calculating: boolean, };

export type PriceOrigin = "custom" | "offline";

export type OfflinePriceTier = "standard" | "batch" | "flex" | "fast" | "ultrafast";

export type OfflineContextBand = "all" | "short" | "long";

export type OfflineReferenceBasis = "global_api_reference";

export type OfflinePriceEntry = { model_exact: string, tier: OfflinePriceTier, context: OfflineContextBand,
/**
 * Currency units per million tokens, exact decimal strings (never f64).
 */
input_per_million: string, cached_per_million: string | null, cache_write_per_million: string | null, output_per_million: string, reference: string, };

export type OfflinePriceCatalog = { format_version: number, catalog_id: string, verified_at_ms: EpochMs, provider: string, currency: string, short_context_max_input: number, reference_basis: OfflineReferenceBasis, entries: Array<OfflinePriceEntry>, };

export type OfflinePriceCatalogSnapshot = { price_revision: DecimalInt, catalog: OfflinePriceCatalog | null, };

export type PriceRevalueRequest = { scope: JobScope, basis: PriceBasis, expected_price_revision: DecimalInt, request_key: string, };

export type PriceRevalueState = "queued" | "running" | "cancelling" | "succeeded" | "cancelled" | "failed" | "interrupted";

export type PriceRevalueJob = { job_id: string, state: PriceRevalueState, automatic: boolean, price_revision: DecimalInt, basis: PriceBasis, total_ledgers: DecimalInt, completed_ledgers: DecimalInt, total_events: DecimalInt, processed_events: DecimalInt, can_cancel: boolean, error: ErrorCode | null, created_at_ms: EpochMs, updated_at_ms: EpochMs, };

export type PriceRevalueStatus = { current_price_revision: DecimalInt, active_job: PriceRevalueJob | null, latest_job: PriceRevalueJob | null, uncached_ledgers: DecimalInt, };

export type PriceRule = { rule_id: string, introduced_revision: DecimalInt, retired_revision: DecimalInt | null, provider: string, model_exact: string, source_id: string | null, currency: string, effective_from_ms: EpochMs, effective_to_ms: EpochMs | null, priority: number, input_rate_atoms: DecimalInt, cached_rate_atoms: DecimalInt | null, cache_write_rate_atoms: DecimalInt | null, output_rate_atoms: DecimalInt, origin: PriceOrigin, origin_reference: string | null, created_at_ms: EpochMs, };

export type PriceRuleDraft = { provider: string, model_exact: string, source_id: string | null, currency: string, effective_from_ms: EpochMs, effective_to_ms: EpochMs | null, priority: number, input_rate_atoms: DecimalInt, cached_rate_atoms: DecimalInt | null, cache_write_rate_atoms: DecimalInt | null, output_rate_atoms: DecimalInt, origin_reference: string | null, };

export type ModelAliasDraft = { provider: string, alias: string, canonical_model: string, };

export type ModelAliasMutation = { "kind": "create", draft: ModelAliasDraft, } | { "kind": "replace", alias_id: string, draft: ModelAliasDraft, } | { "kind": "retire", alias_id: string, };

export type PriceRuleMutation = { "kind": "create", draft: PriceRuleDraft, } | { "kind": "replace", rule_id: string, draft: PriceRuleDraft, } | { "kind": "retire", rule_id: string, };

export type PriceRulesSnapshot = { price_revision: DecimalInt, rules: Array<PriceRule>, aliases: Array<ModelAlias>, };

export type PriceChanged = { price_revision: DecimalInt, all_models: boolean, };

export type ModelAlias = { alias_id: string, provider: string, alias: string, canonical_model: string, introduced_revision: DecimalInt, retired_revision: DecimalInt | null, };

export type UnpricedCode = "unknown_model" | "missing_rule" | "ambiguous_rule" | "insufficient_usage" | "overflow";

export type PriceOutcome = { "status": "redacted", } | { "status": "priced", rule_id: string, currency: string, cost_atoms: DecimalInt, estimated_cost: DecimalMoney, } | { "status": "unpriced", reason: UnpricedCode, };

export type CoverageState = "complete" | "partial" | "unknown";

export type SourceIssue = { source_id: string, code: string, last_success_ms: EpochMs | null, };

export type FormatIssue = { format: string, count: DecimalInt, };

export type Coverage = { state: CoverageState, pending_observation_count: DecimalInt, unattributed_observation_count: DecimalInt, unattributed_total_tokens: DecimalInt | null, pending_file_count: DecimalInt, source_issues: Array<SourceIssue>, format_issues: Array<FormatIssue>, breakdown_complete: boolean, };

export type ContextSnapshot = { context_tokens: DecimalInt | null, model_context_window: DecimalInt | null, percentage: number | null, observed_at_ms: EpochMs | null, quality: string, };

export type ScopeStart = { "kind": "today", } | { "kind": "fixed", start_ms: EpochMs, };

export type MiniScope = { "kind": "today_all_sources", } | { "kind": "session", session_key: string, start: ScopeStart, };

export type QuotaState = "disconnected" | "connecting" | "authorization_required" | "unsupported" | "ready" | "stale" | "error";

export type QuotaWindow = { window_id: string, duration_mins: number | null, used_percent: number | null, remaining_percent: number | null, resets_at_ms: EpochMs | null, };

export type QuotaLimit = { limit_id: string, display_name: string | null, };

export type QuotaSnapshot = { connection_epoch: string, quota_revision: DecimalInt, state: QuotaState, selected_limit_id: string | null, available_limits: Array<QuotaLimit>, fetched_at_ms: EpochMs | null, last_attempt_at_ms: EpochMs | null, windows: Array<QuotaWindow>, error_code: string | null, };

export type QuotaRefreshStatus = "started" | "in_flight" | "rate_limited" | "not_due";

export type QuotaRefreshResult = { status: QuotaRefreshStatus, retry_after_ms: number | null, quota: QuotaSnapshot, };

export type QuotaChanged = { connection_epoch: string, quota_revision: DecimalInt, state: QuotaState, };

export type AccountServiceConfigSnapshot = { settings_revision: DecimalInt, executable_display_path: string | null, home_display_path: string | null, executable_sha256: string | null, configured: boolean, auto_connect: boolean, };

export type AccountServiceSelectionKind = "detect_local" | "current" | "executable" | "home" | "default_home";

export type AccountServiceSelectionRequest = { kind: AccountServiceSelectionKind, base_selection_handle: string | null, expected_settings_revision: DecimalInt, };

export type AccountServiceSelection = { selection_handle: string, preview: AccountServiceConfigSnapshot, expires_at_ms: EpochMs, };

export type AccountServiceConfigMutation = { selection_handle: string, auto_connect: boolean, expected_settings_revision: DecimalInt, };

export type AccountConnectionRequest = { "kind": "connect", expected_settings_revision: DecimalInt, expected_connection_epoch: string, acknowledged_executable_sha256: string, } | { "kind": "disconnect", expected_connection_epoch: string, } | { "kind": "select_limit", expected_connection_epoch: string, expected_quota_revision: DecimalInt, limit_id: string, };

export type MiniSnapshot = { usage_meta: SnapshotMeta, mini_scope: MiniScope, scope_display_name: string | null, usage: TokenTotals, pricing: PricingSummary, coverage: Coverage, quota: QuotaSnapshot, privacy: boolean, usage_last_success_ms: EpochMs | null, };

export type JobKind = "import" | "reconcile" | "rebuild" | "export" | "backup" | "restore" | "price_revalue" | "clear";

export type JobState = "queued" | "running" | "validating" | "publishing" | "cancelling" | "succeeded" | "cancelled" | "failed" | "interrupted";

export type Job = { job_id: string, kind: JobKind, state: JobState, phase: string, discovered_files: DecimalInt, discovery_complete: boolean, processed_files: DecimalInt, processed_bytes: DecimalInt, accepted_events: DecimalInt, pending_observations: DecimalInt, can_cancel: boolean, error: AppError | null, created_at_ms: EpochMs, updated_at_ms: EpochMs, };

export type DiagnosticsRequest = { source_id: string | null, };

export type DiagnosticKind = "log_record" | "unconfirmed_usage" | "unattributed_usage" | "missing_file" | "directory_scan";

export type DiagnosticIssue = { issue_id: string, source_id: string | null, kind: DiagnosticKind, code: ErrorCode | null, path: string | null, byte_offset: DecimalInt | null, };

export type DiagnosticsSnapshot = { data_revision: DecimalInt, issues: Array<DiagnosticIssue>, has_more: boolean, };

export type JobScope = { "kind": "all", } | { "kind": "sources", source_ids: Array<string>, } | { "kind": "sessions", session_keys: Array<string>, };

export type JobRequest = { kind: JobKind, scope: JobScope, request_key: string, };

export type CancelJobResult = "accepted" | "already_finished" | "too_late";

export type WindowAction = "open_stats" | "show_mini" | "hide_main" | "quit";

export type SourceOrigin = "windows_default" | "environment" | "custom" | "wsl";

export type SourceReadability = "awaiting_directory" | "readable" | "partially_readable" | "unreadable" | "disabled";

export type CapabilityState = "not_probed" | "available" | "unavailable";

export type SourceCapabilities = { physical_identity: CapabilityState, byte_seek: CapabilityState, watcher: CapabilityState, polling_required: boolean, };

export type SourceSummary = { source_id: string, root_path: string, origin: SourceOrigin, enabled: boolean, removed: boolean, readability: SourceReadability, capabilities: SourceCapabilities, last_scan_at_ms: EpochMs | null, last_success_at_ms: EpochMs | null, error: ErrorCode | null, };

export type SourceDirectoryKind = "local" | "wsl";

export type SourceDirectorySelection = { selection_handle: string, root_path: string, origin: SourceOrigin, };

export type SourcesSnapshot = { settings_revision: DecimalInt, sources: Array<SourceSummary>, };

export type ManageSourceAction = { "kind": "add", selection_handle: string, } | { "kind": "pause", source_id: string, } | { "kind": "resume", source_id: string, } | { "kind": "detect", } | { "kind": "retain_remove", source_id: string, };

export type Response<T> = { api_version: 1, request_id: string, data: T, display_policy?: DisplayPolicyStamp, };
