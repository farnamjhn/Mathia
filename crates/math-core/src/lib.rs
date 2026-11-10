//! `math-core` — Mathia's semantic document compiler.
//!
//! Pipeline: Markdown + LaTeX → [`parser`] → nodes → [`classifier`] →
//! [`model::Document`] → [`html`] → one standalone HTML file.
//!
//! The crate is pure and deterministic: no Tauri, no network, no LLM. Decision
//! models plug in from outside through [`classifier::BlockJudge`].

pub mod classifier;
pub mod config;
pub mod error;
pub mod features;
pub mod html;
pub mod model;
pub mod parser;
pub mod text;

pub use classifier::{BlockJudge, HeuristicClassifier, NodeScore, ScopeOverrides, Verdict};
pub use config::ScoringConfig;
pub use error::{Error, Result};
pub use html::{HtmlOptions, KatexAssets, MathAssets};
pub use model::{BlockType, Direction, Document, Node, NodeKind, SemanticBlock, TocEntry};

/// Parses and classifies a Markdown source.
pub fn analyze(markdown: &str, config: &ScoringConfig) -> Document {
    let nodes = parser::parse_markdown(markdown);
    let blocks = HeuristicClassifier::new(config.clone()).classify(&nodes);
    Document::new(nodes, blocks)
}

/// Like [`analyze`], with a decision model reviewing block scopes.
pub fn analyze_with_judge(
    markdown: &str,
    config: &ScoringConfig,
    judge: &dyn BlockJudge,
    max_rounds: usize,
) -> Document {
    let nodes = parser::parse_markdown(markdown);
    let blocks =
        HeuristicClassifier::new(config.clone()).classify_with_judge(&nodes, judge, max_rounds);
    Document::new(nodes, blocks)
}

/// Markdown in, standalone HTML out.
pub fn compile(markdown: &str, config: &ScoringConfig, options: &HtmlOptions) -> Result<String> {
    html::render(&analyze(markdown, config), options)
}
