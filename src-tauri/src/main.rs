//! Mathia desktop app: a Tauri v2 shell over `math-core`.
//!
//! The frontend (`ui/`) has no file-system, dialog or shell permission. It
//! calls the commands below; file dialogs are opened here, and a path is only
//! read if the user picked it in a dialog or dropped it on the window
//! ([`Session::grant`]). The compiled document is shown in an `<iframe>` fed by
//! the `mathia-preview` protocol, so it keeps its own origin and its own
//! Content-Security-Policy.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod decision;
mod dock;
mod menu;
mod session;
mod theme;
mod update;

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use math_core::{ConfigFormat, ScopeOverrides, ScoringConfig};
use tauri::{
    AppHandle, DragDropEvent, Emitter, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder,
    WindowEvent,
};
use tauri_plugin_dialog::DialogExt;

use session::{AnalysisView, CompileOptions, Compiled, Session, Source};

#[derive(Clone, Debug, serde::Serialize)]
pub struct CompileProgress {
    pub progress: f32,
    pub stage: String,
}

/// Scheme of the preview frame; the frontend builds the URL with
/// `convertFileSrc(revision, "mathia-preview")`.
const PREVIEW_SCHEME: &str = "mathia-preview";

type Shared = Mutex<Session>;
type SharedDecision = Mutex<decision::Decision>;

fn lock_decision(state: &SharedDecision) -> MutexGuard<'_, decision::Decision> {
    state.lock().unwrap_or_else(|p| p.into_inner())
}

fn data_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

/// The document waiting in the print window. Kept apart from the session's
/// preview so printing never disturbs the preview frame's revision.
#[derive(Default)]
struct PrintDoc(Mutex<String>);

/// Path of the print document on the preview scheme; every other path serves
/// the preview.
const PRINT_PATH: &str = "/print";
const PRINT_LABEL: &str = "print";

fn lock(state: &Shared) -> MutexGuard<'_, Session> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Where the recent list is kept between runs.
fn recent_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join("recent.json"))
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
    #[cfg(target_os = "macos")]
    {
        if let Ok(m) = menu::build_app_menu(app, &paths) {
            let _ = app.set_menu(m);
        }
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

/// The file name (without extension) a save dialog proposes for `source`.
fn default_stem(source: &Source, title: Option<&str>) -> String {
    if let Some(title) = title {
        return title.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
    }
    match source {
        Source::Path(path) => path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "document".into()),
        Source::MultiPath(paths) => {
            if paths.len() == 1 {
                paths[0]
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "document".into())
            } else {
                "combined".into()
            }
        }
        Source::Text(_) => "document".into(),
    }
}

/// Opens the file dialog. Returns the chosen paths (canonical) in order,
/// or an empty list if the dialog was cancelled.
#[tauri::command]
async fn pick_markdown(app: AppHandle, state: State<'_, Shared>) -> Result<Vec<String>, String> {
    let Some(picked) = app
        .dialog()
        .file()
        .add_filter("Markdown and HTML", session::SUPPORTED_EXTENSIONS)
        .add_filter("Markdown", session::MARKDOWN_EXTENSIONS)
        .add_filter("HTML", session::HTML_EXTENSIONS)
        .blocking_pick_files()
    else {
        return Ok(Vec::new());
    };
    let mut session = lock(&state);
    let mut granted = Vec::new();
    for item in picked {
        let path = item.into_path().map_err(|e| e.to_string())?;
        let canon = session.grant(&path)?;
        granted.push(display(&canon));
    }
    save_recent(&app, &session);
    Ok(granted)
}

