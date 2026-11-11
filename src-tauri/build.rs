/// Every command the frontend may call. Listing them in the app manifest puts
/// them under Tauri's ACL: a window can invoke one only if a capability
/// (`capabilities/default.json`) grants its `allow-…` permission.
const COMMANDS: &[&str] = &[
    "pick_markdown",
    "startup_file",
    "analyze",
    "compile",
    "export",
    "export_pdf",
    "get_config",
    "set_config",
    "reset_config",
    "load_config",
    "save_config",
    "set_window_theme",
    "source_stamp",
    "set_overrides",
    "save_overrides",
    "load_overrides",
    "export_folder",
    "recent_files",
    "open_recent",
    "clear_recent",
    "get_decision_model",
    "set_decision_model",
    "test_decision_model",
    "run_decision_model",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run the Tauri build script");
}
