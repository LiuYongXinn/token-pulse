fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_app_status",
            "perform_window_action",
            "get_sources",
            "choose_source_directory",
            "manage_source",
        ]),
    ))
    .expect("failed to build desktop resources");
}
