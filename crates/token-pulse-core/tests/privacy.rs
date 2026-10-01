use serde_json::{Value, json};
use token_pulse_core::{
    error::ErrorCode,
    numeric::DecimalInt,
    pricing::{PriceOutcome, PricingAccumulator},
    privacy::{DisplayPolicyStamp, PrivacyRedact, PrivacyState, PrivateResponse, alias},
    protocol::{PriceBasis, PricingSummary},
    query::{
        DashboardBundle, FilterOptionsPage, GroupDimension, GroupedUsageBundle, SessionBundle,
        SessionsPage, UsageEventsPage,
    },
    sources::SourcesSnapshot,
};
fn totals() -> Value {
    let unknown = json!({"value":null,"covered_total_tokens":"0","complete":false});
    json!({"total_tokens":"9007199254740994","input_total":unknown,"cached_input":unknown,"noncached_input":unknown,"output_total":unknown,"reasoning_output":unknown,"session_count":"1","usage_event_count":"2","reliable_turn_count":null,"reliable_turns_complete":false})
}
fn pricing() -> Value {
    json!({"redacted":false,"basis":{"mode":"event_time"},"currencies":[{"currency":"USD","estimated_cost":"9.007199254740993","priced_total_tokens":"9007199254740993"}],"priced_total_tokens":"9007199254740993","unpriced_total_tokens":"1","reasons":[{"code":"missing_rule","total_tokens":"1","event_count":"1"}],"calculating":false})
}
fn meta() -> Value {
    json!({"snapshot_id":"immutable-usage-snapshot","data_revision":"7","price_revision":"3","generated_at_ms":1000,"parser_versions":["v1"],"accounting_versions":["v2"],"display_timezone":"UTC"})
}
fn coverage() -> Value {
    json!({"state":"unknown","pending_observation_count":"0","unattributed_observation_count":"0","unattributed_total_tokens":null,"pending_file_count":"0","source_issues":[],"format_issues":[],"breakdown_complete":false})
}
fn context() -> Value {
    json!({"context_tokens":null,"model_context_window":null,"percentage":null,"observed_at_ms":null,"quality":"unknown"})
}
fn base() -> Value {
    json!({"meta":meta(),"summary":totals(),"pricing":pricing(),"coverage":coverage()})
}
fn identity(key: &str) -> Value {
    json!({"session_key":key,"display_name":"SECRET session title","parent_key":"parent-key","parent_display_name":"SECRET parent title","parent_provider_id":"SECRET unresolved provider identity"})
}
fn stamp(revision: u8, enabled: bool) -> DisplayPolicyStamp {
    DisplayPolicyStamp {
        settings_revision: DecimalInt::from_nonnegative(revision.into()).unwrap(),
        privacy: enabled,
    }
}
fn assert_price(value: &Value) {
    assert_eq!(value["redacted"], true);
    assert_eq!(value["currencies"][0]["estimated_cost"], Value::Null);
    assert_eq!(value["priced_total_tokens"], "9007199254740993");
    assert_eq!(value["reasons"], json!([]));
}
fn no_secret(value: &Value) {
    let s = serde_json::to_string(value).unwrap();
    assert!(!s.contains("SECRET"));
    assert!(!s.contains("9.007199254740993"));
}

