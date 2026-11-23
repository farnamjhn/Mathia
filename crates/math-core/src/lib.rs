//! `math-core` — Mathia's semantic document compiler.
//!
//! Pipeline: Markdown + LaTeX → [`parser`] → nodes → [`classifier`] →
//! [`model::Document`] → [`html`] → one standalone HTML file.
//!
//! The crate is pure and deterministic: no Tauri, no network, no LLM. Decision
//! models plug in from outside through [`classifier::BlockJudge`].

pub mod classifier;
pub mod clean_html;
pub mod config;
pub mod crf;
mod crossref;
mod dom;
pub mod error;
pub mod features;
pub mod html;
mod html_import;
pub mod markdown;
pub mod mindmap;
pub mod model;
pub mod normalize;
pub mod parser;
pub mod repair;
mod scoping;
mod sha256;
pub mod tagger;
pub mod text;

pub use classifier::{
    AppliedVerdict, BlockClassifier, BlockJudge, Classification, ContentAnchor,
    HeuristicClassifier, JudgedClassification, OverrideKey, ResolvedOverrides, ScopeOverrides,
    Verdict,
};
pub use clean_html::{analyze_html, analyze_html_detailed, clean_html, is_html, is_html_file};
pub use config::{CONFIG_SCHEMA_VERSION, ConfigFormat, ScoringConfig};
pub use crf::{CrfModel, CrfTagger, RawCrfWeights};
pub use error::{Error, Result};
pub use features::{
    DumpNode, FeatureDump, Features, Lexicon, NodeFeatures, TelemetryEntry, anonymize_dump_nodes,
    append_telemetry_entry, create_override_telemetry, detect_language, dump_features,
    dump_features_with_overrides,
};
pub use html::{
    AiOptions, AiProvider, CustomProvider, EmbeddedFont, HtmlOptions, KatexAssets, MathAssets,
};
pub use model::{
    BlockType, Direction, Document, EndReason, EnvSpan, ListInfo, Node, NodeKind, SemanticBlock,
    TocEntry,
};
pub use normalize::normalize_markdown;
pub use repair::{Repair, repair_markdown, wrap_bare_math};
pub use tagger::{BioTag, SequenceTagger, TaggedNode};

/// A [`Document`] together with the evidence behind every decision: the static
/// features and BIO tag of every node, and (on each block) why its scope ended.
#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    pub document: Document,
    /// One entry per node of the document.
    pub features: Vec<NodeFeatures>,
    /// One entry per node of the document.
    pub tags: Vec<TaggedNode>,
    /// For every node, the index in `document.blocks` of its block.
    pub block_of: Vec<usize>,
}

impl Analysis {
    /// A plain-text table answering "why is this a theorem": one line per
    /// node with its block, its BIO tag and the tagger's confidence margin.
    pub fn explain(&self) -> String {
        let mut out = String::new();
        let mut current = usize::MAX;
        for (index, tag) in self.tags.iter().enumerate() {
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
            let preview: String = self
                .document
                .nodes
                .get(index)
                .map(|node| {
                    node.flat
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .chars()
                        .take(48)
                        .collect()
                })
                .unwrap_or_default();
            let best = if let Some(kind) = tag.tag.block_type() {
                format!("{} {:.2}", kind.slug(), tag.confidence)
            } else {
                "-".to_string()
            };
            out.push_str(&format!(
                "  {index:>4} {} {best}  | {preview}\n",
                if tag.tag.is_begin() { '*' } else { ' ' },
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
    if is_html(markdown) {
        let mut analysis = clean_html::analyze_html_detailed(markdown);
        clean_html::apply_overrides(&mut analysis.document, overrides);
        clean_html::reindex(&mut analysis);
        return analysis;
    }
    let nodes = parser::parse_markdown(&normalize_markdown(markdown));
    let Classification {
        blocks,
        features,
        tags,
        block_of,
    } = HeuristicClassifier::new(config.clone()).classify_detailed(&nodes, overrides);
    Analysis {
        document: Document::new(nodes, blocks),
        features,
        tags,
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

/// Markdown or HTML in, standalone Mathia HTML out.
pub fn compile(input: &str, config: &ScoringConfig, options: &HtmlOptions) -> Result<String> {
    if is_html(input) {
        clean_html(input, options)
    } else {
        html::render(&analyze(input, config), options)
    }
}
