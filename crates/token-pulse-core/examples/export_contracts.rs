use schemars::{JsonSchema, generate::SchemaSettings};
use std::{collections::BTreeMap, fs, path::PathBuf};
use token_pulse_core::calendar::{
    CalendarBucket, CalendarSelection, CalendarSelectionRequest, CalendarSelectionResult, Grain,
};
use token_pulse_core::diagnostics::*;
use token_pulse_core::jobs::*;
use token_pulse_core::mini::*;
use token_pulse_core::pricing::offline::*;
use token_pulse_core::pricing::revalue::*;
use token_pulse_core::pricing::{
    ModelAlias, ModelAliasDraft, ModelAliasMutation, PriceChanged, PriceOrigin, PriceOutcome,
    PriceRule, PriceRuleDraft, PriceRuleMutation, PriceRulesSnapshot, UnpricedCode,
};
use token_pulse_core::query::{
    ClassificationKind, RawTokenCount, RawUsageVector, SessionActivity, SessionBundle,
    SessionBundleRequest, SessionClassification, SessionIdentity, UsageEventRow, UsageEventSort,
    UsageEventsPage, UsageEventsQuery, UsageEventsRequest,
};
use token_pulse_core::query::{
    CloseQuerySnapshotRequest, DashboardBundle, DashboardRequest, FacetDimension, FilterOption,
    FilterOptionsPage, FilterOptionsQuery, FilterOptionsRequest, GroupDimension, GroupSort,
    GroupedUsage, GroupedUsageBundle, GroupedUsageRequest, PricedUsageGroup, RecentSession,
    SessionRow, SessionSort, SessionsPage, SessionsQuery, SessionsRequest, UsageSeriesBucket,
};
use token_pulse_core::query::{TurnRow, TurnsPage, TurnsQuery, TurnsRequest};
use token_pulse_core::settings::*;
use token_pulse_core::sources::*;
use token_pulse_core::taskbar::*;
use token_pulse_core::{ServiceState, error::*, numeric::*, protocol::*};
use ts_rs::{Config, TS};