/// Explicitly grants a list of paths so the session can read them.
#[tauri::command]
async fn grant_paths(
    app: AppHandle,
    state: State<'_, Shared>,
    paths: Vec<String>,
) -> Result<Vec<String>, String> {
    let mut session = lock(&state);
    let mut granted = Vec::new();
    for p in paths {
        let path = std::path::PathBuf::from(&p);
        if session::is_supported(&path) {
            let canon = session.grant(&path)?;
            granted.push(display(&canon));
        }
    }
    save_recent(&app, &session);
    Ok(granted)
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

/// The Markdown or HTML files given on the command line (`mathia-app notes.md`), if any.
#[tauri::command]
async fn startup_file(app: AppHandle, state: State<'_, Shared>) -> Result<Vec<String>, String> {
    let args: Vec<std::path::PathBuf> = std::env::args_os()
        .skip(1)
        .map(std::path::PathBuf::from)
        .collect();
    let supported: Vec<_> = args
        .into_iter()
        .filter(|p| session::is_supported(p))
        .collect();
    if supported.is_empty() {
        return Ok(Vec::new());
    }
    let mut session = lock(&state);
    let mut granted = Vec::new();
    for path in supported {
        if let Ok(canon) = session.grant(&path) {
            granted.push(display(&canon));
        }
    }
    save_recent(&app, &session);
    Ok(granted)
}

#[tauri::command]
async fn analyze(state: State<'_, Shared>, source: Source) -> Result<AnalysisView, String> {
    lock(&state).analyze(&source)
}

/// Compiles `source` and makes it the preview document.
#[tauri::command]
async fn compile(
    app: AppHandle,
    state: State<'_, Shared>,
    source: Source,
    options: CompileOptions,
) -> Result<Compiled, String> {
    app.emit(
        "compile-progress",
        CompileProgress {
            progress: 0.05,
            stage: "Preparing compilation…".to_string(),
        },
    )
    .ok();

    let result =
        lock(&state).compile_preview_with_progress(&source, &options, |progress, stage| {
            app.emit(
                "compile-progress",
                CompileProgress {
                    progress,
                    stage: stage.to_string(),
                },
            )
            .ok();
        });

    if result.is_ok() {
        app.emit(
            "compile-progress",
            CompileProgress {
                progress: 0.95,
                stage: "HTML compiled".to_string(),
            },
        )
        .ok();
    }

    result
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
    app.emit(
        "compile-progress",
        CompileProgress {
            progress: 0.05,
            stage: "Compiling HTML for export…".to_string(),
        },
    )
    .ok();

    let (html, _) = lock(&state).compile_with_progress(&source, &options, |progress, stage| {
        app.emit(
            "compile-progress",
            CompileProgress {
                progress,
                stage: stage.to_string(),
            },
        )
        .ok();
    })?;

    app.emit(
        "compile-progress",
        CompileProgress {
            progress: 1.0,
            stage: "Complete".to_string(),
        },
    )
    .ok();

    let mut dialog = app.dialog().file().add_filter("HTML", &["html"]);
    let default_name = default_stem(&source, options.title.as_deref());
    dialog = dialog.set_file_name(format!("{default_name}.html"));
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

/// Writes the compiled document as Markdown where the user says. Returns the
/// destination, or `None` if the dialog was cancelled.
#[tauri::command]
async fn export_markdown(
    app: AppHandle,
    state: State<'_, Shared>,
    source: Source,
) -> Result<Option<String>, String> {
    let markdown = lock(&state).markdown(&source)?;
    let mut dialog = app
        .dialog()
        .file()
        .add_filter("Markdown", &["md"])
        .set_file_name(format!("{}.md", default_stem(&source, None)));
    if let Some(dir) = source.path().and_then(Path::parent) {
        dialog = dialog.set_directory(dir);
    }
    let Some(dest) = dialog.blocking_save_file() else {
        return Ok(None);
    };
    let dest = dest.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&dest, markdown).map_err(|e| format!("{}: {e}", dest.display()))?;
    Ok(Some(display(&dest)))
}

/// Save a rendered map from the sandboxed preview through the native dialog.
#[tauri::command]
async fn export_mindmap(
    app: AppHandle,
    extension: String,
    bytes: Vec<u8>,
) -> Result<Option<String>, String> {
    if !matches!(extension.as_str(), "svg" | "png") || bytes.len() > 48 * 1024 * 1024 {
        return Err("Unsupported mind map export".into());
    }
    let valid = match extension.as_str() {
        "png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        _ => std::str::from_utf8(&bytes).is_ok_and(|text| text.trim_start().starts_with("<svg")),
    };
    if !valid {
        return Err("Invalid mind map image".into());
    }
    let Some(dest) = app
        .dialog()
        .file()
        .add_filter(extension.to_uppercase(), &[extension.as_str()])
        .set_file_name(format!("mathia-mindmap.{extension}"))
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let dest = dest.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&dest, bytes).map_err(|e| format!("{}: {e}", dest.display()))?;
    Ok(Some(display(&dest)))
}

/// What `fix_markdown` reports.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Fixed {
    /// False when the source needed no repair; nothing is written then.
    changed: bool,
    /// Inline formulas that got their `$ … $` back.
    formulas: usize,
    /// Where the repaired Markdown was written; `None` if the dialog was
    /// cancelled (or nothing changed).
    path: Option<String>,
}

