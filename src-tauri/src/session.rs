//! Everything the commands do, without any Tauri type: the app state and the
//! thin wrappers over `math-core`. Kept separate so it can be unit-tested.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use math_core::{
    AiOptions, AiProvider, BlockType, CustomProvider, Document, HtmlOptions, MathAssets,
    ScopeOverrides, ScoringConfig,
};
use serde::{Deserialize, Serialize};

/// File extensions accepted from a drag and drop (the open dialog filters on
/// the same list).
pub const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown", "mdown", "txt"];
pub const HTML_EXTENSIONS: &[&str] = &["html", "htm"];
pub const SUPPORTED_EXTENSIONS: &[&str] = &["md", "markdown", "mdown", "txt", "html", "htm"];

/// How many recent files are kept.
const RECENT_MAX: usize = 8;

pub fn is_markdown(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        MARKDOWN_EXTENSIONS
            .iter()
            .any(|m| e.eq_ignore_ascii_case(m))
    })
}

pub fn is_html(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| HTML_EXTENSIONS.iter().any(|m| e.eq_ignore_ascii_case(m)))
}

pub fn is_supported(path: &Path) -> bool {
    is_markdown(path) || is_html(path)
}

/// What to compile: a file the user opened, text held by the frontend,
/// or multiple files compiled in a chosen order.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Path(PathBuf),
    Text(String),
    MultiPath(Vec<PathBuf>),
}

impl Source {
    pub fn path(&self) -> Option<&Path> {
        match self {
            Source::Path(path) => Some(path),
            Source::Text(_) => None,
            Source::MultiPath(paths) => paths.first().map(|p| p.as_path()),
        }
    }
}

/// The subset of [`HtmlOptions`] the frontend can set.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct CompileOptions {
    pub title: Option<String>,
    /// Leave LaTeX unrendered.
    pub no_math: bool,
    /// Built-in AI providers to enable, by slug (`anthropic`, `openai`, `gemini`, `openai-compatible`).
    pub ai_providers: Vec<String>,
    /// Custom OpenAI-compatible providers as `[NAME=]URL`.
    pub ai_custom: Vec<String>,
}

impl CompileOptions {
    pub fn html_options(&self) -> Result<HtmlOptions, String> {
        let mut providers: Vec<AiProvider> = Vec::new();
        for slug in &self.ai_providers {
            let provider = AiProvider::from_slug(slug)
                .ok_or_else(|| format!("unknown AI provider '{slug}'"))?;
            if !providers.contains(&provider) {
                providers.push(provider);
            }
        }
        let mut custom_providers: Vec<CustomProvider> = Vec::new();
        for spec in &self.ai_custom {
            let custom = CustomProvider::from_spec(spec)?;
            if !custom_providers.iter().any(|c| c.slug == custom.slug) {
                custom_providers.push(custom);
            }
        }
        Ok(HtmlOptions {
            math: if self.no_math {
                MathAssets::None
            } else {
                MathAssets::bundled()
            },
            title: self.title.clone().filter(|t| !t.trim().is_empty()),
            source_name: None,
            fonts: vec![math_core::EmbeddedFont::bundled()],
            ai: AiOptions {
                providers,
                custom_providers,
            },
        })
    }
}

/// Scores of one node, for the block inspector.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScoreView {
    pub best: BlockType,
    pub score: f32,
    pub opens: bool,
    /// Every semantic type with its score.
    pub scores: Vec<(BlockType, f32)>,
}

/// `math_core::Analysis` in a serialisable shape.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AnalysisView {
    pub document: Document,
    /// One entry per node.
    pub scores: Vec<ScoreView>,
    /// For every node, the index in `document.blocks` of its block.
    pub block_of: Vec<usize>,
    pub threshold: f32,
}

/// What `compile` reports back; the HTML itself is served to the preview
/// frame by the `mathia-preview` protocol instead of crossing the IPC bridge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Compiled {
    /// Bumped whenever the preview HTML changes.
    pub revision: u64,
    pub changed: bool,
    pub semantic_blocks: usize,
    pub bytes: usize,
}

