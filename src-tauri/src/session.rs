//! Everything the commands do, without any Tauri type: the app state and the
//! thin wrappers over `math-core`. Kept separate so it can be unit-tested.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use math_core::{
    AiOptions, AiProvider, BlockType, CustomProvider, Document, HtmlOptions, MathAssets,
    ScoringConfig,
};
use serde::{Deserialize, Serialize};

/// File extensions accepted from a drag and drop (the open dialog filters on
/// the same list).
pub const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown", "mdown", "txt"];

pub fn is_markdown(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        MARKDOWN_EXTENSIONS
            .iter()
            .any(|m| e.eq_ignore_ascii_case(m))
    })
}

/// What to compile: a file the user opened, or text held by the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Path(PathBuf),
    Text(String),
}

impl Source {
    pub fn path(&self) -> Option<&Path> {
        match self {
            Source::Path(path) => Some(path),
            Source::Text(_) => None,
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
            fonts: Vec::new(),
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

/// The app state: one scoring configuration, the files the user has opened,
/// and the HTML currently shown in the preview.
#[derive(Debug, Default)]
pub struct Session {
    config: ScoringConfig,
    /// Canonical paths the user chose in an open dialog or dropped on the
    /// window. Commands read nothing else from disk.
    granted: HashSet<PathBuf>,
    preview: String,
    revision: u64,
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
        Ok(canonical)
    }

    fn read(&self, source: &Source) -> Result<String, String> {
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
        }
    }

    pub fn analyze(&self, source: &Source) -> Result<AnalysisView, String> {
        let analysis = math_core::analyze_detailed(&self.read(source)?, &self.config);
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

    /// Markdown to standalone HTML with the session's scoring configuration.
    /// Returns the HTML and the number of semantic blocks.
    pub fn compile(
        &self,
        source: &Source,
        options: &CompileOptions,
    ) -> Result<(String, usize), String> {
        let document = math_core::analyze(&self.read(source)?, &self.config);
        let semantic = document
            .blocks
            .iter()
            .filter(|b| b.kind.is_semantic())
            .count();
        let html = math_core::html::render(&document, &options.html_options()?)
            .map_err(|e| e.to_string())?;
        Ok((html, semantic))
    }

    /// Compiles and makes the result the preview.
    pub fn compile_preview(
        &mut self,
        source: &Source,
        options: &CompileOptions,
    ) -> Result<Compiled, String> {
        let (html, semantic_blocks) = self.compile(source, options)?;
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
}