/// Repairs the Markdown and LaTeX of `source` and writes the result where the
/// user says: a copy by default, or over the original if they pick it. The
/// destination becomes a file the session may open.
#[tauri::command]
async fn fix_markdown(
    app: AppHandle,
    state: State<'_, Shared>,
    source: Source,
) -> Result<Fixed, String> {
    let repair = lock(&state).repair(&source)?;
    if !repair.changed {
        return Ok(Fixed {
            changed: false,
            formulas: 0,
            path: None,
        });
    }
    let stem = match &source {
        Source::Path(p) => p
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "document".into()),
        Source::MultiPath(_) => "combined".into(),
        Source::Text(_) => "document".into(),
    };
    let mut dialog = app
        .dialog()
        .file()
        .add_filter("Markdown", &["md"])
        .set_file_name(format!("{stem}.fixed.md"));
    if let Some(dir) = source.path().and_then(Path::parent) {
        dialog = dialog.set_directory(dir);
    }
    let Some(dest) = dialog.blocking_save_file() else {
        return Ok(Fixed {
            changed: true,
            formulas: repair.formulas,
            path: None,
        });
    };
    let dest = dest.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&dest, &repair.text).map_err(|e| format!("{}: {e}", dest.display()))?;
    let mut session = lock(&state);
    let granted = session.grant(&dest)?;
    save_recent(&app, &session);
    Ok(Fixed {
        changed: true,
        formulas: repair.formulas,
        path: Some(display(&granted)),
    })
}

/// Compiles `source` and opens it in a window of its own, with the system print
/// dialog on top: choose "Save as PDF" there. A window without any permission
/// (no capability names it) shows the document, so it cannot reach the app's
/// commands. The compiled page's print stylesheet lays out the pages.
#[tauri::command]
async fn export_pdf(
    app: AppHandle,
    state: State<'_, Shared>,
    print: State<'_, PrintDoc>,
    source: Source,
    options: CompileOptions,
) -> Result<(), String> {
    let (html, _) = lock(&state).compile(&source, &options)?;
    *print.0.lock().unwrap_or_else(|p| p.into_inner()) = html;

    let title = options
        .title
        .as_deref()
        .map(ToOwned::to_owned)
        .or_else(|| match &source {
            Source::Path(p) => p.file_stem().map(|s| s.to_string_lossy().into_owned()),
            Source::MultiPath(paths) => {
                if paths.len() == 1 {
                    paths[0]
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                } else {
                    Some("Combined Document".into())
                }
            }
            Source::Text(_) => None,
        })
        .unwrap_or_else(|| "Document".into());

    // A window left open from the last time shows the old document: reload it.
    if let Some(window) = app.get_webview_window(PRINT_LABEL) {
        window
            .eval("location.reload()")
            .map_err(|e| e.to_string())?;
        let _ = window.set_title(&format!("{title} — Mathia"));
        let _ = window.set_focus();
        return Ok(());
    }

    // Windows serves custom schemes as `http://<scheme>.localhost`.
    let url = if cfg!(windows) {
        format!("http://{PREVIEW_SCHEME}.localhost{PRINT_PATH}")
    } else {
        format!("{PREVIEW_SCHEME}://localhost{PRINT_PATH}")
    };
    let url = url.parse().map_err(|e| format!("{e}"))?;
    WebviewWindowBuilder::new(&app, PRINT_LABEL, WebviewUrl::External(url))
        .title(format!("{title} — Mathia"))
        .inner_size(900.0, 1100.0)
        .on_page_load(|webview, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                // Give KaTeX a moment to typeset before the dialog snapshots it.
                let webview = webview.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(600));
                    let target = webview.clone();
                    let _ = webview.run_on_main_thread(move || {
                        let _ = target.print();
                    });
                });
            }
        })
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// An opaque stamp that changes when the opened file changes on disk; the
/// frontend polls it to recompile edits made in another program.
#[tauri::command]
async fn source_stamp(state: State<'_, Shared>, source: Source) -> Result<Option<String>, String> {
    lock(&state).stamp(&source)
}

