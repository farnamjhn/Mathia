//! Domain model shared by the parser, classifier and HTML compiler.

use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::features::Features;

/// Semantic role of a block in the compiled document.
///
/// Declaration order is the deterministic tie-break order used by the scorer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockType {
    Theorem,
    Lemma,
    Proposition,
    Corollary,
    Definition,
    Proof,
    Example,
    Remark,
    Exercise,
    /// The worked answer to an example or exercise.
    Solution,
    Claim,
    Notation,
    /// A structural Markdown heading that is not itself a semantic opener.
    Heading,
    /// Ordinary prose / math that no classifier rule claimed.
    Text,
}

impl BlockType {
    /// Every type the classifier can assign from keywords and features.
    pub const SEMANTIC: [BlockType; 12] = [
        BlockType::Theorem,
        BlockType::Lemma,
        BlockType::Proposition,
        BlockType::Corollary,
        BlockType::Definition,
        BlockType::Proof,
        BlockType::Example,
        BlockType::Remark,
        BlockType::Exercise,
        BlockType::Solution,
        BlockType::Claim,
        BlockType::Notation,
    ];

    pub fn is_semantic(self) -> bool {
        !matches!(self, BlockType::Heading | BlockType::Text)
    }

    /// Claims that are stated and then (usually) proved.
    pub fn is_statement(self) -> bool {
        matches!(
            self,
            BlockType::Theorem
                | BlockType::Lemma
                | BlockType::Proposition
                | BlockType::Corollary
                | BlockType::Claim
        )
    }

    /// An argument that follows the block it belongs to and runs to a QED
    /// marker: proofs and solutions.
    pub fn is_argument(self) -> bool {
        matches!(self, BlockType::Proof | BlockType::Solution)
    }

    /// The block type a LaTeX theorem-like environment name stands for
    /// (`\begin{theorem}`, `\begin{defn*}`, …).
    pub fn from_env_name(name: &str) -> Option<BlockType> {
        Some(
            match name
                .trim()
                .trim_end_matches('*')
                .to_ascii_lowercase()
                .as_str()
            {
                "theorem" | "thm" => BlockType::Theorem,
                "lemma" | "lem" => BlockType::Lemma,
                "proposition" | "prop" => BlockType::Proposition,
                "corollary" | "cor" => BlockType::Corollary,
                "definition" | "defn" | "def" => BlockType::Definition,
                "proof" => BlockType::Proof,
                "example" | "exmp" | "ex" => BlockType::Example,
                "remark" | "rem" | "note" => BlockType::Remark,
                "exercise" | "problem" => BlockType::Exercise,
                "solution" | "sol" => BlockType::Solution,
                "claim" => BlockType::Claim,
                "notation" => BlockType::Notation,
                _ => return None,
            },
        )
    }

