//! Mathia desktop app: a Tauri v2 shell over `math-core`.
//!
//! The frontend (`ui/`) has no file-system, dialog or shell permission. It
//! calls the commands below; file dialogs are opened here, and a path is only
//! read if the user picked it in a dialog or dropped it on the window
//! ([`Session::grant`]). The compiled document is shown in an `<iframe>` fed by
//! the `mathia-preview` protocol, so it keeps its own origin and its own
//! Content-Security-Policy.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod session;

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use math_core::{ConfigFormat, ScoringConfig};
use tauri::{AppHandle, DragDropEvent, Manager, State, WindowEvent};
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
    let granted = lock(&state).grant(&path)?;
    Ok(Some(display(&granted)))
}

/// The Markdown file given on the command line (`mathia-app notes.md`), if any.
#[tauri::command]
async fn startup_file(state: State<'_, Shared>) -> Result<Option<String>, String> {
    let Some(path) = std::env::args_os().nth(1).map(std::path::PathBuf::from) else {
        return Ok(None);
    };
    if !session::is_markdown(&path) {
        return Ok(None);
    }
    Ok(Some(display(&lock(&state).grant(&path)?)))
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
        .on_window_event(|window, event| {
            // A dropped Markdown file is as much the user's choice as one
            // picked in the dialog. The frontend gets the same paths through
            // the `tauri://drag-drop` event and asks for them afterwards.
            if let WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event {
                let state = window.state::<Shared>();
                let mut session = lock(&state);
                for path in paths.iter().filter(|p| session::is_markdown(p)) {
                    let _ = session.grant(path);
                }
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
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Mathia");
}