/// The app state: one scoring configuration, the hand corrections for the
/// open document, the files the user has opened, and the HTML currently
/// shown in the preview.
#[derive(Debug, Default)]
pub struct Session {
    config: ScoringConfig,
    /// Corrections made in the inspector, keyed by node index of the open
    /// document. The frontend clears them when it opens another file.
    overrides: ScopeOverrides,
    /// Canonical paths the user chose in an open dialog or dropped on the
    /// window. Commands read nothing else from disk.
    granted: HashSet<PathBuf>,
    /// Files opened before, most recent first. Only these may be re-opened
    /// without a dialog (see [`Session::open_recent`]).
    recent: Vec<PathBuf>,
    preview: String,
    revision: u64,
    current_content: Mutex<Option<String>>,
}

impl Session {
    /// Records that the user picked `path`; returns its canonical form, which
    /// is what the frontend should pass back in [`Source::Path`].
    pub fn grant(&mut self, path: &Path) -> Result<PathBuf, String> {
        let canonical = path
            .canonicalize()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if !canonical.is_file() {
            return Err(format!("{}: not a file", path.display()));
        }
        self.granted.insert(canonical.clone());
        self.recent.retain(|p| p != &canonical);
        self.recent.insert(0, canonical.clone());
        self.recent.truncate(RECENT_MAX);
        Ok(canonical)
    }

    /// Replaces the recent list with one saved by an earlier run.
    pub fn set_recent(&mut self, paths: Vec<PathBuf>) {
        self.recent = paths;
        self.recent.truncate(RECENT_MAX);
    }

    /// The recent files that still exist, most recent first.
    pub fn recent(&self) -> Vec<PathBuf> {
        self.recent
            .iter()
            .filter(|p| p.is_file())
            .cloned()
            .collect()
    }

    /// Opens a file from the recent list. A path that is not in the list is
    /// refused, so the frontend cannot use this to read arbitrary files. A
    /// file that has gone is dropped from the list.
    pub fn open_recent(&mut self, path: &Path) -> Result<PathBuf, String> {
        if !self.recent.iter().any(|p| p == path) {
            return Err(format!("{}: not a recent file", path.display()));
        }
        self.grant(path)
            .inspect_err(|_| self.recent.retain(|p| p != path))
    }

    pub fn clear_recent(&mut self) {
        self.recent.clear();
    }

    pub fn read(&self, source: &Source) -> Result<String, String> {
        match source {
            Source::Text(text) => Ok(text.clone()),
            Source::Path(path) => {
                let canonical = path
                    .canonicalize()
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                if !self.granted.contains(&canonical) {
                    return Err(format!(
                        "{}: not opened in Mathia (use Open or drop the file on the window)",
                        path.display()
                    ));
                }
                std::fs::read_to_string(&canonical).map_err(|e| format!("{}: {e}", path.display()))
            }
            Source::MultiPath(paths) => {
                if paths.is_empty() {
                    return Err("no files to read".to_string());
                }
                let mut combined = String::new();
                for (i, path) in paths.iter().enumerate() {
                    let canonical = path
                        .canonicalize()
                        .map_err(|e| format!("{}: {e}", path.display()))?;
                    if !self.granted.contains(&canonical) {
                        return Err(format!(
                            "{}: not opened in Mathia (use Open or drop the file on the window)",
                            path.display()
                        ));
                    }
                    let content = std::fs::read_to_string(&canonical)
                        .map_err(|e| format!("{}: {e}", path.display()))?;
                    let content_trimmed = content.trim_end_matches(['\r', '\n']);
                    if i > 0 {
                        combined.push_str("\n\n");
                    }
                    let content_clean = content_trimmed.trim_start_matches(['\r', '\n']);
                    combined.push_str(content_clean);
                }
                if !combined.is_empty() && !combined.ends_with('\n') {
                    combined.push('\n');
                }
                Ok(combined)
            }
        }
    }

    pub fn is_html_source(&self, source: &Source) -> bool {
        match source {
            Source::Path(path) => is_html(path),
            Source::Text(text) => math_core::is_html(text),
            Source::MultiPath(paths) => paths.iter().any(|p| is_html(p)),
        }
    }

    /// The source with its Markdown and LaTeX repaired, for saving.
    pub fn repair(&self, source: &Source) -> Result<math_core::Repair, String> {
        let content = self.read(source)?;
        if self.is_html_source(source) || math_core::is_html(&content) {
            return Ok(math_core::Repair {
                text: content,
                changed: false,
                formulas: 0,
            });
        }
        Ok(math_core::repair_markdown(&content))
    }

    /// Classifies with the session's configuration and corrections.
    fn classify(&self, source: &Source) -> Result<math_core::Analysis, String> {
        let content = self.read(source)?;
        self.classify_content(source, &content)
    }