    /// Stable machine name, used in ids (`block-theorem-1`) and CSS classes.
    pub fn slug(self) -> &'static str {
        match self {
            BlockType::Theorem => "theorem",
            BlockType::Lemma => "lemma",
            BlockType::Proposition => "proposition",
            BlockType::Corollary => "corollary",
            BlockType::Definition => "definition",
            BlockType::Proof => "proof",
            BlockType::Example => "example",
            BlockType::Remark => "remark",
            BlockType::Exercise => "exercise",
            BlockType::Solution => "solution",
            BlockType::Claim => "claim",
            BlockType::Notation => "notation",
            BlockType::Heading => "heading",
            BlockType::Text => "text",
        }
    }

    /// Human label shown in callout badges and the table of contents.
    pub fn label(self, dir: Direction) -> &'static str {
        match (self, dir) {
            (BlockType::Theorem, Direction::Rtl) => "قضیه",
            (BlockType::Lemma, Direction::Rtl) => "لم",
            (BlockType::Proposition, Direction::Rtl) => "گزاره",
            (BlockType::Corollary, Direction::Rtl) => "نتیجه",
            (BlockType::Definition, Direction::Rtl) => "تعریف",
            (BlockType::Proof, Direction::Rtl) => "برهان",
            (BlockType::Example, Direction::Rtl) => "مثال",
            (BlockType::Remark, Direction::Rtl) => "تذکر",
            (BlockType::Exercise, Direction::Rtl) => "تمرین",
            (BlockType::Solution, Direction::Rtl) => "حل",
            (BlockType::Claim, Direction::Rtl) => "ادعا",
            (BlockType::Notation, Direction::Rtl) => "نمادگذاری",
            (BlockType::Heading, Direction::Rtl) => "بخش",
            (BlockType::Text, Direction::Rtl) => "متن",
            (BlockType::Theorem, Direction::Ltr) => "Theorem",
            (BlockType::Lemma, Direction::Ltr) => "Lemma",
            (BlockType::Proposition, Direction::Ltr) => "Proposition",
            (BlockType::Corollary, Direction::Ltr) => "Corollary",
            (BlockType::Definition, Direction::Ltr) => "Definition",
            (BlockType::Proof, Direction::Ltr) => "Proof",
            (BlockType::Example, Direction::Ltr) => "Example",
            (BlockType::Remark, Direction::Ltr) => "Remark",
            (BlockType::Exercise, Direction::Ltr) => "Exercise",
            (BlockType::Solution, Direction::Ltr) => "Solution",
            (BlockType::Claim, Direction::Ltr) => "Claim",
            (BlockType::Notation, Direction::Ltr) => "Notation",
            (BlockType::Heading, Direction::Ltr) => "Section",
            (BlockType::Text, Direction::Ltr) => "Text",
        }
    }
}

/// Base text direction of a block or of the whole document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Rtl,
    Ltr,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Rtl => "rtl",
            Direction::Ltr => "ltr",
        }
    }

    /// Any strong RTL letter makes the block RTL: Persian text routinely
    /// embeds English terms (and LaTeX letters), which must not flip it.
    /// Pure Latin text is LTR. Returns `None` when the text has neither
    /// (e.g. a pure-math node).
    pub fn detect(text: &str) -> Option<Direction> {
        let mut ltr = false;
        for c in text.chars() {
            match c {
                '\u{0590}'..='\u{08FF}' | '\u{FB1D}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}'
                    if c.is_alphabetic() =>
                {
                    return Some(Direction::Rtl)
                }
                c if c.is_ascii_alphabetic() => ltr = true,
                _ => {}
            }
        }
        ltr.then_some(Direction::Ltr)
    }
}

#[cfg(test)]
mod direction_tests {
    use super::Direction;

    #[test]
    fn mixed_persian_and_english_is_rtl() {
        let heading = "بخش سوم: استقلال خطی و وابستگی خطی (Linear Independence and Dependence)";
        assert_eq!(Direction::detect(heading), Some(Direction::Rtl));
    }

    #[test]
    fn pure_english_is_ltr_and_math_only_is_none() {
        assert_eq!(Direction::detect("Linear Independence"), Some(Direction::Ltr));
        assert_eq!(Direction::detect("1 + 2 = 3"), None);
    }
}

/// Kind of a top-level Markdown node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Heading(u8),
    Paragraph,
    /// A paragraph that holds nothing but `$$ … $$`.
    DisplayMath,
    List,
    Quote,
    Code,
    Table,
    Rule,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MathSpan {
    pub latex: String,
    pub display: bool,
}

/// A LaTeX theorem-like environment (`\begin{theorem}[Name] … \end{theorem}`),
/// recorded on the first node of its body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvSpan {
    pub kind: BlockType,
    /// The optional `[Name]` argument.
    pub name: Option<String>,
    /// Number of nodes in the environment, this one included.
    pub len: usize,
}

/// Why a block's scope stopped where it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    /// Headings and plain text are not scoped.
    Structural,
    /// A QED marker closed the argument.
    Qed,
    /// The next node opens a block of its own.
    NextOpener,
    Heading,
    EndOfDocument,
    /// The next node does not read as a continuation of the statement.
    NoContinuation,
    /// The configured size cap was reached.
    SizeLimit,
    /// The scope is the exact extent of a LaTeX environment.
    Environment,
    /// A [`crate::ScopeOverrides`] entry moved the end.
    Override,
}

