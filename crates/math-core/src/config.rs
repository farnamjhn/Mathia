//! Everything tunable about classification lives here as plain data, so it can
//! be loaded from a file or edited in the UI without touching the scorer.

use serde::{Deserialize, Serialize};

use crate::model::BlockType;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScoringConfig {
    /// A node opens a block when its best type score reaches this value.
    pub threshold: f32,
    pub weights: Weights,
    pub scope: ScopeConfig,
    pub keywords: Vec<KeywordRule>,
    /// Counted case-sensitively (`\Rightarrow` is not `\rightarrow`).
    pub logic_symbols: Vec<String>,
    /// Looked for at the very end of a node.
    pub qed_markers: Vec<String>,
    pub definitional_cues: Vec<String>,
    pub proof_cues: Vec<String>,
    pub statement_cues: Vec<String>,
    /// A paragraph starting with one of these continues the statement above it.
    pub continuation_cues: Vec<String>,
}

/// Feature weights. Each feature is in `[0, 1]`; the weighted sum is the score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Weights {
    /// The node starts with a keyword of the type.
    pub lead: f32,
    /// …followed by a number, a `(name)`, or `.`/`:` — i.e. it reads as a label.
    pub label: f32,
    /// …and the node starts with bold/italic text.
    pub emphasis: f32,
    /// The keyword only appears somewhere inside the node.
    pub mention: f32,
    /// QED / Halmos ending (proofs).
    pub qed: f32,
    /// Density of implication and quantifier symbols.
    pub logic: f32,
    /// Share of the node that is LaTeX.
    pub math: f32,
    /// "is called", "گوییم", `:=` … (definitions).
    pub definitional: f32,
    /// Type-specific phrasing ("suppose", "فرض کنید", "if … then", "آنگاه").
    pub cue: f32,
}

/// Default scope limits, counted in top-level Markdown nodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScopeConfig {
    /// Statements, definitions, examples and remarks.
    pub statement_max_nodes: usize,
    /// A proof with no QED marker stops after this many nodes.
    pub proof_max_nodes: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeywordRule {
    pub kind: BlockType,
    pub words: Vec<String>,
}

impl Default for Weights {
    fn default() -> Self {
        Weights {
            lead: 0.45,
            label: 0.20,
            emphasis: 0.15,
            mention: 0.10,
            qed: 0.30,
            logic: 0.15,
            math: 0.10,
            definitional: 0.25,
            cue: 0.10,
        }
    }
}

impl Default for ScopeConfig {
    fn default() -> Self {
        ScopeConfig { statement_max_nodes: 6, proof_max_nodes: 12 }
    }
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

impl Default for ScoringConfig {
    fn default() -> Self {
        let rule = |kind, words: &[&str]| KeywordRule { kind, words: strings(words) };
        ScoringConfig {
            threshold: 0.5,
            weights: Weights::default(),
            scope: ScopeConfig::default(),
            keywords: vec![
                rule(BlockType::Theorem, &["قضیه", "theorem", "thm"]),
                rule(BlockType::Lemma, &["لم", "lemma"]),
                rule(BlockType::Proposition, &["گزاره", "proposition", "prop"]),
                rule(BlockType::Corollary, &["نتیجه", "corollary", "cor"]),
                rule(BlockType::Definition, &["تعریف", "definition", "defn", "def"]),
                rule(BlockType::Proof, &["برهان", "اثبات", "proof"]),
                rule(BlockType::Example, &["مثال", "example"]),
                rule(BlockType::Remark, &["تذکر", "نکته", "توجه", "remark", "note"]),
            ],
            logic_symbols: strings(&[
                "\\implies", "\\impliedby", "\\Rightarrow", "\\Longrightarrow", "\\iff",
                "\\Leftrightarrow", "\\Longleftrightarrow", "\\forall", "\\exists", "\\nexists",
                "\\therefore", "\\because", "⇒", "⟹", "⇔", "∀", "∃", "∴",
            ]),
            qed_markers: strings(&[
                "\\blacksquare", "\\square", "\\Box", "\\qed", "∎", "■", "□", "q.e.d", "qed",
                "حکم ثابت شد", "اثبات کامل است", "برهان کامل است", "اثبات کامل می‌شود",
                "برهان کامل می‌شود",
            ]),
            definitional_cues: strings(&[
                "گوییم", "می‌گوییم", "می‌نامیم", "نامیده می‌شود", "تعریف می‌کنیم",
                "تعریف می‌شود", "عبارت است از", "is called", "is said to be", "are called",
                "we say", "we call", "we define", "is defined", ":=", "\\coloneqq",
                "\\triangleq", "\\overset{\\text{def}}",
            ]),
            proof_cues: strings(&[
                "فرض کنید", "فرض کنیم", "برهان خلف", "تناقض", "استقرا", "بنابراین", "suppose",
                "assume", "contradiction", "induction", "without loss of generality",
                "it follows", "hence", "therefore",
            ]),
            statement_cues: strings(&[
                "آنگاه", "اگر و تنها اگر", "به ازای هر", "برای هر", "وجود دارد",
                "if and only if", "for all", "for every", "for any", "there exists", "then",
            ]),
            continuation_cues: strings(&[
                "که در آن", "که", "آنگاه", "به‌طوری‌که", "به طوری که", "و در نتیجه", "where",
                "such that", "then", "in which",
            ]),
        }
    }
}