fn add<T: TS + JsonSchema>(ts: &mut String, schemas: &mut BTreeMap<String, serde_json::Value>) {
    let config = Config::default();
    ts.push_str("export ");
    ts.push_str(&T::decl(&config));
    ts.push_str("\n\n");
    let schema = SchemaSettings::default()
        .for_serialize()
        .into_generator()
        .into_root_schema_for::<T>();
    schemas.insert(
        T::name(&config),
        serde_json::to_value(schema).expect("schema JSON"),
    );
}
fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let check = std::env::args().any(|arg| arg == "--check");
    let mut ts = String::from(
        "// Generated from token-pulse-core Rust DTOs. Run npm run contracts; do not edit.\n\n",
    );
    let mut schemas = BTreeMap::new();
    macro_rules! types { ($($ty:ty),* $(,)?) => { $(add::<$ty>(&mut ts, &mut schemas);)* }; }
    types!(
        DecimalInt,
        DecimalMoney,
        EpochMs,
        ServiceState,
        token_pulse_core::updates::UpdatePhase,
        token_pulse_core::updates::UpdateIssue,
        token_pulse_core::updates::UpdateRelease,
        token_pulse_core::updates::UpdateSnapshot,
        token_pulse_core::updates::UpdateActionRequest,
        token_pulse_core::notify_integration::NotifyIssue,
        token_pulse_core::notify_integration::NotifyConfigOperation,
        token_pulse_core::notify_integration::NotifyPrepareAction,
        token_pulse_core::notify_integration::NotifyIntegrationRow,
        token_pulse_core::notify_integration::NotifyIntegrationsSnapshot,
        token_pulse_core::notify_integration::NotifyConfigPreview,
        token_pulse_core::notify_integration::NotifyApplyResult,
        token_pulse_core::privacy::DisplayPolicyStamp,
        DisplayPreferences,
        DisplaySettingsSnapshot,
        TaskbarDisplayLayout,
        TaskbarDisplayPreferences,
        TaskbarPosition,
        TaskbarPreferences,
        TaskbarPreferencesSnapshot,
        TaskbarPreferencesMutation,
        TaskbarRuntimeState,
        TaskbarRuntimeIssue,
        TaskbarRuntimeSnapshot,
        TaskbarCleanupOutcome,
        TimezoneMutation,
        DisplayPrivacyMutation,
        AppTheme,
        DisplayThemeMutation,
        SettingsChanged,
        MiniScopeMutation,
        MiniScopeSnapshot,
        MiniUsageSnapshot,
        MiniWindowState,
        MiniWindowAction,
        token_pulse_core::mini_opacity::MiniOpacitySnapshot,
        token_pulse_core::mini_opacity::MiniOpacityMutation,
        token_pulse_core::mini_passthrough::MiniPassthroughSnapshot,
        token_pulse_core::mini_passthrough::MiniPassthroughMutation,
        token_pulse_core::shortcuts::RecoveryShortcut,
        token_pulse_core::shortcuts::ShortcutRegistration,
        token_pulse_core::shortcuts::RecoveryShortcutSnapshot,
        token_pulse_core::shortcuts::RecoveryShortcutMutation,
        MiniStatsRequest,
        MiniStatsOpenRequest,
        token_pulse_core::navigation::MainNavigationIntent,
        token_pulse_core::navigation::MainNavigationSnapshot,
        MiniSessionsQuery,
        MiniSessionsRequest,
        MiniSessionOption,
        MiniSessionsPage,
        ErrorCode,
        ErrorDetail,
        AppError,
        AppStatus,
        SnapshotMeta,
        DateRange,
        Grain,
        CalendarBucket,
        CalendarSelection,
        CalendarSelectionRequest,
        CalendarSelectionResult,
        DimensionSelection,
        UsageFilter,
        PriceBasis,
        TokenMeasure,
        TokenTotals,
        GroupDimension,
        GroupSort,
        GroupedUsage,
        GroupedUsageRequest,
        PricedUsageGroup,
        GroupedUsageBundle,
        FacetDimension,
        FilterOptionsQuery,
        FilterOptionsRequest,
        FilterOption,
        FilterOptionsPage,
        SessionRow,
        SessionSort,
        SessionsQuery,
        SessionsRequest,
        SessionsPage,
        TurnsQuery,
        TurnsRequest,
        TurnsPage,
        TurnRow,
        SessionBundleRequest,
        SessionBundle,
        SessionIdentity,
        SessionActivity,
        SessionClassification,
        ClassificationKind,
        RawTokenCount,
        RawUsageVector,
        UsageEventSort,
        UsageEventsQuery,
        UsageEventsRequest,
        UsageEventRow,
        UsageEventsPage,
        CloseQuerySnapshotRequest,
        DashboardRequest,
        UsageSeriesBucket,
        RecentSession,
        DashboardBundle,
        CurrencyEstimate,
        UnpricedReason,
        PricingSummary,
        PriceOrigin,
        OfflinePriceTier,
        OfflineContextBand,
        OfflineReferenceBasis,
        OfflinePriceEntry,
        OfflinePriceCatalog,
        OfflinePriceCatalogSnapshot,
        PriceRevalueRequest,
        PriceRevalueState,
        PriceRevalueJob,
        PriceRevalueStatus,
        PriceRule,
        PriceRuleDraft,
        ModelAliasDraft,
        ModelAliasMutation,
        PriceRuleMutation,
        PriceRulesSnapshot,
        PriceChanged,
        ModelAlias,
        UnpricedCode,
        PriceOutcome,
        CoverageState,
        SourceIssue,
        FormatIssue,
        Coverage,
        ContextSnapshot,
        ScopeStart,
        MiniScope,
        QuotaState,
        QuotaWindow,
        QuotaLimit,
        QuotaSnapshot,
        token_pulse_core::quota::QuotaRefreshStatus,
        token_pulse_core::quota::QuotaRefreshResult,
        token_pulse_core::quota::QuotaChanged,
        token_pulse_core::quota::AccountServiceConfigSnapshot,
        token_pulse_core::quota::AccountServiceSelectionKind,
        token_pulse_core::quota::AccountServiceSelectionRequest,
        token_pulse_core::quota::AccountServiceSelection,
        token_pulse_core::quota::AccountServiceConfigMutation,
        token_pulse_core::quota::AccountConnectionRequest,
        MiniSnapshot,
        JobKind,
        JobState,
        Job,
        DiagnosticsRequest,
        DiagnosticKind,
        DiagnosticIssue,
        DiagnosticsSnapshot,
        JobScope,
        JobRequest,
        CancelJobResult,
        WindowAction,
        SourceOrigin,
        SourceReadability,
        CapabilityState,
        SourceCapabilities,
        SourceSummary,
        SourceDirectoryKind,
        SourceDirectorySelection,
        SourcesSnapshot,
        ManageSourceAction
    );
    // Generic response cannot be represented by a single JSON Schema. Instantiated responses can.
    schemas.insert(
        "AppStatusResponse".into(),
        serde_json::to_value(
            SchemaSettings::default()
                .for_serialize()
                .into_generator()
                .into_root_schema_for::<Response<AppStatus>>(),
        )
        .unwrap(),
    );
    ts.push_str("export ");
    ts.push_str(&Response::<AppStatus>::decl(&Config::default()));
    ts.push('\n');
    // Field documentation can make ts-rs emit spaces before a line break.
    // Keep generated artifacts clean without hand-editing the TypeScript file.
    let ts = ts.lines().map(str::trim_end).collect::<Vec<_>>().join("\n") + "\n";
    let json =
        serde_json::to_string_pretty(&serde_json::json!({"api_version":1,"schemas":schemas}))
            .unwrap()
            + "\n";
    for (path, content) in [
        (root.join("ui/src/shared/generated/contracts.ts"), ts),
        (root.join("schemas/protocol-v1.json"), json),
    ] {
        if check {
            assert_eq!(
                fs::read_to_string(&path).unwrap(),
                content,
                "protocol drift: {}",
                path.display()
            );
        } else {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
    }
}