/// Replaces the hand corrections for the open document.
#[tauri::command]
async fn set_overrides(
    app: AppHandle,
    state: State<'_, Shared>,
    overrides: ScopeOverrides,
) -> Result<(), String> {
    let mut session = lock(&state);
    session.set_overrides(overrides.clone());
    if session.config().ml_telemetry && !overrides.is_empty() {
        let Some(content) = session.current_content() else {
            return Ok(());
        };
        if let Ok(dir) = data_dir(&app) {
            let telemetry_path = dir.join("telemetry.jsonl");
            let _ = session.record_override_telemetry(&content, &telemetry_path);
        }
        let ml_data = std::path::Path::new("ml/data/feedback.jsonl");
        if ml_data.parent().map(|p| p.is_dir()).unwrap_or(false) {
            let _ = session.record_override_telemetry(&content, ml_data);
        }
    }
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
    let stem = match &source {
        Source::Path(p) => p
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "document".into()),
        Source::MultiPath(_) => "combined".into(),
        Source::Text(_) => "document".into(),
    };
    let mut dialog = app
        .dialog()
        .file()
        .add_filter("JSON", &["json"])
        .set_file_name(format!("{stem}.overrides.json"));
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
    let written =
        lock(&state).compile_folder_with_progress(&dir, &out, &options, |progress, stage| {
            app.emit(
                "compile-progress",
                CompileProgress {
                    progress,
                    stage: stage.to_string(),
                },
            )
            .ok();
        })?;
    app.emit(
        "compile-progress",
        CompileProgress {
            progress: 1.0,
            stage: "Complete".to_string(),
        },
    )
    .ok();
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

/// The decision model's settings; never the key itself.
#[tauri::command]
async fn get_decision_model(
    state: State<'_, SharedDecision>,
) -> Result<decision::DecisionView, String> {
    Ok(lock_decision(&state).view())
}

/// Saves the settings and changes the key as asked.
#[tauri::command]
async fn set_decision_model(
    app: AppHandle,
    state: State<'_, SharedDecision>,
    settings: decision::DecisionSettings,
    key: decision::KeyChange,
) -> Result<decision::DecisionView, String> {
    let mut decision = lock_decision(&state);
    decision.update(settings, key)?;
    decision.save(&data_dir(&app)?)?;
    Ok(decision.view())
}

