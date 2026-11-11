/// Every command the frontend may call. Listing them in the app manifest puts
/// them under Tauri's ACL: a window can invoke one only if a capability
/// (`capabilities/default.json`) grants its `allow-…` permission.
const COMMANDS: &[&str] = &[
    "pick_markdown",
    "startup_file",
    "analyze",
    "compile",
    "export",
    "get_config",
    "set_config",
    "reset_config",
    "load_config",
    "save_config",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run the Tauri build script");
}
