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
    /// A structural Markdown heading that is not itself a semantic opener.
    Heading,
    /// Ordinary prose / math that no classifier rule claimed.
    Text,
}

impl BlockType {
    /// Every type the classifier can assign from keywords and features.
    pub const SEMANTIC: [BlockType; 8] = [
        BlockType::Theorem,
        BlockType::Lemma,
        BlockType::Proposition,
        BlockType::Corollary,
        BlockType::Definition,
        BlockType::Proof,
        BlockType::Example,
        BlockType::Remark,
    ];

    pub fn is_semantic(self) -> bool {
        !matches!(self, BlockType::Heading | BlockType::Text)
    }

    /// Claims that are stated and then (usually) proved.
    pub fn is_statement(self) -> bool {
        matches!(
            self,
            BlockType::Theorem | BlockType::Lemma | BlockType::Proposition | BlockType::Corollary
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

    /// Majority vote between strong RTL letters and Latin letters.
    /// Returns `None` when the text has neither (e.g. a pure-math node).
    pub fn detect(text: &str) -> Option<Direction> {
        let (mut rtl, mut ltr) = (0usize, 0usize);
        for c in text.chars() {
            match c {
                '\u{0590}'..='\u{08FF}' | '\u{FB1D}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}'
                    if c.is_alphabetic() =>
                {
                    rtl += 1
                }
                c if c.is_ascii_alphabetic() => ltr += 1,
                _ => {}
            }
        }
        match (rtl, ltr) {
            (0, 0) => None,
            _ if rtl >= ltr => Some(Direction::Rtl),
            _ => Some(Direction::Ltr),
        }
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