/// Sends one tiny request with the given (unsaved) settings, so a wrong key
/// or address shows up before a whole document is run. A key sent as `keep`
/// is the stored one.
#[tauri::command]
async fn test_decision_model(
    state: State<'_, SharedDecision>,
    settings: decision::DecisionSettings,
    key: decision::KeyChange,
) -> Result<String, String> {
    let trial = lock_decision(&state).trial(settings, key)?;
    let judge = math_judge::ApiJudge::new(trial.api_config()?);
    tauri::async_runtime::spawn_blocking(move || {
        judge
            .test()
            .map(|()| format!("Connected to {}", judge.model_version()))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// What `run_decision_model` reports.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelRun {
    overrides: ScopeOverrides,
    /// Blocks the model reviewed.
    reviewed: usize,
    /// Verdicts that changed a block.
    changed: usize,
    /// Verdicts that had to be asked of the model; the rest came from the cache.
    asked: usize,
}

/// Has the decision model review the open document and makes its verdicts the
/// session's corrections, which the user can then edit like any other.
#[tauri::command]
async fn run_decision_model(
    app: AppHandle,
    state: State<'_, Shared>,
    decision: State<'_, SharedDecision>,
    source: Source,
) -> Result<ModelRun, String> {
    let config = lock_decision(&decision).api_config()?;
    let (markdown, scoring) = {
        let session = lock(&state);
        (session.read(&source)?, session.config().clone())
    };
    let cache = decision::cache_path(&data_dir(&app)?);
    // Network calls run off the UI thread with no lock held.
    let run = tauri::async_runtime::spawn_blocking(move || {
        let version = math_judge::ApiJudge::new(config.clone()).model_version();
        let build =
            || math_judge::CachedJudge::new(math_judge::ApiJudge::new(config.clone()), &version);
        let judge = build().load(&cache).unwrap_or_else(|_| build());
        let (_, overrides, log) = math_core::analyze_with_judge_detailed(
            &markdown,
            &scoring,
            &judge,
            4,
            ScopeOverrides::default(),
        );
        // A failed request is cached as "accept"; keep that out of the cache
        // and out of the corrections.
        if let Some(error) = judge.inner().errors().into_iter().next() {
            return Err(error);
        }
        if let Some(dir) = cache.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = judge.save(&cache);
        let changed = log
            .iter()
            .filter(|v| v.applied && v.verdict != math_core::Verdict::Accept)
            .count();
        Ok(ModelRun {
            overrides,
            reviewed: log.len(),
            changed,
            asked: judge.misses(),
        })
    })
    .await
    .map_err(|e| e.to_string())??;
    lock(&state).set_overrides(run.overrides.clone());
    Ok(run)
}

/// The running app's version (also shown in the About window).
#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Looks for a newer published release; `None` means the app is current.
#[tauri::command]
async fn check_for_update(app: AppHandle) -> Result<Option<update::UpdateInfo>, String> {
    update::check_manual(&app).await
}

/// Downloads and installs the newest release, then restarts the app.
#[tauri::command]
async fn install_update(app: AppHandle) -> Result<(), String> {
    update::install(&app).await
}

fn main() {
    // A development launch starts from a raw executable, so macOS initially
    // assigns it the generic `exec` icon. Initializing AppKit and replacing
    // that icon before Tauri builds its event loop prevents that first flash.
    dock::refresh(None);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            if let Some(recent_path) = id.strip_prefix("recent:") {
                let _ = app.emit("menu-open-recent", recent_path);
            } else {
                let _ = app.emit("menu-action", id);
            }
        })
        .manage(Shared::default())
        .manage(PrintDoc::default())
        .manage(SharedDecision::default())
        .register_uri_scheme_protocol(PREVIEW_SCHEME, |context, request| {
            let app = context.app_handle();
            let html = if request.uri().path() == PRINT_PATH {
                let print = app.state::<PrintDoc>();
                let html = print.0.lock().unwrap_or_else(|p| p.into_inner());
                html.as_bytes().to_vec()
            } else {
                lock(&app.state::<Shared>()).preview().as_bytes().to_vec()
            };
            tauri::http::Response::builder()
                .header("Content-Type", "text/html; charset=utf-8")
                .header("Cache-Control", "no-store")
                .header("X-Content-Type-Options", "nosniff")
                .body(html)
                .expect("static response headers are valid")
        })
        .setup(|app| {
            let saved = load_recent(app.handle());
            lock(&app.state::<Shared>()).set_recent(saved.clone());
            if let Ok(dir) = app.path().app_data_dir() {
                *lock_decision(&app.state::<SharedDecision>()) = decision::Decision::load(&dir);
            }
            #[cfg(target_os = "macos")]
            {
                if let Ok(m) = menu::build_app_menu(app.handle(), &saved) {
                    let _ = app.set_menu(m);
                }
            }
            // Replace the static bundle icon as soon as AppKit is available.
            // Waiting for `RunEvent::Ready` leaves the light bundle icon visible
            // while the application is starting in Dark Mode.
            dock::refresh(None);
            // Dev builds must never replace themselves with a downloaded
            // release, so the automatic check only runs in release builds.
            #[cfg(not(debug_assertions))]
            update::spawn_periodic_check(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            match event {
                WindowEvent::ThemeChanged(theme) => {
                    dock::sync(Some(*theme));
                    theme::sync_os_content_appearance(window, *theme);
                }
                WindowEvent::Focused(_) => {
                    dock::sync(None);
                }
                #[cfg(target_os = "macos")]
                WindowEvent::CloseRequested { api, .. } => {
                    if window.label() == "main" {
                        let _ = window.hide();
                        api.prevent_close();
                    }
                }
                // A dropped Markdown or HTML file is as much the user's choice as one
                // picked in the dialog. The frontend gets the same paths through
                // the `tauri://drag-drop` event and asks for them afterwards.
                WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) => {
                    let state = window.state::<Shared>();
                    let mut session = lock(&state);
                    for path in paths.iter().filter(|p| session::is_supported(p)) {
                        let _ = session.grant(path);
                    }
                    save_recent(window.app_handle(), &session);
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            pick_markdown,
            grant_paths,
            startup_file,
            analyze,
            compile,
            export,
            export_markdown,
            export_mindmap,
            export_pdf,
            fix_markdown,
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
            get_decision_model,
            set_decision_model,
            test_decision_model,
            run_decision_model,
            app_version,
            check_for_update,
            install_update,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build Mathia")
        .run(|_app_handle, event| {
            match event {
                RunEvent::Ready => {
                    // In dev builds on macOS, Tauri sets the light bundle icon
                    // right after `setup` returns (inside `RuntimeRunEvent::Ready`)
                    // before emitting `RunEvent::Ready`, so apply ours here.
                    dock::refresh(None);
                }
                RunEvent::Resumed => {
                    dock::sync(None);
                }
                #[cfg(target_os = "macos")]
                RunEvent::Reopen {
                    has_visible_windows,
                    ..
                } => {
                    if !has_visible_windows
                        && let Some(window) = _app_handle.get_webview_window("main")
                    {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                    dock::refresh(None);
                }
                _ => {}
            }
        });
}
