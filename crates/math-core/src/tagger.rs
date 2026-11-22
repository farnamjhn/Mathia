//! Sequence labeling abstraction and heuristic BIO tagger.
//!
//! Provides a unified [`SequenceTagger`] trait to decouple block boundary
//! prediction (Stages 3 & 4) from deterministic rendering, cross-referencing,
//! and manual [`crate::classifier::ScopeOverrides`].

use serde::{Deserialize, Serialize};

use crate::classifier::NodeScore;
use crate::config::ScoringConfig;
use crate::features::{Features, Lexicon, NodeFeatures};
use crate::model::{BlockType, EndReason, Node, NodeKind};

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

fn on(flag: bool) -> f32 {
    if flag { 1.0 } else { 0.0 }
}

/// Heuristic sequence tagger using the weighted feature scorer and greedy scoping rules.
#[derive(Debug, Clone)]
pub struct HeuristicTagger {
    pub config: ScoringConfig,
    pub lexicon: Lexicon,
}

impl HeuristicTagger {
    pub fn new(config: ScoringConfig, lexicon: Lexicon) -> Self {
        HeuristicTagger { config, lexicon }
    }

    pub fn config(&self) -> &ScoringConfig {
        &self.config
    }

    pub fn lexicon(&self) -> &Lexicon {
        &self.lexicon
    }

    /// Score of `features` for one candidate type, in `[0, 1]`, by the
    /// configured [`crate::config::Scorer`].
    pub fn score(&self, features: &Features, kind: BlockType) -> f32 {
        match &self.config.scorer {
            crate::config::Scorer::Linear | crate::config::Scorer::Crf => {
                self.linear_score(features, kind)
            }
            crate::config::Scorer::Tree(tree) => tree.score(features, kind),
        }
    }

    /// Weighted sum of the features for one candidate type, in `[0, 1]`.
    pub fn linear_score(&self, features: &Features, kind: BlockType) -> f32 {
        let w = &self.config.weights;
        let mut score = match &features.lead {
            Some(hit) if hit.kind == kind && !hit.reference => {
                w.lead + w.label * on(hit.labelled) + w.emphasis * on(features.emphasized)
            }
            _ => w.mention * on(features.mentions.contains(&kind)),
        };
        score += match kind {
            BlockType::Proof => {
                w.qed * on(features.qed)
                    + w.logic * features.logic_density
                    + w.cue * on(features.proof_cue)
            }
            BlockType::Definition | BlockType::Notation => {
                w.definitional * on(features.definitional) + w.math * features.math_density
            }
            k if k.is_statement() => {
                w.math * features.math_density
                    + 0.5 * w.logic * features.logic_density
                    + w.cue * on(features.statement_cue)
            }
            _ => 0.0,
        };
        score.clamp(0.0, 1.0)
    }

    pub fn score_node_with_features(&self, node: &Node, features: &Features) -> NodeScore {
        let scores: Vec<(BlockType, f32)> = BlockType::SEMANTIC
            .iter()
            .map(|&kind| (kind, self.score(features, kind)))
            .collect();
        let mut best = (BlockType::Theorem, f32::MIN);
        for &(kind, s) in &scores {
            if s > best.1 {
                best = (kind, s);
            }
        }
        let can_open = matches!(
            node.kind,
            NodeKind::Paragraph | NodeKind::ListItem | NodeKind::Heading(_) | NodeKind::Quote
        );
        NodeScore {
            opens: can_open && best.1 >= self.config.threshold,
            features: features.clone(),
            scores,
            best,
        }
    }

    pub fn score_node(&self, node: &Node) -> NodeScore {
        let features = self.lexicon.extract(node);
        self.score_node_with_features(node, &features)
    }

    pub fn implicit_proof_score(
        &self,
        nodes: &[Node],
        scores: &[NodeScore],
        prev_is_statement: bool,
        i: usize,
        limit: usize,
    ) -> Option<f32> {
        if !prev_is_statement || !matches!(nodes[i].kind, NodeKind::Paragraph | NodeKind::ListItem)
        {
            return None;
        }
        let w = &self.config.weights;
        let features = &scores[i].features;
        let cap = (i + self.config.scope.proof_max_nodes.max(1)).min(limit);
        let qed = (i..cap).any(|j| scores[j].features.qed);
        let score = w.after_statement
            + w.qed * on(qed)
            + w.logic * features.logic_density
            + w.cue * on(features.proof_cue);
        (score >= self.config.threshold).then_some(score.clamp(0.0, 1.0))
    }

    pub fn argument_end(
        &self,
        scores: &[NodeScore],
        start: usize,
        limit: usize,
        at_limit: EndReason,
    ) -> (usize, EndReason) {
        let cap = (start + self.config.scope.proof_max_nodes.max(1)).min(limit);
        match (start..cap).find(|&j| scores[j].features.qed) {
            Some(j) => (j + 1, EndReason::Qed),
            None if cap == limit => (cap, at_limit),
            None => (cap, EndReason::SizeLimit),
        }
    }

