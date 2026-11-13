//! `math-core` — Mathia's semantic document compiler.
//!
//! Pipeline: Markdown + LaTeX → [`parser`] → nodes → [`classifier`] →
//! [`model::Document`] → [`html`] → one standalone HTML file.
//!
//! The crate is pure and deterministic: no Tauri, no network, no LLM. Decision
//! models plug in from outside through [`classifier::BlockJudge`].

pub mod classifier;
pub mod config;
mod crossref;
pub mod error;
pub mod features;
pub mod html;
pub mod model;
pub mod normalize;
pub mod parser;
pub mod repair;
mod sha256;
pub mod text;
pub mod tree;

pub use classifier::{
    AppliedVerdict, BlockJudge, Classification, HeuristicClassifier, JudgedClassification,
    NodeScore, ScopeOverrides, Verdict,
};
pub use config::{CONFIG_SCHEMA_VERSION, ConfigFormat, Scorer, ScoringConfig};
pub use error::{Error, Result};
pub use html::{
    AiOptions, AiProvider, CustomProvider, EmbeddedFont, HtmlOptions, KatexAssets, MathAssets,
};
pub use model::{
    BlockType, Direction, Document, EndReason, EnvSpan, Node, NodeKind, SemanticBlock, TocEntry,
};
pub use normalize::normalize_markdown;
pub use repair::{Repair, repair_markdown, wrap_bare_math};
pub use tree::DecisionTree;

/// A [`Document`] together with the evidence behind every decision: all type
/// scores of every node, and (on each block) why its scope ended.
#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    pub document: Document,
    /// One entry per node of the document.
    pub scores: Vec<NodeScore>,
    /// For every node, the index in `document.blocks` of its block.
    pub block_of: Vec<usize>,
}

impl Analysis {
    /// A plain-text table answering "why is this a theorem": one line per
    /// node with its block, its best type and score, and the runner-up.
    pub fn explain(&self, threshold: f32) -> String {
        let mut out = String::new();
        let mut current = usize::MAX;
        for (index, score) in self.scores.iter().enumerate() {
            let block_index = self.block_of[index];
            let block = &self.document.blocks[block_index];
            if block_index != current {
                current = block_index;
                out.push_str(&format!(
                    "{}  [{}]  nodes {}..{}  score {:.2}  ended by {}{}\n",
                    block.id,
                    block.kind.slug(),
                    block.span.start,
                    block.span.end,
                    block.score,
                    block.end.as_str(),
                    block
                        .proves
                        .as_deref()
                        .map(|id| format!("  proves {id}"))
                        .unwrap_or_default(),
                ));
            }
            let mut ranked = score.scores.clone();
            ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
            let runner_up = ranked
                .get(1)
                .filter(|(_, s)| *s > 0.0)
                .map(|(kind, s)| format!(", then {} {s:.2}", kind.slug()))
                .unwrap_or_default();
            let preview: String = self.document.nodes[index]
                .flat
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(48)
                .collect();
            let best = if score.best.1 > 0.0 {
                format!("{} {:.2}", score.best.0.slug(), score.best.1)
            } else {
                "-".to_string()
            };
            out.push_str(&format!(
                "  {index:>4} {} {best}{runner_up}  | {preview}\n",
                if score.best.1 >= threshold { '*' } else { ' ' },
            ));
        }
        out
    }
}

/// Normalises OCR artefacts ([`normalize_markdown`]), then parses and
/// classifies. Byte offsets in the result refer to the normalised text.
pub fn analyze(markdown: &str, config: &ScoringConfig) -> Document {
    analyze_detailed(markdown, config).document
}

/// [`analyze`], keeping the per-node scores for diagnostics and tuning.
pub fn analyze_detailed(markdown: &str, config: &ScoringConfig) -> Analysis {
    analyze_with_overrides(markdown, config, &ScopeOverrides::default())
}

/// [`analyze_detailed`] with corrections applied (made by hand in the
/// desktop app, or recorded from a [`BlockJudge`] run). Override keys are
/// node indices of the normalised source.
pub fn analyze_with_overrides(
    markdown: &str,
    config: &ScoringConfig,
    overrides: &ScopeOverrides,
) -> Analysis {
    let nodes = parser::parse_markdown(&normalize_markdown(markdown));
    let Classification {
        blocks,
        scores,
        block_of,
    } = HeuristicClassifier::new(config.clone()).classify_detailed(&nodes, overrides);
    Analysis {
        document: Document::new(nodes, blocks),
        scores,
        block_of,
    }
}

/// Like [`analyze`], with a decision model reviewing block scopes.
pub fn analyze_with_judge(
    markdown: &str,
    config: &ScoringConfig,
    judge: &dyn BlockJudge,
    max_rounds: usize,
) -> Document {
    analyze_with_judge_detailed(
        markdown,
        config,
        judge,
        max_rounds,
        ScopeOverrides::default(),
    )
    .0
}

/// [`analyze_with_judge`] starting from `initial` overrides, also returning
/// the final overrides and the log of verdicts. Passing those overrides to
/// [`analyze_with_overrides`] reproduces the document without the judge.
pub fn analyze_with_judge_detailed(
    markdown: &str,
    config: &ScoringConfig,
    judge: &dyn BlockJudge,
    max_rounds: usize,
    initial: ScopeOverrides,
) -> (Document, ScopeOverrides, Vec<AppliedVerdict>) {
    let nodes = parser::parse_markdown(&normalize_markdown(markdown));
    let JudgedClassification {
        blocks,
        overrides,
        log,
        ..
    } = HeuristicClassifier::new(config.clone())
        .classify_with_judge_detailed(&nodes, judge, max_rounds, initial);
    (Document::new(nodes, blocks), overrides, log)
}

/// Markdown in, standalone HTML out.
pub fn compile(markdown: &str, config: &ScoringConfig, options: &HtmlOptions) -> Result<String> {
    html::render(&analyze(markdown, config), options)
}
