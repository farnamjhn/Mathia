//! Everything the commands do, without any Tauri type: the app state and the
//! thin wrappers over `math-core`. Kept separate so it can be unit-tested.

use std::collections::HashSet;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use math_core::{
    AiOptions, AiProvider, BlockType, CustomProvider, Document, HtmlOptions, MathAssets,
    ScopeOverrides, ScoringConfig, TaggedNode,
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
            ..HtmlOptions::default()
        })
    }
}

/// BIO tag and confidence of one node, for the block inspector.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TagView {
    /// BIO label, e.g. `O`, `B-THEOREM`, `I-PROOF`.
    pub label: String,
    pub kind: Option<BlockType>,
    pub confidence: f32,
}

fn tag_view(tag: TaggedNode) -> TagView {
    TagView {
        label: tag.tag.to_label(),
        kind: tag.tag.block_type(),
        confidence: tag.confidence,
    }
}

/// `math_core::Analysis` in a serialisable shape.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AnalysisView {
    pub document: Document,
    /// One entry per node.
    pub tags: Vec<TagView>,
    /// For every node, the index in `document.blocks` of its block.
    pub block_of: Vec<usize>,
    /// Confidence below which a block is considered ambiguous for the
    /// currently selected tagger.
    pub confidence_threshold: f32,
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

/// One edited block sent from the Live Preview editor.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockEdit {
    pub id: String,
    pub md: String,
    #[serde(default)]
    pub file_index: usize,
    #[serde(default)]
    pub edited: bool,
}

/// A disk file snapshot used for unified Undo/Redo across live edits.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSnapshot {
    pub path: PathBuf,
    pub content: String,
}

/// Updated outer HTML for a single block so the desktop iframe can patch
/// non-focused blocks without reloading and losing the active cursor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderedBlockPatch {
    pub id: String,
    pub outer_html: String,
}