    pub fn statement_end(
        &self,
        nodes: &[Node],
        scores: &[NodeScore],
        start: usize,
        limit: usize,
        at_limit: EndReason,
    ) -> (usize, EndReason) {
        let cap = (start + self.config.scope.statement_max_nodes.max(1)).min(limit);
        let mut needs_body = scores[start]
            .features
            .lead
            .as_ref()
            .is_some_and(|hit| hit.body_empty);
        let mut end = start + 1;
        while end < cap {
            let take = match nodes[end].kind {
                NodeKind::DisplayMath
                | NodeKind::List
                | NodeKind::ListItem
                | NodeKind::Quote
                | NodeKind::Table => true,
                NodeKind::Paragraph => {
                    needs_body
                        || scores[end].features.continuation
                        || nodes[end - 1].flat.trim_end().ends_with([':', '،', ','])
                }
                _ => false,
            };
            if !take {
                return (end, EndReason::NoContinuation);
            }
            if nodes[end].kind == NodeKind::Paragraph {
                needs_body = false;
            }
            end += 1;
        }
        if cap == limit {
            (end, at_limit)
        } else {
            (end, EndReason::SizeLimit)
        }
    }
}

impl SequenceTagger for HeuristicTagger {
    fn tag(&self, nodes: &[Node], features: &[NodeFeatures]) -> Vec<TaggedNode> {
        let n = nodes.len();
        if n == 0 {
            return Vec::new();
        }
        let scores: Vec<NodeScore> = nodes
            .iter()
            .zip(features)
            .map(|(node, feat)| self.score_node_with_features(node, feat))
            .collect();

        let mut tags = vec![
            TaggedNode {
                tag: BioTag::Outside,
                confidence: 1.0,
            };
            n
        ];

        let mut covered = vec![false; n];
        let mut env_first = vec![false; n];
        let mut i = 0;
        while i < n {
            match &nodes[i].env {
                Some(env) => {
                    let end = (i + env.len.max(1)).min(n);
                    env_first[i] = true;
                    covered[i + 1..end].fill(true);
                    tags[i] = TaggedNode {
                        tag: BioTag::Begin(env.kind),
                        confidence: 1.0,
                    };
                    for tag in &mut tags[i + 1..end] {
                        *tag = TaggedNode {
                            tag: BioTag::Inside(env.kind),
                            confidence: 1.0,
                        };
                    }
                    i = end;
                }
                _ => i += 1,
            }
        }

        let is_heading = |i: usize| matches!(nodes[i].kind, NodeKind::Heading(_)) && !covered[i];
        let base_opens = |i: usize| !covered[i] && (env_first[i] || scores[i].opens);
        let boundary = |i: usize| base_opens(i) || is_heading(i);

        let mut prev_statement = false;
        let mut i = 0;
        while i < n {
            if env_first[i] {
                let env = nodes[i].env.as_ref().unwrap();
                prev_statement = env.kind.is_statement();
                let end = (i + env.len.max(1)).min(n);
                i = end;
                continue;
            }

            let limit = (i + 1..n).find(|&j| boundary(j)).unwrap_or(n);
            let at_limit = if limit == n {
                EndReason::EndOfDocument
            } else if base_opens(limit) {
                EndReason::NextOpener
            } else {
                EndReason::Heading
            };

            let bulleted = nodes[i].kind == NodeKind::ListItem;
            let (scope_limit, at_scope_limit) = if let Some(separator) = (i + 1..limit).find(|&j| {
                nodes[j].kind == NodeKind::Rule || (bulleted && nodes[j].kind == NodeKind::ListItem)
            }) {
                (separator, EndReason::NoContinuation)
            } else {
                (limit, at_limit)
            };

            if base_opens(i) {
                let (kind, score) = scores[i].best;
                let natural = match kind {
                    BlockType::Proof => {
                        self.argument_end(&scores, i, scope_limit, at_scope_limit).0
                    }
                    BlockType::Solution => {
                        match self.argument_end(&scores, i, scope_limit, at_scope_limit) {
                            (end, EndReason::Qed) => end,
                            _ => {
                                self.statement_end(nodes, &scores, i, scope_limit, at_scope_limit)
                                    .0
                            }
                        }
                    }
                    _ => {
                        self.statement_end(nodes, &scores, i, scope_limit, at_scope_limit)
                            .0
                    }
                };
                tags[i] = TaggedNode {
                    tag: BioTag::Begin(kind),
                    confidence: score,
                };
                for tag in &mut tags[i + 1..natural] {
                    *tag = TaggedNode {
                        tag: BioTag::Inside(kind),
                        confidence: score,
                    };
                }
                prev_statement = kind.is_statement();
                i = natural;
            } else if is_heading(i) {
                prev_statement = false;
                i += 1;
            } else if let Some(score) =
                self.implicit_proof_score(nodes, &scores, prev_statement, i, scope_limit)
            {
                let natural = self.argument_end(&scores, i, scope_limit, at_scope_limit).0;
                tags[i] = TaggedNode {
                    tag: BioTag::Begin(BlockType::Proof),
                    confidence: score,
                };
                for tag in &mut tags[i + 1..natural] {
                    *tag = TaggedNode {
                        tag: BioTag::Inside(BlockType::Proof),
                        confidence: score,
                    };
                }
                prev_statement = false;
                i = natural;
            } else {
                prev_statement = false;
                i = limit;
            }
        }

        tags
    }
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
