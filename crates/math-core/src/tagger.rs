//! Sequence labeling abstraction.
//!
//! Provides a unified [`SequenceTagger`] trait to decouple block boundary
//! prediction (Stages 3 & 4) from deterministic rendering, cross-referencing,
//! and manual [`crate::classifier::ScopeOverrides`].

use serde::{Deserialize, Serialize};

use crate::features::NodeFeatures;
use crate::model::{BlockType, Node};

/// Sequence labeling tag in the BIO (Begin, Inside, Outside) scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "tag", content = "kind", rename_all = "snake_case")]
pub enum BioTag {
    Outside,
    Begin(BlockType),
    Inside(BlockType),
}

impl BioTag {
    pub fn is_outside(&self) -> bool {
        matches!(self, BioTag::Outside)
    }

    pub fn is_begin(&self) -> bool {
        matches!(self, BioTag::Begin(_))
    }

    pub fn is_inside(&self) -> bool {
        matches!(self, BioTag::Inside(_))
    }

    pub fn block_type(&self) -> Option<BlockType> {
        match self {
            BioTag::Outside => None,
            BioTag::Begin(k) | BioTag::Inside(k) => Some(*k),
        }
    }

    /// BIO label string (e.g. `O`, `B-THEOREM`, `I-THEOREM`).
    pub fn to_label(&self) -> String {
        match self {
            BioTag::Outside => "O".to_string(),
            BioTag::Begin(k) => format!("B-{}", k.slug().to_uppercase()),
            BioTag::Inside(k) => format!("I-{}", k.slug().to_uppercase()),
        }
    }

    /// Parses a BIO label string (e.g. `O`, `B-THEOREM`, `I-LEMMA`).
    pub fn from_label(s: &str) -> Option<BioTag> {
        let s = s.trim();
        if s.eq_ignore_ascii_case("O") {
            return Some(BioTag::Outside);
        }
        if let Some(rest) = s.strip_prefix("B-").or_else(|| s.strip_prefix("b-")) {
            let kind = parse_block_type_slug(rest)?;
            return Some(BioTag::Begin(kind));
        }
        if let Some(rest) = s.strip_prefix("I-").or_else(|| s.strip_prefix("i-")) {
            let kind = parse_block_type_slug(rest)?;
            return Some(BioTag::Inside(kind));
        }
        None
    }
}

fn parse_block_type_slug(s: &str) -> Option<BlockType> {
    BlockType::SEMANTIC
        .iter()
        .find(|&&kind| kind.slug().eq_ignore_ascii_case(s))
        .copied()
}

/// A node labeled with its predicted BIO tag and confidence margin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaggedNode {
    pub tag: BioTag,
    pub confidence: f32,
}

/// Abstract sequence labeler over parsed Markdown AST nodes and static features.
pub trait SequenceTagger {
    fn tag(&self, nodes: &[Node], features: &[NodeFeatures]) -> Vec<TaggedNode>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn biotag_labels_and_parsing() {
        assert_eq!(BioTag::Outside.to_label(), "O");
        assert_eq!(BioTag::Begin(BlockType::Theorem).to_label(), "B-THEOREM");
        assert_eq!(BioTag::Inside(BlockType::Proof).to_label(), "I-PROOF");

        assert_eq!(BioTag::from_label("O"), Some(BioTag::Outside));
        assert_eq!(
            BioTag::from_label("B-THEOREM"),
            Some(BioTag::Begin(BlockType::Theorem))
        );
        assert_eq!(
            BioTag::from_label("i-lemma"),
            Some(BioTag::Inside(BlockType::Lemma))
        );
        assert_eq!(BioTag::from_label("invalid"), None);
    }
}
