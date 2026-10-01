fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_app_status",
            "perform_window_action",
            "get_sources",
            "choose_source_directory",
            "manage_source",
            "start_job",
            "get_job",
            "list_jobs",
            "cancel_job",
            "get_context_snapshot",
            "get_dashboard_bundle",
            "get_grouped_usage",
            "get_filter_options",
            "query_sessions",
            "get_session_bundle",
            "query_turns",
            "query_usage_events",
            "close_query_snapshot",
            "get_price_rules",
            "save_price_rule",
            "retire_price_rule",
        ]),
    ))
    .expect("failed to build desktop resources");
}
