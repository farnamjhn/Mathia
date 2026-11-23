use tauri::{
    AppHandle, Runtime,
    menu::{Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder},
};

/// Builds the native application menubar for Mathia (primarily for macOS).
pub fn build_app_menu<R: Runtime>(
    app: &AppHandle<R>,
    recent_paths: &[std::path::PathBuf],
) -> tauri::Result<Menu<R>> {
    let mut builder = MenuBuilder::new(app);

    #[cfg(target_os = "macos")]
    {
        let app_menu = SubmenuBuilder::new(app, "Mathia")
            .about(None)
            .separator()
            .item(
                &MenuItemBuilder::with_id("scoring", "Detection Settings…")
                    .accelerator("CmdOrCtrl+,")
                    .build(app)?,
            )
            .separator()
            .services()
            .separator()
            .hide()
            .hide_others()
            .show_all()
            .separator()
            .quit()
            .build()?;
        builder = builder.item(&app_menu);
    }

    // --- File menu ---
    let mut file_builder = SubmenuBuilder::new(app, "File")
        .item(
            &MenuItemBuilder::with_id("open", "Open…")
                .accelerator("CmdOrCtrl+O")
                .build(app)?,
        )
        .item(&MenuItemBuilder::with_id("combine", "Combine…").build(app)?);

    // Recent files submenu
    let mut recent_builder = SubmenuBuilder::new(app, "Open Recent");
    if recent_paths.is_empty() {
        recent_builder = recent_builder.item(
            &MenuItemBuilder::with_id("no_recent", "No Recent Files")
                .enabled(false)
                .build(app)?,
        );
    } else {
        for path in recent_paths.iter().take(10) {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            let id = format!("recent:{}", path.display());
            recent_builder = recent_builder.item(&MenuItemBuilder::with_id(id, name).build(app)?);
        }
        recent_builder = recent_builder
            .separator()
            .item(&MenuItemBuilder::with_id("clear_recent", "Clear Recent").build(app)?);
    }
    let recent_submenu = recent_builder.build()?;
    file_builder = file_builder.item(&recent_submenu);

    let file_menu = file_builder
        .separator()
        .item(&MenuItemBuilder::with_id("home", "Back to Home").build(app)?)
        .item(
            &MenuItemBuilder::with_id("reload", "Compile Again")
                .accelerator("CmdOrCtrl+R")
                .build(app)?,
        )
        .separator()
        .item(
            &MenuItemBuilder::with_id("export_html", "Export as HTML…")
                .accelerator("CmdOrCtrl+E")
                .build(app)?,
        )
        .item(&MenuItemBuilder::with_id("export_markdown", "Export as Markdown…").build(app)?)
        .item(
            &MenuItemBuilder::with_id("export_pdf", "Export as PDF…")
                .accelerator("CmdOrCtrl+P")
                .build(app)?,
        )
        .item(&MenuItemBuilder::with_id("export_folder", "Export Folder…").build(app)?)
        .separator()
        .close_window()
        .build()?;

    // --- Edit menu ---
    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .item(
            &MenuItemBuilder::with_id("undo", "Undo")
                .accelerator("CmdOrCtrl+Z")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("redo", "Redo")
                .accelerator("CmdOrCtrl+Shift+Z")
                .build(app)?,
        )
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .separator()
        .item(&MenuItemBuilder::with_id("fix_markdown", "Fix Markdown…").build(app)?)
        .build()?;

    // --- View menu ---
    let view_menu = SubmenuBuilder::new(app, "View")
        .item(&MenuItemBuilder::with_id("preview_toggle", "Toggle Preview").build(app)?)
        .item(&MenuItemBuilder::with_id("theme_toggle", "Toggle Color Theme").build(app)?)
        .build()?;

    // --- Tools menu ---
    let tools_menu = SubmenuBuilder::new(app, "Tools")
        .item(&MenuItemBuilder::with_id("guide", "Transcription Prompt…").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("decision_run", "Review Blocks with Model").build(app)?)
        .item(&MenuItemBuilder::with_id("decision", "Decision Model…").build(app)?)
        .item(&MenuItemBuilder::with_id("scoring", "Detection Settings…").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("toggle_render_math", "Render Formulas").build(app)?)
        .item(&MenuItemBuilder::with_id("toggle_ai", "Add AI Study Tools").build(app)?)
        .build()?;

    // --- Window menu ---
    let window_menu = SubmenuBuilder::new(app, "Window")
        .minimize()
        .maximize()
        .fullscreen()
        .separator()
        .bring_all_to_front()
        .build()?;

    builder = builder
        .item(&file_menu)
        .item(&edit_menu)
        .item(&view_menu)
        .item(&tools_menu)
        .item(&window_menu);

    builder.build()
}