    fn classify_content(
        &self,
        source: &Source,
        content: &str,
    ) -> Result<math_core::Analysis, String> {
        if let Ok(mut lock) = self.current_content.lock() {
            *lock = Some(content.to_string());
        }
        if self.is_html_source(source) || math_core::is_html(content) {
            let mut analysis = math_core::clean_html::analyze_html_detailed(content);
            math_core::clean_html::apply_overrides(&mut analysis.document, &self.overrides);
            math_core::clean_html::reindex(&mut analysis);
            return Ok(analysis);
        }
        let analysis = math_core::analyze_with_overrides(content, &self.config, &self.overrides);
        self.overrides
            .validate(analysis.document.nodes.len())
            .map_err(|e| format!("the corrections no longer fit this file ({e}); reset them"))?;
        Ok(analysis)
    }

    /// The open document as Markdown, with every block labelled the way
    /// Mathia detected it. HTML sources are converted; Markdown sources keep
    /// their own text.
    pub fn markdown(&self, source: &Source) -> Result<String, String> {
        let content = self.read(source)?;
        let analysis = self.classify_content(source, &content)?;
        if analysis.document.nodes.is_empty() {
            return Ok(math_core::markdown::render(&analysis.document, None));
        }
        let normalized = math_core::normalize_markdown(&content);
        Ok(math_core::markdown::render(
            &analysis.document,
            Some(&normalized),
        ))
    }

