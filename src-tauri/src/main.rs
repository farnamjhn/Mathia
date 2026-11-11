//! Mathia desktop app: a Tauri v2 shell over `math-core`.
//!
//! The frontend (`ui/`) has no file-system, dialog or shell permission. It
//! calls the commands below; file dialogs are opened here, and a path is only
//! read if the user picked it in a dialog or dropped it on the window
//! ([`Session::grant`]). The compiled document is shown in an `<iframe>` fed by
//! the `mathia-preview` protocol, so it keeps its own origin and its own
//! Content-Security-Policy.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod dock;
mod session;
mod theme;

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use math_core::{ConfigFormat, ScopeOverrides, ScoringConfig};
use tauri::{AppHandle, DragDropEvent, Manager, RunEvent, State, WindowEvent};
use tauri_plugin_dialog::DialogExt;

use session::{AnalysisView, CompileOptions, Compiled, MARKDOWN_EXTENSIONS, Session, Source};

/// Scheme of the preview frame; the frontend builds the URL with
/// `convertFileSrc(revision, "mathia-preview")`.
const PREVIEW_SCHEME: &str = "mathia-preview";

type Shared = Mutex<Session>;

fn lock(state: &Shared) -> MutexGuard<'_, Session> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Where the recent list is kept between runs.
fn recent_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_data_dir().ok().map(|dir| dir.join("recent.json"))
}

/// Best effort: a recent list that cannot be saved is not worth an error.
fn save_recent(app: &AppHandle, session: &Session) {
    let Some(file) = recent_path(app) else { return };
    let paths: Vec<_> = session.recent();
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string(&paths) {
        let _ = std::fs::write(file, json);
    }
}