impl EndReason {
    pub fn as_str(self) -> &'static str {
        match self {
            EndReason::Structural => "structural",
            EndReason::Qed => "qed",
            EndReason::NextOpener => "next_opener",
            EndReason::Heading => "heading",
            EndReason::EndOfDocument => "end_of_document",
            EndReason::NoContinuation => "no_continuation",
            EndReason::SizeLimit => "size_limit",
            EndReason::Environment => "environment",
            EndReason::Override => "override",
        }
    }
}

/// One top-level Markdown node: the atom the classifier scores and the
/// segmenter groups into [`SemanticBlock`]s.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub kind: NodeKind,
    /// Prose only, math removed.
    pub text: String,
    /// Prose with LaTeX sources spliced in at their original positions.
    pub flat: String,
    pub math: Vec<MathSpan>,
    /// The node starts with `**…**` / `*…*`, the usual "**Theorem 1.**" form.
    pub leading_emphasis: bool,
    /// Rendered HTML. For headings this is the inner HTML only.
    pub html: String,
    /// Byte range in the Markdown source.
    pub source: Range<usize>,
    /// Set on the first node of a LaTeX theorem-like environment.
    pub env: Option<EnvSpan>,
}

/// A classified, addressable unit of the compiled document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticBlock {
    /// Deterministic id: `block-theorem-1`, `block-proof-2`, `sec-3`, …
    pub id: String,
    pub kind: BlockType,
    /// Indices into [`Document::nodes`] covered by this block (its *scope*).
    pub span: Range<usize>,
    pub heading_level: Option<u8>,
    /// Label parsed from the opener, e.g. `۳.۲`.
    pub number: Option<String>,
    /// Parenthesised name parsed from the opener, e.g. `هاینه–کانتور`.
    pub name: Option<String>,
    /// Title used in the table of contents.
    pub title: String,
    pub direction: Direction,
    /// Winning classifier score of the opener (0 for headings and text).
    pub score: f32,
    /// Features of the opener node, kept for debugging and decision models.
    pub features: Option<Features>,
    /// Id of the statement this proof proves, or of the example / exercise
    /// this solution answers.
    pub proves: Option<String>,
    /// Why the scope ended where it did.
    pub end: EndReason,
    pub html: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TocEntry {
    /// Id of the target block.
    pub id: String,
    pub title: String,
    /// 1-based nesting depth; semantic blocks sit one level under their section.
    pub depth: u8,
    pub kind: BlockType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub title: Option<String>,
    pub direction: Direction,
    pub nodes: Vec<Node>,
    pub blocks: Vec<SemanticBlock>,
    pub toc: Vec<TocEntry>,
}

impl Document {
    pub fn new(nodes: Vec<Node>, blocks: Vec<SemanticBlock>) -> Self {
        let direction = document_direction(&nodes);
        let title = blocks
            .iter()
            .find(|b| b.kind == BlockType::Heading && b.heading_level == Some(1))
            .or_else(|| blocks.iter().find(|b| b.kind == BlockType::Heading))
            .map(|b| b.title.clone());

        let mut toc = Vec::new();
        let mut section_depth = 0u8;
        for b in &blocks {
            match b.kind {
                BlockType::Text => {}
                BlockType::Heading => {
                    section_depth = b.heading_level.unwrap_or(1);
                    toc.push(TocEntry {
                        id: b.id.clone(),
                        title: b.title.clone(),
                        depth: section_depth,
                        kind: b.kind,
                    });
                }
                _ => toc.push(TocEntry {
                    id: b.id.clone(),
                    title: b.title.clone(),
                    depth: section_depth + 1,
                    kind: b.kind,
                }),
            }
        }
        Document {
            title,
            direction,
            nodes,
            blocks,
            toc,
        }
    }

    pub fn blocks_of(&self, kind: BlockType) -> impl Iterator<Item = &SemanticBlock> {
        self.blocks.iter().filter(move |b| b.kind == kind)
    }
}

pub(crate) fn document_direction(nodes: &[Node]) -> Direction {
    let all: String = nodes.iter().map(|n| n.text.as_str()).collect();
    Direction::detect(&all).unwrap_or(Direction::Ltr)
}