#[test]
fn serialization_uses_latest_policy_after_response_construction_without_mutating_usage_data() {
    let mut value = base();
    value["series"] = json!([]);
    value["heatmap"] = json!([]);
    value["recent_sessions"] = json!([{"session_key":"session-key","display_name":"SECRET title","latest_at_ms":1000,"latest_model":"public-model","latest_project_id":"project-key","latest_project_name":"SECRET project path","summary":totals(),"pricing":pricing()}]);
    let original: DashboardBundle = serde_json::from_value(value.clone()).unwrap();
    let policy = PrivacyState::new(stamp(1, false));
    let response = PrivateResponse::new("request-1".into(), original.clone(), policy.clone());
    policy.publish(stamp(2, true)).unwrap();
    let sent = serde_json::to_value(response).unwrap();
    no_secret(&sent);
    assert_eq!(
        sent["display_policy"],
        json!({"settings_revision":"2","privacy":true})
    );
    assert_eq!(sent["data"]["summary"], totals());
    assert_eq!(sent["data"]["meta"], meta());
    assert_eq!(
        sent["data"]["recent_sessions"][0]["display_name"],
        alias("会话", Some("session-key"))
    );
    assert_eq!(
        sent["data"]["recent_sessions"][0]["latest_project_name"],
        alias("项目", Some("project-key"))
    );
    assert_eq!(
        sent["data"]["recent_sessions"][0]["latest_model"],
        "public-model"
    );
    assert_price(&sent["data"]["pricing"]);
    assert_price(&sent["data"]["recent_sessions"][0]["pricing"]);
    assert_eq!(serde_json::to_value(&original).unwrap(), value);
    policy.publish(stamp(3, false)).unwrap();
    let fresh =
        serde_json::to_value(PrivateResponse::new("request-2".into(), original, policy)).unwrap();
    assert_eq!(fresh["data"], value);
    assert_eq!(fresh["display_policy"]["settings_revision"], "3");
}
#[test]
fn aliases_are_shared_across_list_detail_children_and_events_and_unknowns_stay_null() {
    let mut list = base();
    let mut row = identity("session-key");
    for (key, value) in [
        ("latest_at_ms", json!(1000)),
        ("latest_model", json!("public-model")),
        ("latest_project_id", json!("project-key")),
        ("latest_project_name", json!("SECRET project")),
        ("child_count", json!("1")),
        ("summary", totals()),
        ("pricing", pricing()),
        ("coverage", coverage()),
        ("latest_context", context()),
    ] {
        row[key] = value;
    }
    list["sessions"] = json!([row]);
    list["next_cursor"] = json!("x".repeat(151));
    let mut list: SessionsPage = serde_json::from_value(list).unwrap();
    let original = serde_json::to_value(&list).unwrap();
    list.redact();
    let once = serde_json::to_value(&list).unwrap();
    list.redact();
    assert_eq!(serde_json::to_value(list).unwrap(), once);
    no_secret(&once);
    assert_eq!(once["summary"], totals());
    assert_eq!(once["next_cursor"], original["next_cursor"]);
    assert_eq!(
        once["sessions"][0]["parent_display_name"],
        alias("会话", Some("parent-key"))
    );
    assert_eq!(once["sessions"][0]["latest_context"], context());
    let mut detail = base();
    detail["identity"] = identity("session-key");
    detail["latest_context"] = context();
    detail["latest_selected_activity"] = json!({"occurred_at_ms":1000,"model":"public-model","project_id":null,"project_display_name":null});
    detail["child_count"] = json!("1");
    detail["children"] = json!([identity("child-key")]);
    detail["children_truncated"] = json!(false);
    detail["classifications"] = json!([]);
    let mut detail: SessionBundle = serde_json::from_value(detail).unwrap();
    detail.redact();
    let detail = serde_json::to_value(detail).unwrap();
    no_secret(&detail);
    assert_eq!(
        detail["identity"]["display_name"],
        once["sessions"][0]["display_name"]
    );
    assert_eq!(
        detail["latest_selected_activity"]["project_display_name"],
        Value::Null
    );
    assert_eq!(
        detail["children"][0]["display_name"],
        alias("会话", Some("child-key"))
    );
    let vector = json!({"input_total":"-1","cached_input":null,"output_total":"9007199254740993","reasoning_output":null,"reported_total":"9007199254740993"});
    let mut increment = vector.clone();
    increment["input_total"] = Value::Null;
    let mut events = base();
    events["events"] = json!([{"event_id":"event-key","session_key":"session-key","session_display_name":"SECRET title","occurred_at_ms":1000,"model":"public-model","provider":"public-provider","project_id":"project-key","project_display_name":"SECRET path","source_ids":["source-key"],"turn_id":null,"total_tokens":"9007199254740993","usage":increment,"raw_last":vector,"raw_cumulative":null,"calculation_method":"last_with_baseline","quality_flags":["confirmed"],"price":{"status":"priced","rule_id":"SECRET private rule","currency":"USD","cost_atoms":"9007199254740993","estimated_cost":"9.007199254740993"},"parser_version":"v1","accounting_version":"v2"}]);
    events["next_cursor"] = json!("y".repeat(151));
    let mut events: UsageEventsPage = serde_json::from_value(events).unwrap();
    events.redact();
    let events = serde_json::to_value(events).unwrap();
    no_secret(&events);
    assert_eq!(
        events["events"][0]["session_display_name"],
        once["sessions"][0]["display_name"]
    );
    assert_eq!(events["events"][0]["price"], json!({"status":"redacted"}));
    assert_eq!(events["events"][0]["raw_last"], vector);
    assert_eq!(events["events"][0]["raw_cumulative"], Value::Null);
    assert_eq!(events["events"][0]["turn_id"], Value::Null);
}
#[test]
fn grouped_context_never_leaks_project_names_and_facet_keys_and_counts_stay_unchanged() {
    let mut groups = base();
    groups["total_group_count"] = json!("1");
    groups["truncated"] = json!(false);
    groups["groups"] = json!([{"key":"project-key","display_name":"SECRET project","totals":totals(),"pricing":pricing(),"coverage":coverage()}]);
    let group: GroupedUsageBundle = serde_json::from_value(groups).unwrap();
    let policy = PrivacyState::new(stamp(1, true));
    let generic = serde_json::to_value(PrivateResponse::new(
        "r".into(),
        group.clone(),
        policy.clone(),
    ))
    .unwrap();
    no_secret(&generic);
    let projects = serde_json::to_value(PrivateResponse::groups(
        "r".into(),
        group.clone(),
        policy.clone(),
        GroupDimension::Projects,
    ))
    .unwrap();
    no_secret(&projects);
    assert_eq!(
        projects["data"]["groups"][0]["display_name"],
        alias("项目", Some("project-key"))
    );
    let mut model = group;
    model.groups[0].display_name = "public-model".into();
    let models = serde_json::to_value(PrivateResponse::groups(
        "r".into(),
        model,
        policy,
        GroupDimension::Models,
    ))
    .unwrap();
    assert_eq!(models["data"]["groups"][0]["display_name"], "public-model");
    assert_price(&models["data"]["groups"][0]["pricing"]);
    for (dimension, kind) in [
        ("projects", "项目"),
        ("sessions", "会话"),
        ("sources", "来源"),
    ] {
        let mut page:FilterOptionsPage=serde_json::from_value(json!({"meta":meta(),"dimension":dimension,"options":[{"key":"opaque-key","display_name":"SECRET name","count":"9007199254740993"},{"key":null,"display_name":"未知","count":"0"}],"next_cursor":"z".repeat(151)})).unwrap();
        page.redact();
        let value = serde_json::to_value(page).unwrap();
        no_secret(&value);
        assert_eq!(value["options"][0]["key"], "opaque-key");
        assert_eq!(value["options"][0]["count"], "9007199254740993");
        assert_eq!(
            value["options"][0]["display_name"],
            alias(kind, Some("opaque-key"))
        );
        assert_eq!(value["options"][1]["key"], Value::Null);
        assert_eq!(value["options"][1]["display_name"], "未知");
    }
}
#[test]
fn source_paths_are_removed_and_old_policy_publications_cannot_restore_raw_display() {
    let mut sources:SourcesSnapshot=serde_json::from_value(json!({"settings_revision":"9007199254740993","sources":[{"source_id":"source-key","root_path":"E:\\SECRET\\.codex","origin":"custom","enabled":true,"removed":false,"readability":"awaiting_directory","capabilities":{"physical_identity":"not_probed","byte_seek":"not_probed","watcher":"unavailable","polling_required":true},"last_scan_at_ms":null,"last_success_at_ms":null,"error":null}]})).unwrap();
    sources.redact();
    let value = serde_json::to_value(sources).unwrap();
    no_secret(&value);
    assert_eq!(
        value["sources"][0]["root_path"],
        alias("来源", Some("source-key"))
    );
    assert_eq!(value["sources"][0]["last_success_at_ms"], Value::Null);
    let policy = PrivacyState::new(stamp(2, true));
    assert_eq!(
        policy.publish(stamp(1, false)),
        Err(ErrorCode::RevisionConflict)
    );
    assert_eq!(
        policy.publish(stamp(2, false)),
        Err(ErrorCode::RevisionConflict)
    );
    assert!(policy.current().unwrap().privacy);
    policy.publish(stamp(2, true)).unwrap();
    policy.publish(stamp(3, false)).unwrap();
    assert!(!policy.current().unwrap().privacy);
}
#[test]
fn display_only_redacted_outcome_cannot_be_accounted_as_zero_or_unpriced() {
    let mut accumulator = PricingAccumulator::new(PriceBasis::EventTime {});
    let before = serde_json::to_value(accumulator.summary(false).unwrap()).unwrap();
    assert_eq!(
        accumulator.push(110, PriceOutcome::Redacted {}),
        Err(ErrorCode::InvalidQuery)
    );
    assert_eq!(
        serde_json::to_value(accumulator.summary(false).unwrap()).unwrap(),
        before
    );
    let mut summary: PricingSummary = serde_json::from_value(pricing()).unwrap();
    summary.redact();
    assert_price(&serde_json::to_value(summary).unwrap());
}