fn load_recent(app: &AppHandle) -> Vec<std::path::PathBuf> {
    recent_path(app)
        .and_then(|file| std::fs::read_to_string(file).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Opens the file dialog. Returns the chosen path (canonical), or `None` if
/// the dialog was cancelled.
#[tauri::command]
async fn pick_markdown(app: AppHandle, state: State<'_, Shared>) -> Result<Option<String>, String> {
    let Some(picked) = app
        .dialog()
        .file()
        .add_filter("Markdown", MARKDOWN_EXTENSIONS)
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    let mut session = lock(&state);
    let granted = session.grant(&path)?;
    save_recent(&app, &session);
    Ok(Some(display(&granted)))
}

/// The files opened before that still exist, most recent first.
#[tauri::command]
async fn recent_files(state: State<'_, Shared>) -> Result<Vec<String>, String> {
    Ok(lock(&state).recent().iter().map(|p| display(p)).collect())
}

/// Opens a file from the recent list. Returns its canonical path.
#[tauri::command]
async fn open_recent(
    app: AppHandle,
    state: State<'_, Shared>,
    path: String,
) -> Result<String, String> {
    let mut session = lock(&state);
    let result = session.open_recent(Path::new(&path));
    save_recent(&app, &session);
    result.map(|p| display(&p))
}

#[tauri::command]
async fn clear_recent(app: AppHandle, state: State<'_, Shared>) -> Result<(), String> {
    let mut session = lock(&state);
    session.clear_recent();
    save_recent(&app, &session);
    Ok(())
}

/// The Markdown file given on the command line (`mathia-app notes.md`), if any.
#[tauri::command]
async fn startup_file(app: AppHandle, state: State<'_, Shared>) -> Result<Option<String>, String> {
    let Some(path) = std::env::args_os().nth(1).map(std::path::PathBuf::from) else {
        return Ok(None);
    };
    if !session::is_markdown(&path) {
        return Ok(None);
    }
    let mut session = lock(&state);
    let granted = session.grant(&path)?;
    save_recent(&app, &session);
    Ok(Some(display(&granted)))
}

#[tauri::command]
async fn analyze(state: State<'_, Shared>, source: Source) -> Result<AnalysisView, String> {
    lock(&state).analyze(&source)
}

/// Compiles `source` and makes it the preview document.
#[tauri::command]
async fn compile(
    state: State<'_, Shared>,
    source: Source,
    options: CompileOptions,
) -> Result<Compiled, String> {
    lock(&state).compile_preview(&source, &options)
}

/// Compiles `source` and writes it where the user says. Returns the
/// destination, or `None` if the dialog was cancelled.
#[tauri::command]
async fn export(
    app: AppHandle,
    state: State<'_, Shared>,
    source: Source,
    options: CompileOptions,
) -> Result<Option<String>, String> {
    let (html, _) = lock(&state).compile(&source, &options)?;
    let mut dialog = app.dialog().file().add_filter("HTML", &["html"]);
    let stem = source.path().and_then(Path::file_stem);
    dialog = dialog.set_file_name(format!(
        "{}.html",
        stem.map(|s| s.to_string_lossy())
            .as_deref()
            .unwrap_or("document")
    ));
    if let Some(dir) = source.path().and_then(Path::parent) {
        dialog = dialog.set_directory(dir);
    }
    let Some(dest) = dialog.blocking_save_file() else {
        return Ok(None);
    };
    let dest = dest.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&dest, html).map_err(|e| format!("{}: {e}", dest.display()))?;
    Ok(Some(display(&dest)))
}

/// An opaque stamp that changes when the opened file changes on disk; the
/// frontend polls it to recompile edits made in another program.
#[tauri::command]
async fn source_stamp(state: State<'_, Shared>, source: Source) -> Result<Option<String>, String> {
    lock(&state).stamp(&source)
}

/// Replaces the hand corrections for the open document.
#[tauri::command]
async fn set_overrides(state: State<'_, Shared>, overrides: ScopeOverrides) -> Result<(), String> {
    lock(&state).set_overrides(overrides);
    Ok(())
}

/// Saves the corrections as JSON (usable with `mathia --overrides`).
#[tauri::command]
async fn save_overrides(
    app: AppHandle,
    state: State<'_, Shared>,
    source: Source,
) -> Result<Option<String>, String> {
    let overrides = lock(&state).overrides().clone();
    let stem = source.path().and_then(Path::file_stem);
    let mut dialog = app
        .dialog()
        .file()
        .add_filter("JSON", &["json"])
        .set_file_name(format!(
            "{}.overrides.json",
            stem.map(|s| s.to_string_lossy())
                .as_deref()
                .unwrap_or("document")
        ));
    if let Some(dir) = source.path().and_then(Path::parent) {
        dialog = dialog.set_directory(dir);
    }
    let Some(dest) = dialog.blocking_save_file() else {
        return Ok(None);
    };
    let dest = dest.into_path().map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(&overrides).map_err(|e| e.to_string())?;
    std::fs::write(&dest, json + "\n").map_err(|e| format!("{}: {e}", dest.display()))?;
    Ok(Some(display(&dest)))
}

/// Loads corrections from a JSON file the user picks (a saved set, or a
/// `mathia --judge-log` file) and makes them current.
#[tauri::command]
async fn load_overrides(
    app: AppHandle,
    state: State<'_, Shared>,
) -> Result<Option<ScopeOverrides>, String> {
    let Some(picked) = app
        .dialog()
        .file()
        .add_filter("JSON", &["json"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    // A judge log keeps the overrides under "overrides".
    let value = match value.get("overrides") {
        Some(inner) => inner.clone(),
        None => value,
    };
    let overrides: ScopeOverrides =
        serde_json::from_value(value).map_err(|e| format!("{}: {e}", path.display()))?;
    lock(&state).set_overrides(overrides.clone());
    Ok(Some(overrides))
}

/// Compiles every Markdown file of a folder the user picks into a folder
/// the user picks. Returns the number of files and the destination.
#[tauri::command]
async fn export_folder(
    app: AppHandle,
    state: State<'_, Shared>,
    options: CompileOptions,
) -> Result<Option<(usize, String)>, String> {
    let Some(dir) = app
        .dialog()
        .file()
        .set_title("Folder of Markdown files")
        .blocking_pick_folder()
    else {
        return Ok(None);
    };
    let dir = dir.into_path().map_err(|e| e.to_string())?;
    let Some(out) = app
        .dialog()
        .file()
        .set_title("Where to write the HTML files")
        .set_directory(&dir)
        .blocking_pick_folder()
    else {
        return Ok(None);
    };
    let out = out.into_path().map_err(|e| e.to_string())?;
    let written = lock(&state).compile_folder(&dir, &out, &options)?;
    Ok(Some((written.len(), display(&out))))
}

#[tauri::command]
async fn get_config(state: State<'_, Shared>) -> Result<ScoringConfig, String> {
    Ok(lock(&state).config().clone())
}

#[tauri::command]
async fn set_config(state: State<'_, Shared>, config: ScoringConfig) -> Result<(), String> {
    lock(&state).set_config(config)
}

#[tauri::command]
async fn reset_config(state: State<'_, Shared>) -> Result<ScoringConfig, String> {
    let mut session = lock(&state);
    session.set_config(ScoringConfig::default())?;
    Ok(session.config().clone())
}

/// Loads a scoring configuration chosen in a file dialog and makes it current.
#[tauri::command]
async fn load_config(
    app: AppHandle,
    state: State<'_, Shared>,
) -> Result<Option<ScoringConfig>, String> {
    let Some(picked) = app
        .dialog()
        .file()
        .add_filter("Scoring configuration", &["toml", "json"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    let config = ScoringConfig::load(&path).map_err(|e| e.to_string())?;
    lock(&state).set_config(config.clone())?;
    Ok(Some(config))
}

/// Saves the current scoring configuration (TOML or JSON, by extension).
#[tauri::command]
fn set_window_theme(window: tauri::WebviewWindow, theme: Option<String>) -> Result<String, String> {
    let choice = theme::ThemeChoice::parse(theme.as_deref())?;
    let os_theme = theme::apply(&window, choice)?;
    Ok(os_theme.to_string())
}

#[tauri::command]
async fn save_config(app: AppHandle, state: State<'_, Shared>) -> Result<Option<String>, String> {
    let config = lock(&state).config().clone();
    let Some(dest) = app
        .dialog()
        .file()
        .add_filter("TOML", &["toml"])
        .add_filter("JSON", &["json"])
        .set_file_name("scoring.toml")
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let dest = dest.into_path().map_err(|e| e.to_string())?;
    ConfigFormat::from_path(&dest).map_err(|e| e.to_string())?;
    config.save(&dest).map_err(|e| e.to_string())?;
    Ok(Some(display(&dest)))
}

fn main() {
    // A development launch starts from a raw executable, so macOS initially
    // assigns it the generic `exec` icon. Initializing AppKit and replacing
    // that icon before Tauri builds its event loop prevents that first flash.
    dock::refresh(None);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Shared::default())
        .register_uri_scheme_protocol(PREVIEW_SCHEME, |context, _request| {
            let state = context.app_handle().state::<Shared>();
            let html = lock(&state).preview().as_bytes().to_vec();
            tauri::http::Response::builder()
                .header("Content-Type", "text/html; charset=utf-8")
                .header("Cache-Control", "no-store")
                .header("X-Content-Type-Options", "nosniff")
                .body(html)
                .expect("static response headers are valid")
        })
        .setup(|app| {
            let saved = load_recent(app.handle());
            lock(&app.state::<Shared>()).set_recent(saved);
            // Replace the static bundle icon as soon as AppKit is available.
            // Waiting for `RunEvent::Ready` leaves the light bundle icon visible
            // while the application is starting in Dark Mode.
            dock::refresh(None);
            Ok(())
        })
        .on_window_event(|window, event| {
            match event {
                WindowEvent::ThemeChanged(theme) => {
                    dock::sync(Some(*theme));
                    theme::sync_os_content_appearance(window, *theme);
                }
                WindowEvent::Focused(true) => {
                    dock::sync(None);
                }
                // A dropped Markdown file is as much the user's choice as one
                // picked in the dialog. The frontend gets the same paths through
                // the `tauri://drag-drop` event and asks for them afterwards.
                WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) => {
                    let state = window.state::<Shared>();
                    let mut session = lock(&state);
                    for path in paths.iter().filter(|p| session::is_markdown(p)) {
                        let _ = session.grant(path);
                    }
                    save_recent(window.app_handle(), &session);
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            pick_markdown,
            startup_file,
            analyze,
            compile,
            export,
            get_config,
            set_config,
            reset_config,
            load_config,
            save_config,
            set_window_theme,
            source_stamp,
            set_overrides,
            save_overrides,
            load_overrides,
            export_folder,
            recent_files,
            open_recent,
            clear_recent,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build Mathia")
        .run(|_app_handle, event| {
            if let RunEvent::Ready = event {
                // In dev builds on macOS, Tauri sets the light bundle icon
                // right after `setup` returns (inside `RuntimeRunEvent::Ready`)
                // before emitting `RunEvent::Ready`, so apply ours here.
                dock::refresh(None);
            }
        });
}
