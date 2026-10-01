// Generated from token-pulse-core Rust DTOs. Run npm run contracts; do not edit.

export type DecimalInt = string;

export type DecimalMoney = string;

export type EpochMs = number;

export type ServiceState = "not_configured" | "not_implemented" | "ready" | "error";

export type ErrorCode = "INVALID_QUERY" | "UNSUPPORTED_API" | "SOURCE_UNREADABLE" | "UNSUPPORTED_FORMAT" | "AMBIGUOUS_USAGE" | "CHECKPOINT_CONFLICT" | "DB_WRITE_FAILED" | "DISK_FULL" | "DB_CORRUPT" | "MIGRATION_FAILED" | "SNAPSHOT_EXPIRED" | "CURSOR_INVALID" | "REVISION_CONFLICT" | "STALE_CONFIRMATION" | "REQUEST_KEY_CONFLICT" | "JOB_CANCELLED" | "JOB_INTERRUPTED" | "QUOTA_DISCONNECTED" | "QUOTA_UNSUPPORTED" | "QUOTA_TIMEOUT" | "QUOTA_AUTH_REQUIRED" | "TASKBAR_UNSUPPORTED" | "TASKBAR_NO_SPACE" | "TASKBAR_EMBED_FAILED" | "NUMERIC_OVERFLOW" | "PERMISSION_DENIED" | "INVALID_USAGE" | "WINDOW_UNAVAILABLE";

export type ErrorDetail = string | number | boolean | null;

export type AppError = { code: ErrorCode, message_key: string, retryable: boolean, correlation_id: string, source_id: string | null, job_id: string | null, details: { [key in string]: ErrorDetail }, };

export type AppStatus = { version: string, development: boolean, data_directory: string, collector: ServiceState, storage: ServiceState, storage_error: ErrorCode | null, quota: ServiceState, taskbar: ServiceState, };

export type SnapshotMeta = { snapshot_id: string, data_revision: DecimalInt, price_revision: DecimalInt, generated_at_ms: EpochMs, parser_versions: Array<string>, accounting_versions: Array<string>, display_timezone: string, };

export type DateRange = { start_ms: EpochMs, end_ms: EpochMs, timezone: string, };

export type DimensionSelection = { "kind": "all", } | { "kind": "ids", ids: Array<string>, include_unknown: boolean, };

export type UsageFilter = { range: DateRange, sources: DimensionSelection, models: DimensionSelection, projects: DimensionSelection, sessions: DimensionSelection, };

export type PriceBasis = { "mode": "event_time", } | { "mode": "specified_time", specified_at_ms: EpochMs, };

export type TokenMeasure = { value: DecimalInt | null, covered_total_tokens: DecimalInt, complete: boolean, };

export type TokenTotals = { total_tokens: DecimalInt, input_total: TokenMeasure, cached_input: TokenMeasure, noncached_input: TokenMeasure, output_total: TokenMeasure, reasoning_output: TokenMeasure, session_count: DecimalInt, usage_event_count: DecimalInt, reliable_turn_count: DecimalInt | null, reliable_turns_complete: boolean, };

export type CurrencyEstimate = { currency: string, estimated_cost: DecimalMoney | null, priced_total_tokens: DecimalInt, };

export type UnpricedReason = { code: string, total_tokens: DecimalInt, event_count: DecimalInt, };

export type PricingSummary = { redacted: boolean, basis: PriceBasis, currencies: Array<CurrencyEstimate>, priced_total_tokens: DecimalInt, unpriced_total_tokens: DecimalInt, reasons: Array<UnpricedReason>, calculating: boolean, };

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

export type MiniSnapshot = { usage_meta: SnapshotMeta, mini_scope: MiniScope, scope_display_name: string | null, usage: TokenTotals, pricing: PricingSummary, coverage: Coverage, quota: QuotaSnapshot, privacy: boolean, usage_last_success_ms: EpochMs | null, };

export type JobKind = "import" | "reconcile" | "rebuild" | "export" | "backup" | "restore" | "price_revalue" | "clear";

export type JobState = "queued" | "running" | "validating" | "publishing" | "cancelling" | "succeeded" | "cancelled" | "failed" | "interrupted";

export type Job = { job_id: string, kind: JobKind, state: JobState, phase: string, discovered_files: DecimalInt, discovery_complete: boolean, processed_files: DecimalInt, processed_bytes: DecimalInt, accepted_events: DecimalInt, pending_observations: DecimalInt, can_cancel: boolean, error: AppError | null, created_at_ms: EpochMs, updated_at_ms: EpochMs, };

export type WindowAction = "open_stats" | "hide_main" | "quit";

export type SourceOrigin = "windows_default" | "environment" | "custom" | "wsl";

export type SourceReadability = "awaiting_directory" | "readable" | "partially_readable" | "unreadable" | "disabled";

export type CapabilityState = "not_probed" | "available" | "unavailable";

export type SourceCapabilities = { physical_identity: CapabilityState, byte_seek: CapabilityState, watcher: CapabilityState, polling_required: boolean, };

export type SourceSummary = { source_id: string, root_path: string, origin: SourceOrigin, enabled: boolean, readability: SourceReadability, capabilities: SourceCapabilities, last_scan_at_ms: EpochMs | null, last_success_at_ms: EpochMs | null, error: ErrorCode | null, };

export type Response<T> = { api_version: 1, request_id: string, data: T, };