/// Response returned after persisting Live Preview edits and recompiling.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveEditResult {
    pub revision: u64,
    pub changed: bool,
    pub stamp: Option<String>,
    pub semantic_blocks: usize,
    pub bytes: usize,
    pub analysis: AnalysisView,
    pub source_data: math_core::SourcePayload,
    pub toc: Vec<math_core::TocEntry>,
    pub mindmap_json: String,
    pub blocks_patch: Vec<RenderedBlockPatch>,
    pub before_files: Vec<FileSnapshot>,
    pub after_files: Vec<FileSnapshot>,
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

    fn read_source_files(&self, source: &Source) -> Result<Vec<(PathBuf, String)>, String> {
        match source {
            Source::Text(_) => Err("text source has no backing file".to_string()),
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
                let content = std::fs::read_to_string(&canonical)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                Ok(vec![(canonical, content)])
            }
            Source::MultiPath(paths) => {
                if paths.is_empty() {
                    return Err("no files to read".to_string());
                }
                let mut files = Vec::with_capacity(paths.len());
                for path in paths {
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
                    files.push((canonical, content));
                }
                Ok(files)
            }
        }
    }

    /// Combines Markdown files into a normalised buffer and returns the byte
    /// range belonging to each file in order.
    fn normalized_markdown_with_segments(
        &self,
        source: &Source,
    ) -> Result<(String, Vec<Range<usize>>), String> {
        match source {
            Source::Text(text) => Ok((math_core::normalize_markdown(text), Vec::new())),
            Source::Path(_) => {
                let raw = self.read(source)?;
                Ok((math_core::normalize_markdown(&raw), Vec::new()))
            }
            Source::MultiPath(_) => {
                let files = self.read_source_files(source)?;
                Ok(combine_normalized_files(&files))
            }
        }
    }

    pub fn read(&self, source: &Source) -> Result<String, String> {
        match source {
            Source::Text(text) => Ok(text.clone()),
            Source::Path(_) => Ok(self.read_source_files(source)?.remove(0).1),
            Source::MultiPath(_) => {
                let files = self.read_source_files(source)?;
                let mut combined = String::new();
                for (i, (_, content)) in files.iter().enumerate() {
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
            html_options.source_markdown = Some(math_core::normalize_markdown(&markdown));
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
        let confidence_threshold = match self.config.tagger {
            math_core::TaggerModel::Crf => self.config.ml_confidence_threshold,
            math_core::TaggerModel::WeightedSum => self.config.threshold,
        };
        Ok(AnalysisView {
            document: analysis.document,
            tags: analysis.tags.into_iter().map(tag_view).collect(),
            block_of: analysis.block_of,
            confidence_threshold,
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
        let (norm_source, segments) = self.normalized_markdown_with_segments(source)?;
        let document = self.classify(source)?.document;
        on_progress(0.6, "Classifying semantic blocks…");
        let semantic = document
            .blocks
            .iter()
            .filter(|b| b.kind.is_semantic())
            .count();
        on_progress(0.8, "Compiling standalone HTML…");
        html_options.source_markdown = Some(norm_source);
        html_options.source_segments = segments;
        let html = math_core::html::render(&document, &html_options).map_err(|e| e.to_string())?;
        on_progress(0.95, "HTML compiled");
        Ok((html, semantic))
    }

    /// Saves edits made in the desktop Live Preview back to the source file(s),
    /// updates the compiled preview in place, and returns synchronized block
    /// metadata plus file snapshots for unified Undo/Redo.
    pub fn apply_live_edit(
        &mut self,
        source: &Source,
        edits: Vec<BlockEdit>,
        options: &CompileOptions,
    ) -> Result<LiveEditResult, String> {
        let mut before_files = Vec::new();
        let mut after_files = Vec::new();

        match source {
            Source::Text(_) => {
                return Err("in-memory text sources cannot be written to disk".to_string());
            }
            Source::Path(_) | Source::MultiPath(_) => {
                let files = self.read_source_files(source)?;
                let is_html_doc = self.is_html_source(source)
                    || files.first().is_some_and(|(_, c)| math_core::is_html(c));

                if is_html_doc {
                    let (path, before_html) = &files[0];
                    let mut analysis = math_core::clean_html::analyze_html_detailed(before_html);
                    math_core::clean_html::apply_overrides(&mut analysis.document, &self.overrides);
                    let updated_doc = rebuild_html_document(
                        &analysis.document,
                        &edits,
                        &self.config,
                        options.title.as_deref(),
                    );
                    let mut html_options = options.html_options()?;
                    html_options.source_name = path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned());
                    let new_html = math_core::html::render(&updated_doc, &html_options)
                        .map_err(|e| e.to_string())?;
                    std::fs::write(path, &new_html)
                        .map_err(|e| format!("{}: {e}", path.display()))?;
                    before_files.push(FileSnapshot {
                        path: path.clone(),
                        content: before_html.clone(),
                    });
                    after_files.push(FileSnapshot {
                        path: path.clone(),
                        content: new_html,
                    });
                } else {
                    let (combined_norm, segments) = combine_normalized_files(&files);
                    let current_analysis = math_core::analyze_with_overrides(
                        &combined_norm,
                        &self.config,
                        &self.overrides,
                    );
                    let current_doc = &current_analysis.document;
                    let mut old_opts = options.html_options()?;
                    old_opts.source_markdown = Some(combined_norm.clone());
                    old_opts.source_segments = segments.clone();
                    let old_payload = math_core::html::build_source_payload(current_doc, &old_opts);

                    for (file_idx, (path, raw)) in files.iter().enumerate() {
                        let file_edits: Vec<&BlockEdit> = edits
                            .iter()
                            .filter(|e| e.file_index.min(files.len() - 1) == file_idx)
                            .collect();
                        let old_indices: Vec<usize> = old_payload
                            .blocks
                            .iter()
                            .enumerate()
                            .filter(|(_, b)| b.file_index == file_idx)
                            .map(|(idx, _)| idx)
                            .collect();

                        let changed = file_edits.len() != old_indices.len()
                            || file_edits.iter().zip(old_indices.iter()).any(|(e, &idx)| {
                                e.edited || e.md.trim() != old_payload.blocks[idx].md.trim()
                            });
                        if !changed {
                            continue;
                        }

                        let updated = update_markdown_file(
                            raw,
                            current_doc,
                            &combined_norm,
                            &segments[file_idx],
                            &old_indices,
                            &file_edits,
                        );
                        if updated != *raw {
                            std::fs::write(path, &updated)
                                .map_err(|e| format!("{}: {e}", path.display()))?;
                            before_files.push(FileSnapshot {
                                path: path.clone(),
                                content: raw.clone(),
                            });
                            after_files.push(FileSnapshot {
                                path: path.clone(),
                                content: updated,
                            });
                        }
                    }
                }
            }
        }

        self.prune_overrides_for_source(source)?;
        self.build_live_edit_result(source, options, before_files, after_files)
    }

    /// Writes `files` (used when undoing/redoing a source edit) and refreshes
    /// preview & analysis state without reloading the active editor frame.
    pub fn write_source_files(
        &mut self,
        source: &Source,
        files: Vec<FileSnapshot>,
        options: &CompileOptions,
    ) -> Result<LiveEditResult, String> {
        let mut verified = Vec::with_capacity(files.len());
        for snap in files {
            let canonical = snap
                .path
                .canonicalize()
                .map_err(|e| format!("{}: {e}", snap.path.display()))?;
            if !self.granted.contains(&canonical) {
                return Err(format!(
                    "{}: not opened in Mathia (use Open or drop the file on the window)",
                    snap.path.display()
                ));
            }
            verified.push((canonical, snap.content));
        }
        for (path, content) in &verified {
            std::fs::write(path, content).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        self.prune_overrides_for_source(source)?;
        self.build_live_edit_result(source, options, Vec::new(), Vec::new())
    }

    fn prune_overrides_for_source(&mut self, source: &Source) -> Result<(), String> {
        if self.overrides.is_empty() || self.is_html_source(source) {
            return Ok(());
        }
        let content = self.read(source)?;
        if math_core::is_html(&content) {
            return Ok(());
        }
        let nodes = math_core::parser::parse_markdown(&math_core::normalize_markdown(&content));
        self.overrides.prune_oob(nodes.len());
        Ok(())
    }

    fn build_live_edit_result(
        &mut self,
        source: &Source,
        options: &CompileOptions,
        before_files: Vec<FileSnapshot>,
        after_files: Vec<FileSnapshot>,
    ) -> Result<LiveEditResult, String> {
        let compiled = self.compile_preview_with_progress(source, options, |_, _| {})?;
        let analysis = self.analyze(source)?;
        let stamp = self.stamp(source)?;
        let mut html_options = options.html_options()?;
        html_options.source_name = match source {
            Source::Path(path) => path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            _ => None,
        };
        if !self.is_html_source(source) {
            let (norm, segments) = self.normalized_markdown_with_segments(source)?;
            html_options.source_markdown = Some(norm);
            html_options.source_segments = segments;
        }
        let source_data = math_core::html::build_source_payload(&analysis.document, &html_options);
        let fallback_title = html_options
            .source_name
            .as_deref()
            .or(html_options.title.as_deref())
            .or(analysis.document.title.as_deref())
            .unwrap_or("Study guide");
        let mindmap_json = serde_json::to_string(&math_core::mindmap::extract(
            &analysis.document,
            fallback_title,
        ))
        .map_err(|e| e.to_string())?;
        let has_ai = !html_options.ai.is_empty();
        let blocks_patch = analysis
            .document
            .blocks
            .iter()
            .map(|b| RenderedBlockPatch {
                id: b.id.clone(),
                outer_html: math_core::html::render_block_outer_html(&analysis.document, b, has_ai),
            })
            .collect();
        let toc = analysis.document.toc.clone();

        Ok(LiveEditResult {
            revision: compiled.revision,
            changed: compiled.changed,
            stamp,
            semantic_blocks: compiled.semantic_blocks,
            bytes: compiled.bytes,
            analysis,
            source_data,
            toc,
            mindmap_json,
            blocks_patch,
            before_files,
            after_files,
        })
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

fn combine_normalized_files(files: &[(PathBuf, String)]) -> (String, Vec<Range<usize>>) {
    let mut combined = String::new();
    let mut segments = Vec::with_capacity(files.len());
    for (i, (_, content)) in files.iter().enumerate() {
        let trimmed = content.trim_matches(['\r', '\n']);
        let norm = math_core::normalize_markdown(trimmed);
        let clean = norm.trim_end_matches('\n');
        if i > 0 {
            combined.push_str("\n\n");
        }
        let start = combined.len();
        combined.push_str(clean);
        let end = combined.len();
        segments.push(start..end);
    }
    if !combined.is_empty() && !combined.ends_with('\n') {
        combined.push('\n');
    }
    (combined, segments)
}

fn update_markdown_file(
    raw: &str,
    current_doc: &Document,
    combined_norm: &str,
    segment: &Range<usize>,
    old_indices: &[usize],
    file_edits: &[&BlockEdit],
) -> String {
    let crlf = raw.contains("\r\n");
    let raw_lf = raw.replace("\r\n", "\n");
    let lead_len = raw_lf.len() - raw_lf.trim_start_matches('\n').len();
    let trimmed_raw = raw_lf.trim_matches('\n');
    let norm_i = math_core::normalize_markdown(trimmed_raw);
    let can_splice = trimmed_raw == norm_i.trim_matches('\n')
        && !old_indices.is_empty()
        && old_indices.len() == file_edits.len();

    let mut out_lf = if can_splice {
        let mut ranges = Vec::with_capacity(old_indices.len());
        let mut valid = true;
        let mut prev_end = 0usize;
        for &idx in old_indices {
            let Some(block) = current_doc.blocks.get(idx) else {
                valid = false;
                break;
            };
            let Some(r) =
                math_core::markdown::block_source_range(current_doc, block, combined_norm)
            else {
                valid = false;
                break;
            };
            if r.start < segment.start || r.end > segment.end {
                valid = false;
                break;
            }
            let local_start = r.start - segment.start + lead_len;
            let local_end = r.end - segment.start + lead_len;
            if local_start < prev_end || local_end > raw_lf.len() {
                valid = false;
                break;
            }
            prev_end = local_end;
            ranges.push(local_start..local_end);
        }

        if valid {
            let mut spliced = raw_lf.clone();
            for (r, edit) in ranges.into_iter().zip(file_edits.iter()).rev() {
                let replacement = edit.md.replace("\r\n", "\n");
                let replacement = replacement.trim();
                if replacement.is_empty() {
                    // Remove block and collapse an adjacent blank-line separator cleanly.
                    let mut del_start = r.start;
                    let mut del_end = r.end;
                    if spliced[del_end..].starts_with("\n\n") {
                        del_end += 2;
                    } else if del_start >= 2 && spliced[..del_start].ends_with("\n\n") {
                        del_start -= 2;
                    }
                    spliced.replace_range(del_start..del_end, "");
                } else {
                    spliced.replace_range(r, replacement);
                }
            }
            spliced
        } else {
            reconstruct_file_markdown(&raw_lf, file_edits)
        }
    } else {
        reconstruct_file_markdown(&raw_lf, file_edits)
    };

    if !out_lf.is_empty() && !out_lf.ends_with('\n') {
        out_lf.push('\n');
    }
    if crlf {
        out_lf.replace('\n', "\r\n")
    } else {
        out_lf
    }
}

fn reconstruct_file_markdown(_raw_lf: &str, file_edits: &[&BlockEdit]) -> String {
    let mut parts = Vec::new();
    for edit in file_edits {
        let t = edit.md.replace("\r\n", "\n");
        let t = t.trim();
        if !t.is_empty() {
            parts.push(t.to_string());
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("{}\n", parts.join("\n\n"))
    }
}

fn rebuild_html_document(
    old_doc: &Document,
    edits: &[BlockEdit],
    config: &ScoringConfig,
    title_override: Option<&str>,
) -> Document {
    let mut new_blocks = Vec::with_capacity(edits.len());
    for edit in edits {
        let existing = old_doc.blocks.iter().find(|b| b.id == edit.id);
        if !edit.edited
            && let Some(existing) = existing
        {
            new_blocks.push(existing.clone());
            continue;
        }
        let figures = existing
            .map(|b| math_core::clean_html::extract_figures_html(&b.html))
            .unwrap_or_default();
        let trimmed = edit.md.trim();
        if trimmed.is_empty() && figures.is_empty() {
            continue;
        }
        if trimmed.is_empty() {
            if let Some(mut existing) = existing.cloned() {
                existing.html = figures;
                new_blocks.push(existing);
            }
            continue;
        }
        let mut parsed = math_core::analyze(trimmed, config);
        if !figures.is_empty()
            && let Some(last) = parsed.blocks.last_mut()
        {
            last.html.push_str(&figures);
        }
        new_blocks.extend(parsed.blocks);
    }

    // Ensure unique HTML ids after partial re-analysis.
    let mut seen_ids = HashSet::new();
    for (idx, block) in new_blocks.iter_mut().enumerate() {
        let base = if block.id.is_empty() {
            format!("block-{}-{}", block.kind.slug(), idx + 1)
        } else {
            block.id.clone()
        };
        let mut candidate = base.clone();
        let mut suffix = 2usize;
        while !seen_ids.insert(candidate.clone()) {
            candidate = format!("{base}-{suffix}");
            suffix += 1;
        }
        block.id = candidate;
    }

    let mut doc = Document::new(Vec::new(), new_blocks);
    doc.direction = old_doc.direction;
    if let Some(title) = title_override.filter(|t| !t.trim().is_empty()) {
        doc.title = Some(title.to_string());
    } else if doc.title.is_none() {
        doc.title = old_doc.title.clone();
    }
    doc
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
    fn analyze_reports_blocks_and_one_tag_per_node() {
        let view = Session::default().analyze(&text()).unwrap();
        assert_eq!(view.tags.len(), view.document.nodes.len());
        assert_eq!(view.block_of.len(), view.document.nodes.len());
        let kinds: Vec<_> = view.document.blocks.iter().map(|b| b.kind).collect();
        assert!(kinds.contains(&BlockType::Theorem), "{kinds:?}");
        assert!(kinds.contains(&BlockType::Proof), "{kinds:?}");
        // The shape the frontend reads.
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["tags"][1]["label"], "B-THEOREM");
        assert_eq!(json["tags"][1]["kind"], "theorem");
        assert_eq!(
            json["confidence_threshold"],
            serde_json::json!(ScoringConfig::default().threshold)
        );
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
            ml_confidence_threshold: -1.0,
            ..ScoringConfig::default()
        };
        assert!(session.set_config(config).is_err());
        assert_eq!(session.config(), &ScoringConfig::default());

        assert!(is_markdown(Path::new("notes.MD")));
        assert!(!is_markdown(Path::new("notes.html")));
    }

    #[test]
    fn session_switches_between_crf_and_weighted_sum() {
        let mut session = Session::default();
        assert_eq!(session.config().tagger, math_core::TaggerModel::WeightedSum);
        let mut config = session.config().clone();
        config.tagger = math_core::TaggerModel::Crf;
        session.set_config(config).unwrap();
        assert_eq!(session.config().tagger, math_core::TaggerModel::Crf);
        let view = session.analyze(&text()).unwrap();
        assert!(!view.document.blocks.is_empty());
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

    #[test]
    fn live_edit_splices_single_markdown_and_supports_undo_restore() {
        let initial =
            "# Notes\n\n<!-- keep comment -->\n\n**Theorem 1.** Let $x > 0$.\n\nPlain text.\n";
        let path = temp_file("live-single.md", initial);
        let mut session = Session::default();
        let canon = session.grant(&path).unwrap();
        let source = Source::Path(canon.clone());

        let compiled = session
            .compile_preview(&source, &CompileOptions::default())
            .unwrap();
        assert!(compiled.changed);

        let analysis = session.analyze(&source).unwrap();
        let mut opts = CompileOptions::default().html_options().unwrap();
        opts.source_markdown = Some(math_core::normalize_markdown(initial));
        let payload = math_core::html::build_source_payload(&analysis.document, &opts);

        let edits: Vec<BlockEdit> = payload
            .blocks
            .iter()
            .map(|b| {
                let mut md = b.md.clone();
                let mut edited = false;
                if b.id == "block-theorem-1" {
                    md = "**Theorem 1.** Let $x > 1$.".to_string();
                    edited = true;
                }
                BlockEdit {
                    id: b.id.clone(),
                    md,
                    file_index: b.file_index,
                    edited,
                }
            })
            .collect();

        let res = session
            .apply_live_edit(&source, edits, &CompileOptions::default())
            .unwrap();
        let on_disk = std::fs::read_to_string(&canon).unwrap();
        assert!(on_disk.contains("<!-- keep comment -->"), "{on_disk}");
        assert!(on_disk.contains("**Theorem 1.** Let $x > 1$."), "{on_disk}");
        assert_eq!(res.before_files.len(), 1);
        assert_eq!(res.before_files[0].content, initial);

        // Undo restores exact original file contents.
        session
            .write_source_files(&source, res.before_files, &CompileOptions::default())
            .unwrap();
        assert_eq!(std::fs::read_to_string(&canon).unwrap(), initial);
    }

    #[test]
    fn live_edit_on_multi_path_writes_only_modified_file() {
        let f1 = temp_file("multi-edit-1.md", "# Chapter 1\n\nUntouched one.\n");
        let f2 = temp_file("multi-edit-2.md", "# Chapter 2\n\nOriginal two.\n");
        let mut session = Session::default();
        let f1 = session.grant(&f1).unwrap();
        let f2 = session.grant(&f2).unwrap();
        let source = Source::MultiPath(vec![f1.clone(), f2.clone()]);

        let compiled = session
            .compile_preview(&source, &CompileOptions::default())
            .unwrap();
        assert!(compiled.changed);

        let analysis = session.analyze(&source).unwrap();
        let (norm, segs) = session.normalized_markdown_with_segments(&source).unwrap();
        let mut opts = CompileOptions::default().html_options().unwrap();
        opts.source_markdown = Some(norm);
        opts.source_segments = segs;
        let payload = math_core::html::build_source_payload(&analysis.document, &opts);

        let edits: Vec<BlockEdit> = payload
            .blocks
            .iter()
            .map(|b| {
                let is_target = b.file_index == 1 && b.md.contains("Original two.");
                BlockEdit {
                    id: b.id.clone(),
                    md: if is_target {
                        "Updated two.".to_string()
                    } else {
                        b.md.clone()
                    },
                    file_index: b.file_index,
                    edited: is_target,
                }
            })
            .collect();

        let res = session
            .apply_live_edit(&source, edits, &CompileOptions::default())
            .unwrap();
        assert_eq!(res.after_files.len(), 1);
        assert_eq!(res.after_files[0].path, f2);
        assert_eq!(
            std::fs::read_to_string(&f1).unwrap(),
            "# Chapter 1\n\nUntouched one.\n"
        );
        assert!(
            std::fs::read_to_string(&f2)
                .unwrap()
                .contains("Updated two.")
        );
    }

    #[test]
    fn live_edit_on_html_preserves_untouched_svg_figures() {
        let html = "<!DOCTYPE html><html><head><title>Geo</title></head><body><main>\
            <h1>Shapes</h1>\
            <p>Old intro.</p>\
            <figure><svg viewBox=\"0 0 10 10\"><circle cx=\"5\" cy=\"5\" r=\"4\"/></svg></figure>\
            </main></body></html>";
        let path = temp_file("svg-live.html", html);
        let mut session = Session::default();
        let path = session.grant(&path).unwrap();
        let source = Source::Path(path.clone());

        let analysis = session.analyze(&source).unwrap();
        let opts = CompileOptions::default().html_options().unwrap();
        let payload = math_core::html::build_source_payload(&analysis.document, &opts);
        let edits: Vec<BlockEdit> = payload
            .blocks
            .iter()
            .map(|b| {
                let edit_me = b.md.contains("Old intro.");
                BlockEdit {
                    id: b.id.clone(),
                    md: if edit_me {
                        "New intro with $r = 4$.".to_string()
                    } else {
                        b.md.clone()
                    },
                    file_index: b.file_index,
                    edited: edit_me,
                }
            })
            .collect();

        session
            .apply_live_edit(&source, edits, &CompileOptions::default())
            .unwrap();
        let updated = std::fs::read_to_string(&path).unwrap();
        assert!(updated.contains("New intro with"), "{updated}");
        assert!(updated.contains("<svg"), "SVG figure must be preserved");
        assert!(
            updated.contains("<circle"),
            "SVG contents must be preserved"
        );
    }
}