    /// Modification time and size of opened file(s), as an opaque string
    /// that changes when any file does; `None` for text sources.
    pub fn stamp(&self, source: &Source) -> Result<Option<String>, String> {
        match source {
            Source::Text(_) => Ok(None),
            Source::Path(path) => {
                let canonical = path
                    .canonicalize()
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                if !self.granted.contains(&canonical) {
                    return Err(format!("{}: not opened in Mathia", path.display()));
                }
                let meta = std::fs::metadata(&canonical)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_nanos());
                Ok(Some(format!("{modified}:{}", meta.len())))
            }
            Source::MultiPath(paths) => {
                let mut stamps = Vec::with_capacity(paths.len());
                for path in paths {
                    let canonical = path
                        .canonicalize()
                        .map_err(|e| format!("{}: {e}", path.display()))?;
                    if !self.granted.contains(&canonical) {
                        return Err(format!("{}: not opened in Mathia", path.display()));
                    }
                    let meta = std::fs::metadata(&canonical)
                        .map_err(|e| format!("{}: {e}", path.display()))?;
                    let modified = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_nanos());
                    stamps.push(format!("{modified}:{}", meta.len()));
                }
                Ok(Some(stamps.join(";")))
            }
        }
    }

    pub fn overrides(&self) -> &ScopeOverrides {
        &self.overrides
    }

    pub fn set_overrides(&mut self, overrides: ScopeOverrides) {
        self.overrides = overrides;
    }

    pub fn current_content(&self) -> Option<String> {
        self.current_content.lock().ok().and_then(|c| c.clone())
    }

    pub fn record_override_telemetry(
        &self,
        content: &str,
        telemetry_path: &Path,
    ) -> Result<bool, String> {
        if !self.config.ml_telemetry || self.overrides.is_empty() {
            return Ok(false);
        }
        if let Some(entry) =
            math_core::create_override_telemetry(content, &self.config, &self.overrides)
        {
            math_core::append_telemetry_entry(telemetry_path, &entry)
                .map_err(|e| format!("failed to record telemetry: {e}"))?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Compiles every Markdown file directly inside `dir` (not recursive)
    /// into `out_dir`, with the session's configuration but without the
    /// corrections, which belong to one document. Returns the files written.
    #[allow(dead_code)]
    pub fn compile_folder(
        &self,
        dir: &Path,
        out_dir: &Path,
        options: &CompileOptions,
    ) -> Result<Vec<PathBuf>, String> {
        self.compile_folder_with_progress(dir, out_dir, options, |_, _| {})
    }

    pub fn compile_folder_with_progress<F>(
        &self,
        dir: &Path,
        out_dir: &Path,
        options: &CompileOptions,
        mut on_progress: F,
    ) -> Result<Vec<PathBuf>, String>
    where
        F: FnMut(f32, &str),
    {
        let mut html_options = options.html_options()?;
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| path.is_file() && is_markdown(path))
            .collect();
        files.sort();
        let total = files.len();
        let mut written = Vec::new();
        for (i, file) in files.iter().enumerate() {
            let file_name = file
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "document".to_string());
            let pct = if total > 0 {
                i as f32 / total as f32
            } else {
                0.0
            };
            on_progress(
                pct,
                &format!("Compiling {} of {}: {}", i + 1, total, file_name),
            );
            html_options.source_name = Some(file_name);
            let markdown =
                std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
            let document = math_core::analyze(&markdown, &self.config);
            let html =
                math_core::html::render(&document, &html_options).map_err(|e| e.to_string())?;
            let name = file.with_extension("html");
            let dest = out_dir.join(name.file_name().expect("a file has a name"));
            std::fs::write(&dest, html).map_err(|e| format!("{}: {e}", dest.display()))?;
            written.push(dest);
        }
        on_progress(1.0, "Folder compilation complete");
        Ok(written)
    }

    pub fn analyze(&self, source: &Source) -> Result<AnalysisView, String> {
        let analysis = self.classify(source)?;
        Ok(AnalysisView {
            document: analysis.document,
            scores: analysis
                .scores
                .into_iter()
                .map(|s| ScoreView {
                    best: s.best.0,
                    score: s.best.1,
                    opens: s.opens,
                    scores: s.scores,
                })
                .collect(),
            block_of: analysis.block_of,
            threshold: self.config.threshold,
        })
    }

    /// Markdown or HTML to standalone HTML with the session's scoring configuration.
    /// Returns the HTML and the number of semantic blocks.
    pub fn compile(
        &self,
        source: &Source,
        options: &CompileOptions,
    ) -> Result<(String, usize), String> {
        self.compile_with_progress(source, options, |_, _| {})
    }

    pub fn compile_with_progress<F>(
        &self,
        source: &Source,
        options: &CompileOptions,
        mut on_progress: F,
    ) -> Result<(String, usize), String>
    where
        F: FnMut(f32, &str),
    {
        on_progress(0.1, "Reading document…");
        let content = self.read(source)?;
        let mut html_options = options.html_options()?;
        html_options.source_name = match source {
            Source::Path(path) => path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            _ => None,
        };
        if self.is_html_source(source) || math_core::is_html(&content) {
            on_progress(0.3, "Extracting and cleaning HTML…");
            let mut analysis = math_core::clean_html::analyze_html_detailed(&content);
            on_progress(0.6, "Applying corrections…");
            math_core::clean_html::apply_overrides(&mut analysis.document, &self.overrides);
            let semantic = analysis
                .document
                .blocks
                .iter()
                .filter(|b| b.kind.is_semantic())
                .count();
            on_progress(0.8, "Compiling standalone HTML…");
            let html = math_core::html::render(&analysis.document, &html_options)
                .map_err(|e| e.to_string())?;
            on_progress(0.95, "HTML compiled");
            return Ok((html, semantic));
        }
        on_progress(0.3, "Parsing Markdown content…");
        let document = self.classify(source)?.document;
        on_progress(0.6, "Classifying semantic blocks…");
        let semantic = document
            .blocks
            .iter()
            .filter(|b| b.kind.is_semantic())
            .count();
        on_progress(0.8, "Compiling standalone HTML…");
        let html = math_core::html::render(&document, &html_options).map_err(|e| e.to_string())?;
        on_progress(0.95, "HTML compiled");
        Ok((html, semantic))
    }

    /// Compiles and makes the result the preview.
    #[allow(dead_code)]
    pub fn compile_preview(
        &mut self,
        source: &Source,
        options: &CompileOptions,
    ) -> Result<Compiled, String> {
        self.compile_preview_with_progress(source, options, |_, _| {})
    }

    pub fn compile_preview_with_progress<F>(
        &mut self,
        source: &Source,
        options: &CompileOptions,
        progress: F,
    ) -> Result<Compiled, String>
    where
        F: FnMut(f32, &str),
    {
        let (html, semantic_blocks) = self.compile_with_progress(source, options, progress)?;
        let changed = html != self.preview;
        if changed {
            self.preview = html;
            self.revision += 1;
        }
        Ok(Compiled {
            revision: self.revision,
            changed,
            semantic_blocks,
            bytes: self.preview.len(),
        })
    }

    pub fn preview(&self) -> &str {
        &self.preview
    }

    pub fn config(&self) -> &ScoringConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: ScoringConfig) -> Result<(), String> {
        config.validate().map_err(|e| e.to_string())?;
        self.config = config;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# Groups\n\n**Theorem 1.** Every group of prime order is cyclic.\n\n\
        *Proof.* Take any $g \\neq e$; its order divides $p$. $\\blacksquare$\n";

    fn text() -> Source {
        Source::Text(SAMPLE.to_string())
    }

    fn temp_file(name: &str, content: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mathia-app-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn analyze_reports_blocks_and_one_score_per_node() {
        let view = Session::default().analyze(&text()).unwrap();
        assert_eq!(view.scores.len(), view.document.nodes.len());
        assert_eq!(view.block_of.len(), view.document.nodes.len());
        let kinds: Vec<_> = view.document.blocks.iter().map(|b| b.kind).collect();
        assert!(kinds.contains(&BlockType::Theorem), "{kinds:?}");
        assert!(kinds.contains(&BlockType::Proof), "{kinds:?}");
        // The shape the frontend reads.
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["scores"][1]["best"], "theorem");
        assert!(json["document"]["blocks"][1]["id"].is_string());
    }

    #[test]
    fn recent_files_are_ordered_unique_and_the_only_ones_reopenable() {
        let a = temp_file("recent-a.md", SAMPLE);
        let b = temp_file("recent-b.md", SAMPLE);
        let mut session = Session::default();
        let a = session.grant(&a).unwrap();
        let b = session.grant(&b).unwrap();
        session.grant(&a).unwrap();
        assert_eq!(session.recent(), vec![a.clone(), b.clone()]);

        // A fresh session (a new run) can reopen a saved recent file...
        let mut next = Session::default();
        next.set_recent(vec![b.clone()]);
        assert_eq!(next.open_recent(&b).unwrap(), b);
        // ...but not one the user never opened.
        assert!(next.open_recent(&a).is_err());

        next.clear_recent();
        assert!(next.recent().is_empty());
    }

    #[test]
    fn paths_must_be_granted_first() {
        let path = temp_file("granted.md", SAMPLE);
        let mut session = Session::default();
        let source = Source::Path(path.clone());
        let refused = session.analyze(&source).unwrap_err();
        assert!(refused.contains("not opened in Mathia"), "{refused}");

        let canonical = session.grant(&path).unwrap();
        assert!(session.analyze(&source).is_ok());
        assert!(session.analyze(&Source::Path(canonical)).is_ok());
        // A sibling that was never picked stays unreadable.
        let other = temp_file("other.md", SAMPLE);
        assert!(session.analyze(&Source::Path(other)).is_err());
        assert!(session.grant(&path.with_file_name("missing.md")).is_err());
        assert!(session.grant(path.parent().unwrap()).is_err());
    }

    #[test]
    fn preview_revision_moves_only_when_the_html_changes() {
        let mut session = Session::default();
        let options = CompileOptions::default();
        let first = session.compile_preview(&text(), &options).unwrap();
        assert_eq!((first.revision, first.changed), (1, true));
        assert_eq!(first.semantic_blocks, 2);
        assert_eq!(first.bytes, session.preview().len());
        assert!(session.preview().contains("block-theorem-1"));

        let again = session.compile_preview(&text(), &options).unwrap();
        assert_eq!((again.revision, again.changed), (1, false));

        let titled = CompileOptions {
            title: Some("Other".into()),
            ..CompileOptions::default()
        };
        let third = session.compile_preview(&text(), &titled).unwrap();
        assert_eq!((third.revision, third.changed), (2, true));
    }

    #[test]
    fn markdown_export_keeps_the_text_and_labels_every_block() {
        let session = Session::default();
        let exported = session.markdown(&text()).unwrap();
        assert!(exported.contains("**Theorem 1.** Every group of prime order is cyclic."));
        assert!(exported.contains("$g \\neq e$"));
        // The corrections shown in the inspector reach the export.
        let mut corrected = Session::default();
        let mut overrides = ScopeOverrides::default();
        overrides.retype.insert(1.into(), BlockType::Lemma);
        corrected.set_overrides(overrides);
        assert!(
            corrected
                .markdown(&text())
                .unwrap()
                .contains("**Lemma 1.**")
        );
    }

    #[test]
    fn markdown_export_converts_an_html_source() {
        let html = "<html><body><h1>Groups</h1>\
            <section class=\"mx-block mx-theorem\"><p><strong>Theorem 1.</strong> \
            Every group of prime order is cyclic.</p></section></body></html>";
        let exported = Session::default()
            .markdown(&Source::Text(html.to_string()))
            .unwrap();
        assert!(exported.contains("# Groups"), "{exported}");
        assert!(
            exported.contains("**Theorem 1.** Every group of prime order is cyclic."),
            "{exported}"
        );
    }

    #[test]
    fn options_map_onto_html_options() {
        let options: CompileOptions = serde_json::from_str(
            r#"{"noMath": true, "aiProviders": ["anthropic", "Anthropic"],
                "aiCustom": ["ollama=http://localhost:11434/v1"]}"#,
        )
        .unwrap();
        let html = options.html_options().unwrap();
        assert_eq!(html.math, MathAssets::None);
        assert_eq!(html.ai.providers, vec![AiProvider::Anthropic]);
        assert_eq!(html.ai.custom_providers[0].slug, "ollama");

        let unknown = CompileOptions {
            ai_providers: vec!["nope".into()],
            ..CompileOptions::default()
        };
        assert!(unknown.html_options().is_err());
        assert!(serde_json::from_str::<CompileOptions>(r#"{"katexDir": "/"}"#).is_err());
        // Without AI options the document allows no connections at all.
        let (plain, _) = Session::default()
            .compile(&text(), &CompileOptions::default())
            .unwrap();
        assert!(!plain.contains("connect-src"));
    }

    #[test]
    fn source_json_and_invalid_config() {
        let source: Source = serde_json::from_str(r#"{"path": "/tmp/a.md"}"#).unwrap();
        assert_eq!(source.path(), Some(Path::new("/tmp/a.md")));
        assert!(serde_json::from_str::<Source>(r#"{"text": "x"}"#).is_ok());

        let mut session = Session::default();
        let config = ScoringConfig {
            threshold: 7.0,
            ..ScoringConfig::default()
        };
        assert!(session.set_config(config).is_err());
        assert_eq!(session.config(), &ScoringConfig::default());

        assert!(is_markdown(Path::new("notes.MD")));
        assert!(!is_markdown(Path::new("notes.html")));
    }

    #[test]
    fn corrections_apply_to_analysis_and_preview() {
        let mut session = Session::default();
        let mut overrides = ScopeOverrides::default();
        overrides.retype.insert(1.into(), BlockType::Lemma);
        session.set_overrides(overrides);
        let view = session.analyze(&text()).unwrap();
        assert_eq!(view.document.blocks[1].kind, BlockType::Lemma);
        let (html, _) = session
            .compile(&text(), &CompileOptions::default())
            .unwrap();
        assert!(html.contains("block-lemma-1"));

        // Overrides that point past the document are reported, not ignored.
        let mut stale = ScopeOverrides::default();
        stale.suppressed.insert(99.into());
        session.set_overrides(stale);
        assert!(session.analyze(&text()).unwrap_err().contains("reset them"));
        // The frontend sends them as JSON.
        let parsed: ScopeOverrides =
            serde_json::from_str(r#"{"retype": {"1": "lemma"}, "end": {"1": 3}}"#).unwrap();
        assert_eq!(parsed.end.get(&1.into()), Some(&3));
    }

    #[test]
    fn html_corrections_apply_to_analysis_and_preview() {
        let html = "<html><body><main>\
            <h1>Groups</h1>\
            <p><strong>Theorem 1.</strong> Every group of prime order is cyclic.</p>\
            <p>This proof follows from Lagrange's theorem.</p>\
            </main></body></html>";
        let source = Source::Text(html.to_string());
        let mut session = Session::default();

        let before = session.analyze(&source).unwrap();
        let theorem = before
            .document
            .blocks
            .iter()
            .find(|b| b.kind == BlockType::Theorem)
            .expect("theorem detected");
        assert_eq!(theorem.span.end - theorem.span.start, 1);
        let start = theorem.span.start;

        let mut overrides = ScopeOverrides::default();
        overrides.end.insert(start.into(), start + 2);
        session.set_overrides(overrides);

        let after = session.analyze(&source).unwrap();
        let theorem = after
            .document
            .blocks
            .iter()
            .find(|b| b.kind == BlockType::Theorem)
            .expect("theorem survives");
        assert_eq!(theorem.span.end - theorem.span.start, 2);
        assert!(theorem.html.contains("This proof follows"));

        let (compiled, _) = session
            .compile(&source, &CompileOptions::default())
            .unwrap();
        assert!(compiled.contains("This proof follows"));
    }

    #[test]
    fn repair_wraps_bare_formulas_of_an_opened_file() {
        let path = temp_file("bare.md", "اگر P(B) \\neq 0 آن‌گاه $x$ است.\n");
        let mut session = Session::default();
        let source = Source::Path(path.clone());
        assert!(session.repair(&source).is_err());
        session.grant(&path).unwrap();
        let repair = session.repair(&source).unwrap();
        assert_eq!(repair.formulas, 1);
        assert_eq!(repair.text, "اگر $P(B) \\neq 0$ آن‌گاه $x$ است.\n");
        assert!(!session.repair(&Source::Text(repair.text)).unwrap().changed);
    }

    #[test]
    fn stamps_change_with_the_file_and_folders_compile() {
        let path = temp_file("stamped.md", SAMPLE);
        let mut session = Session::default();
        let source = Source::Path(path.clone());
        assert!(session.stamp(&source).is_err());
        session.grant(&path).unwrap();
        let first = session.stamp(&source).unwrap();
        std::fs::write(&path, format!("{SAMPLE}\nMore text.\n")).unwrap();
        assert_ne!(session.stamp(&source).unwrap(), first);
        assert_eq!(session.stamp(&text()).unwrap(), None);

        let dir = path.parent().unwrap().join("folder");
        let out = path.parent().unwrap().join("folder-out");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(dir.join("a.md"), SAMPLE).unwrap();
        std::fs::write(dir.join("b.markdown"), SAMPLE).unwrap();
        std::fs::write(dir.join("skip.html"), "x").unwrap();
        let written = session
            .compile_folder(
                &dir,
                &out,
                &CompileOptions {
                    no_math: true,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(written, [out.join("a.html"), out.join("b.html")]);
        assert!(
            std::fs::read_to_string(&written[0])
                .unwrap()
                .contains("block-theorem-1")
        );
    }

    #[test]
    fn html_input_is_cleaned_and_compiled() {
        let dirty_html = r#"<!DOCTYPE html>
<html>
<head>
  <title>Physics Guide</title>
  <link rel="stylesheet" href="bad.css">
  <style>body { font-size: 50px; }</style>
  <script src="bad.js"></script>
</head>
<body onload="alert(1)">
  <h1>Classical Mechanics</h1>
  <p>Newton's second law: $F = ma$.</p>
  <script>console.log("bad");</script>
</body>
</html>"#;

        let source = Source::Text(dirty_html.to_string());
        let session = Session::default();
        let (compiled, semantic) = session
            .compile(&source, &CompileOptions::default())
            .unwrap();

        // Bloat removed
        assert!(!compiled.contains("bad.css"));
        assert!(!compiled.contains("font-size: 50px"));
        assert!(!compiled.contains("bad.js"));
        assert!(!compiled.contains("onload="));
        assert!(!compiled.contains("console.log(\"bad\")"));

        // New Mathia scripts and styles present
        assert!(compiled.contains("<style id=\"mx-style\">"));
        assert!(compiled.contains("<script id=\"mx-runtime\">"));
        assert!(compiled.contains("Classical Mechanics"));
        assert!(compiled.contains("Newton's second law"));
        assert_eq!(semantic, 0); // Headings are not counted as semantic theorems

        let analysis = session.analyze(&source).unwrap();
        assert_eq!(analysis.document.title.as_deref(), Some("Physics Guide"));
        let kinds: Vec<_> = analysis.document.blocks.iter().map(|b| b.kind).collect();
        assert_eq!(kinds, [BlockType::Heading, BlockType::Text]);
    }

    #[test]
    fn multi_path_reads_in_chosen_order_and_compiles() {
        let f1 = temp_file("ch1.md", "# Chapter 1\n\n**Theorem 1.** Statement one.\n");
        let f2 = temp_file("ch2.md", "# Chapter 2\n\n**Definition 1.** Term two.\n");
        let f3 = temp_file("ch3.md", "# Chapter 3\n\n**Proposition 1.** Prop three.\n");

        let mut session = Session::default();
        let f1 = session.grant(&f1).unwrap();
        let f2 = session.grant(&f2).unwrap();
        let f3 = session.grant(&f3).unwrap();

        // Reading in order 1, 2, 3
        let source_123 = Source::MultiPath(vec![f1.clone(), f2.clone(), f3.clone()]);
        let text_123 = session.read(&source_123).unwrap();
        assert_eq!(
            text_123,
            "# Chapter 1\n\n**Theorem 1.** Statement one.\n\n# Chapter 2\n\n**Definition 1.** Term two.\n\n# Chapter 3\n\n**Proposition 1.** Prop three.\n"
        );

        // Reading in custom reordered sequence 3, 1, 2
        let source_312 = Source::MultiPath(vec![f3.clone(), f1.clone(), f2.clone()]);
        let text_312 = session.read(&source_312).unwrap();
        assert_eq!(
            text_312,
            "# Chapter 3\n\n**Proposition 1.** Prop three.\n\n# Chapter 1\n\n**Theorem 1.** Statement one.\n\n# Chapter 2\n\n**Definition 1.** Term two.\n"
        );

        // Analyze finds semantic blocks from all compiled files
        let analysis = session.analyze(&source_123).unwrap();
        let kinds: Vec<_> = analysis.document.blocks.iter().map(|b| b.kind).collect();
        assert!(kinds.contains(&BlockType::Theorem));
        assert!(kinds.contains(&BlockType::Definition));
        assert!(kinds.contains(&BlockType::Proposition));

        // Compile preview renders all chapters together
        let compiled = session
            .compile_preview(&source_123, &CompileOptions::default())
            .unwrap();
        assert!(compiled.semantic_blocks >= 3);
        assert!(session.preview().contains("Chapter 1"));
        assert!(session.preview().contains("Chapter 2"));
        assert!(session.preview().contains("Chapter 3"));
    }

    #[test]
    fn multi_path_requires_all_files_granted_and_tracks_stamps() {
        let f1 = temp_file("part1.md", "# Part 1\n\nSome text.\n");
        let f2 = temp_file("part2.md", "# Part 2\n\nOther text.\n");

        let mut session = Session::default();
        let source = Source::MultiPath(vec![f1.clone(), f2.clone()]);

        // Neither granted -> error
        assert!(session.read(&source).is_err());
        assert!(session.stamp(&source).is_err());

        // Only first granted -> error
        session.grant(&f1).unwrap();
        assert!(session.read(&source).is_err());

        // Both granted -> succeeds
        session.grant(&f2).unwrap();
        assert!(session.read(&source).is_ok());

        let stamp1 = session.stamp(&source).unwrap().unwrap();

        // Modifying any one of the files changes the stamp
        std::fs::write(&f2, "# Part 2\n\nUpdated text.\n").unwrap();
        let stamp2 = session.stamp(&source).unwrap().unwrap();
        assert_ne!(stamp1, stamp2);
    }

    #[test]
    fn multi_path_json_serde() {
        let json = r#"{"multi_path": ["/tmp/a.md", "/tmp/b.md"]}"#;
        let source: Source = serde_json::from_str(json).unwrap();
        assert_eq!(
            source,
            Source::MultiPath(vec![PathBuf::from("/tmp/a.md"), PathBuf::from("/tmp/b.md")])
        );
        let serialized = serde_json::to_string(&source).unwrap();
        assert!(serialized.contains(r#""multi_path":["#));
    }

    #[test]
    fn compile_with_progress_reports_stages() {
        let f_md = temp_file("test.md", "# Heading\n\n**Theorem 1.** Let $x > 0$.\n");
        let f_html = temp_file(
            "test.html",
            "<h1>Title</h1><p><strong>Theorem 1</strong> Mathia</p>",
        );

        let mut session = Session::default();
        session.grant(&f_md).unwrap();
        session.grant(&f_html).unwrap();

        let mut md_events = Vec::new();
        let source_md = Source::Path(f_md);
        let res_md = session.compile_preview_with_progress(
            &source_md,
            &CompileOptions::default(),
            |progress, stage| {
                md_events.push((progress, stage.to_string()));
            },
        );
        assert!(res_md.is_ok());
        assert!(!md_events.is_empty());
        assert!(md_events.first().unwrap().0 <= 0.15);
        assert!(md_events.last().unwrap().0 >= 0.9);

        let mut html_events = Vec::new();
        let source_html = Source::Path(f_html);
        let res_html = session.compile_with_progress(
            &source_html,
            &CompileOptions::default(),
            |progress, stage| {
                html_events.push((progress, stage.to_string()));
            },
        );
        assert!(res_html.is_ok());
        assert!(!html_events.is_empty());
        assert!(html_events.first().unwrap().0 <= 0.15);
        assert!(html_events.last().unwrap().0 >= 0.9);
    }
}
