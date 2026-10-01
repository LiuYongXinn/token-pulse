use schemars::{JsonSchema, generate::SchemaSettings};
use std::{collections::BTreeMap, fs, path::PathBuf};
use token_pulse_core::sources::*;
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
        ErrorCode,
        ErrorDetail,
        AppError,
        AppStatus,
        SnapshotMeta,
        DateRange,
        DimensionSelection,
        UsageFilter,
        PriceBasis,
        TokenMeasure,
        TokenTotals,
        CurrencyEstimate,
        UnpricedReason,
        PricingSummary,
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
        MiniSnapshot,
        JobKind,
        JobState,
        Job,
        WindowAction,
        SourceOrigin,
        SourceReadability,
        CapabilityState,
        SourceCapabilities,
        SourceSummary
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
