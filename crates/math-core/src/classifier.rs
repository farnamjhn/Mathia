//! Deterministic block classification.
//!
//! Two stages, both pure:
//!
//! 1. **Scoring** — every node gets a weighted feature score per [`BlockType`];
//!    a node whose best score reaches [`ScoringConfig::threshold`] is an
//!    *opener*. LaTeX theorem environments are openers with score 1.
//! 2. **Scoping** — each opener absorbs the nodes after it until a terminator
//!    (QED marker, next opener, heading, or a size limit); an environment
//!    covers exactly its body.
//!
//! Afterwards proofs are linked to the statements they prove and textual
//! references ("by Theorem 3.3") become links.
//!
//! Scope is the part a later decision model may disagree with, so it is
//! adjustable from outside through [`ScopeOverrides`] without touching the
//! scoring: see [`BlockJudge`] and [`HeuristicClassifier::classify_with_judge`].

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::config::ScoringConfig;
use crate::crf::CrfTagger;
use crate::crossref;
use crate::features::{Features, Lexicon, NodeFeatures};
use crate::model::{
    BlockType, Direction, EndReason, Node, NodeKind, SemanticBlock, document_direction,
};
use crate::tagger::{BioTag, HeuristicTagger, SequenceTagger, TaggedNode};
use crate::text::fold_digits;

/// Scoring result for one node.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeScore {
    pub features: Features,
    /// The score of every semantic type, in [`BlockType::SEMANTIC`] order.
    pub scores: Vec<(BlockType, f32)>,
    /// Best semantic type and its score, whether or not it passes the threshold.
    pub best: (BlockType, f32),
    /// The node may open a block: right kind of node and score ≥ threshold.
    pub opens: bool,
}

impl NodeScore {
    /// This node's score for `kind` (0 for non-semantic types).
    pub fn score_of(&self, kind: BlockType) -> f32 {
        self.scores
            .iter()
            .find(|(k, _)| *k == kind)
            .map_or(0.0, |(_, s)| *s)
    }
}

/// Blocks plus everything needed to explain them.
#[derive(Debug, Clone, PartialEq)]
pub struct Classification {
    pub blocks: Vec<SemanticBlock>,
    /// One entry per node.
    pub scores: Vec<NodeScore>,
    /// For every node, the index in `blocks` of the block that contains it.
    pub block_of: Vec<usize>,
}

/// Content-based anchor for a node: `(hash(normalized_first_64_chars), occurrence_index)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ContentAnchor {
    pub hash: u64,
    pub occurrence: usize,
}

/// Normalizes text and computes a 64-bit deterministic hash of the first 64 characters.
pub fn compute_text_anchor_hash(text: &str) -> u64 {
    let normalized = crate::text::normalize(text);
    let trimmed = normalized.trim();
    let first_64: String = trimmed.chars().take(64).collect();
    let digest = crate::sha256::sha256(first_64.as_bytes());
    u64::from_be_bytes(digest[..8].try_into().unwrap())
}

/// Computes content anchors for all nodes in `nodes`.
pub fn compute_node_anchors(nodes: &[Node]) -> Vec<ContentAnchor> {
    let mut counts: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
    nodes
        .iter()
        .map(|node| {
            let hash = compute_text_anchor_hash(&node.flat);
            let count = counts.entry(hash).or_insert(0);
            let occurrence = *count;
            *count += 1;
            ContentAnchor { hash, occurrence }
        })
        .collect()
}

/// Computes the content anchor for the node at `index` in `nodes`.
pub fn compute_node_anchor(nodes: &[Node], index: usize) -> ContentAnchor {
    let target_hash = compute_text_anchor_hash(&nodes[index].flat);
    let occurrence = nodes[..index]
        .iter()
        .filter(|n| compute_text_anchor_hash(&n.flat) == target_hash)
        .count();
    ContentAnchor {
        hash: target_hash,
        occurrence,
    }
}

/// Key identifying a block opener node in [`ScopeOverrides`].
///
/// Can identify a node by a content-based anchor (stable across edits above the
/// block), by a fallback node index, or both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OverrideKey {
    pub anchor: Option<ContentAnchor>,
    pub fallback: Option<usize>,
}

impl OverrideKey {
    pub fn index(index: usize) -> Self {
        OverrideKey {
            anchor: None,
            fallback: Some(index),
        }
    }

    pub fn anchor(hash: u64, occurrence: usize) -> Self {
        OverrideKey {
            anchor: Some(ContentAnchor { hash, occurrence }),
            fallback: None,
        }
    }

    pub fn anchored(hash: u64, occurrence: usize, fallback: usize) -> Self {
        OverrideKey {
            anchor: Some(ContentAnchor { hash, occurrence }),
            fallback: Some(fallback),
        }
    }

    pub fn for_node(nodes: &[Node], index: usize) -> Self {
        if index >= nodes.len() {
            return Self::index(index);
        }
        let anchor = compute_node_anchor(nodes, index);
        OverrideKey {
            anchor: Some(anchor),
            fallback: Some(index),
        }
    }
}

impl From<usize> for OverrideKey {
    fn from(index: usize) -> Self {
        OverrideKey::index(index)
    }
}

impl std::fmt::Display for OverrideKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.anchor, self.fallback) {
            (Some(a), Some(fb)) => write!(f, "{:016x}:{}:{}", a.hash, a.occurrence, fb),
            (Some(a), None) => write!(f, "{:016x}:{}", a.hash, a.occurrence),
            (None, Some(fb)) => write!(f, "{}", fb),
            (None, None) => write!(f, "0"),
        }
    }
}

impl std::str::FromStr for OverrideKey {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(':').collect();
        match parts.as_slice() {
            [p] => {
                if let Ok(idx) = p.parse::<usize>() {
                    Ok(OverrideKey::index(idx))
                } else if let Ok(hash) = u64::from_str_radix(p, 16) {
                    Ok(OverrideKey::anchor(hash, 0))
                } else {
                    Err(format!("invalid override key: '{s}'"))
                }
            }
            [h, occ] => {
                let hash = u64::from_str_radix(h, 16)
                    .or_else(|_| h.parse::<u64>())
                    .map_err(|_| format!("invalid anchor hash in override key: '{s}'"))?;
                let occurrence = occ
                    .parse::<usize>()
                    .map_err(|_| format!("invalid occurrence in override key: '{s}'"))?;
                Ok(OverrideKey::anchor(hash, occurrence))
            }
            [h, occ, fb] => {
                let hash = u64::from_str_radix(h, 16)
                    .or_else(|_| h.parse::<u64>())
                    .map_err(|_| format!("invalid anchor hash in override key: '{s}'"))?;
                let occurrence = occ
                    .parse::<usize>()
                    .map_err(|_| format!("invalid occurrence in override key: '{s}'"))?;
                let fallback = fb
                    .parse::<usize>()
                    .map_err(|_| format!("invalid fallback index in override key: '{s}'"))?;
                Ok(OverrideKey {
                    anchor: Some(ContentAnchor { hash, occurrence }),
                    fallback: Some(fallback),
                })
            }
            _ => Err(format!("invalid override key format: '{s}'")),
        }
    }
}

impl Serialize for OverrideKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for OverrideKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct OverrideKeyVisitor;

        impl<'de> serde::de::Visitor<'de> for OverrideKeyVisitor {
            type Value = OverrideKey;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a node index integer or anchor string")
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(OverrideKey::index(v as usize))
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(OverrideKey::index(v.max(0) as usize))
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::str::FromStr;
                OverrideKey::from_str(v).map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_any(OverrideKeyVisitor)
    }
}

/// Node-index-resolved view of [`ScopeOverrides`] against a specific slice of [`Node`]s.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedOverrides {
    pub delta: BTreeMap<usize, isize>,
    pub suppressed: BTreeSet<usize>,
    pub retype: BTreeMap<usize, BlockType>,
    pub end: BTreeMap<usize, usize>,
}

/// Externally requested corrections to the default classification, keyed by
/// content-based anchor (or fallback node index) of a block's opener. An empty
/// value reproduces [`HeuristicClassifier::classify`]. Serialisable, so a set of
/// corrections (made by hand or by a [`BlockJudge`]) can be stored and replayed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScopeOverrides {
    /// Nodes to add to (positive) or drop from (negative) the block's end.
    /// Ignored for LaTeX environments, whose extent is explicit, and for
    /// blocks with an explicit [`Self::end`].
    pub delta: BTreeMap<OverrideKey, isize>,
    /// Opener nodes to treat as ordinary content instead.
    pub suppressed: BTreeSet<OverrideKey>,
    /// The block opened at this node has this type instead of its best
    /// score. A node that does not open a block on its own opens one of this
    /// type. Must be a semantic type.
    pub retype: BTreeMap<OverrideKey, BlockType>,
    /// Exclusive end node of the block opened here. Unlike `delta`, it may
    /// run through later openers (they become part of the block); headings
    /// and LaTeX environments still stop it.
    pub end: BTreeMap<OverrideKey, usize>,
}

impl ScopeOverrides {
    pub fn is_empty(&self) -> bool {
        self.delta.is_empty()
            && self.suppressed.is_empty()
            && self.retype.is_empty()
            && self.end.is_empty()
    }

    /// Resolves content-based anchors and fallback node indices against `nodes`.
    pub fn resolve(&self, nodes: &[Node]) -> ResolvedOverrides {
        let anchors = if nodes.is_empty() {
            Vec::new()
        } else {
            compute_node_anchors(nodes)
        };
        let resolve_key = |key: &OverrideKey| -> Option<usize> {
            if !nodes.is_empty()
                && let Some(anchor) = key.anchor
                && let Some(pos) = anchors.iter().position(|a| *a == anchor)
            {
                return Some(pos);
            }
            if let Some(fb) = key.fallback
                && (nodes.is_empty() || fb < nodes.len())
            {
                return Some(fb);
            }
            None
        };
        let mut resolved = ResolvedOverrides::default();
        for (key, &val) in &self.delta {
            if let Some(idx) = resolve_key(key) {
                resolved.delta.insert(idx, val);
            }
        }
        for key in &self.suppressed {
            if let Some(idx) = resolve_key(key) {
                resolved.suppressed.insert(idx);
            }
        }
        for (key, &val) in &self.retype {
            if let Some(idx) = resolve_key(key) {
                resolved.retype.insert(idx, val);
            }
        }
        for (key, &val) in &self.end {
            if let Some(idx) = resolve_key(key) {
                let shifted_end =
                    if let (Some(anchor), Some(old_start)) = (key.anchor, key.fallback) {
                        if let Some(pos) = anchors.iter().position(|a| *a == anchor) {
                            let shift = pos as isize - old_start as isize;
                            (val as isize + shift).max(0) as usize
                        } else {
                            val
                        }
                    } else {
                        val
                    };
                resolved.end.insert(idx, shifted_end);
            }
        }
        resolved
    }

    /// Drops every correction for the block opened at `opener`.
    pub fn clear(&mut self, opener: usize) {
        self.clear_key(&OverrideKey::from(opener));
    }

    /// Drops every correction matching `key`.
    pub fn clear_key(&mut self, key: &OverrideKey) {
        let matches = |k: &OverrideKey| {
            k == key
                || (k.anchor.is_some() && k.anchor == key.anchor)
                || (k.fallback.is_some() && k.fallback == key.fallback)
        };
        self.delta.retain(|k, _| !matches(k));
        self.suppressed.retain(|k| !matches(k));
        self.retype.retain(|k, _| !matches(k));
        self.end.retain(|k, _| !matches(k));
    }

    /// Checks the corrections against a document of `nodes` nodes.
    pub fn validate(&self, nodes: usize) -> std::result::Result<(), String> {
        let check = |key: &OverrideKey, what: &str| {
            if let Some(i) = key.fallback
                && i >= nodes
                && key.anchor.is_none()
            {
                return Err(format!("{what}: node {i} is past the end ({nodes} nodes)"));
            }
            Ok(())
        };
        for key in self.delta.keys() {
            check(key, "override")?;
        }
        for key in &self.suppressed {
            check(key, "override")?;
        }
        for (key, kind) in &self.retype {
            check(key, "retype")?;
            if !kind.is_semantic() {
                return Err(format!("retype: `{}` is not a block type", kind.slug()));
            }
        }
        for (key, &end) in &self.end {
            check(key, "end")?;
            if let Some(i) = key.fallback
                && (end <= i || end > nodes)
                && key.anchor.is_none()
            {
                return Err(format!("end: block at node {i} cannot end at {end}"));
            }
        }
        Ok(())
    }
}

/// What a decision model thinks of one proposed block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// The block is a complete unit of its type.
    Accept,
    /// It stops too early: widen the scope by one node and ask again.
    Incomplete,
    /// It runs on into unrelated content: narrow the scope by one node.
    Overreach,
    /// It is not a block of this type at all: drop the opener.
    Reject,
    /// It is a block, of this other type.
    Retype(BlockType),
    /// It ends right before this node (absolute index, exclusive): a split
    /// point when inside the block, or a wider scope that may absorb later
    /// openers.
    EndAt(usize),
    /// It really starts at this node (absolute index); the end stays put.
    StartAt(usize),
}

/// Seam for decision models. `math-core` only defines the contract; the
/// implementations (local models, remote calls) live outside this crate.
pub trait BlockJudge {
    fn judge(&self, block: &SemanticBlock, nodes: &[Node]) -> Verdict;

    /// Judges several blocks at once; one call per round of
    /// [`HeuristicClassifier::classify_with_judge`]. Override to batch
    /// requests to a model. Must return one verdict per block, in order.
    fn judge_batch(&self, blocks: &[&SemanticBlock], nodes: &[Node]) -> Vec<Verdict> {
        blocks
            .iter()
            .map(|block| self.judge(block, nodes))
            .collect()
    }
}

/// One verdict and what became of it, for logs and replays.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppliedVerdict {
    /// 1-based round of the judge loop.
    pub round: usize,
    pub block: String,
    pub kind: BlockType,
    pub span: Range<usize>,
    pub verdict: Verdict,
    /// `false` when the verdict was not usable (an out-of-range node, a
    /// non-semantic type, a scope change asked of a LaTeX environment).
    pub applied: bool,
}

/// The outcome of [`HeuristicClassifier::classify_with_judge_detailed`]:
/// `overrides` alone reproduces `blocks` through
/// [`HeuristicClassifier::classify_with_overrides`], without the judge.
#[derive(Debug, Clone, PartialEq)]
pub struct JudgedClassification {
    pub blocks: Vec<SemanticBlock>,
    pub overrides: ScopeOverrides,
    pub log: Vec<AppliedVerdict>,
    pub rounds: usize,
}

#[derive(Debug, Clone)]
pub struct HeuristicClassifier {
    config: ScoringConfig,
    lexicon: Lexicon,
    tagger: HeuristicTagger,
    crf: CrfTagger,
}

impl Default for HeuristicClassifier {
    fn default() -> Self {
        Self::new(ScoringConfig::default())
    }
}

impl HeuristicClassifier {
    pub fn new(config: ScoringConfig) -> Self {
        let lexicon = Lexicon::new(&config);
        let tagger = HeuristicTagger::new(config.clone(), lexicon.clone());
        let crf = CrfTagger::new();
        HeuristicClassifier {
            config,
            lexicon,
            tagger,
            crf,
        }
    }

    pub fn config(&self) -> &ScoringConfig {
        &self.config
    }

    pub fn lexicon(&self) -> &Lexicon {
        &self.lexicon
    }

    pub fn tagger(&self) -> &HeuristicTagger {
        &self.tagger
    }

    pub fn crf(&self) -> &CrfTagger {
        &self.crf
    }

    /// Classifies nodes using the pure-Rust CRF Viterbi tagger.
    pub fn classify_with_crf(&self, nodes: &[Node], overrides: &ScopeOverrides) -> Classification {
        self.classify_with_tagger(nodes, overrides, &self.crf)
    }

    /// Score of `features` for one candidate type, in `[0, 1]`, by the
    /// configured [`Scorer`].
    pub fn score(&self, features: &Features, kind: BlockType) -> f32 {
        self.tagger.score(features, kind)
    }

    /// Weighted sum of the features for one candidate type, in `[0, 1]`.
    pub fn linear_score(&self, features: &Features, kind: BlockType) -> f32 {
        self.tagger.linear_score(features, kind)
    }

    pub fn score_node(&self, node: &Node) -> NodeScore {
        self.tagger.score_node(node)
    }

    /// Classifies with the default scoping.
    pub fn classify(&self, nodes: &[Node]) -> Vec<SemanticBlock> {
        self.classify_with_overrides(nodes, &ScopeOverrides::default())
    }

    pub fn classify_with_overrides(
        &self,
        nodes: &[Node],
        overrides: &ScopeOverrides,
    ) -> Vec<SemanticBlock> {
        self.classify_detailed(nodes, overrides).blocks
    }

    /// [`Self::classify_with_overrides`], keeping the per-node scores.
    pub fn classify_detailed(&self, nodes: &[Node], overrides: &ScopeOverrides) -> Classification {
        self.classify_with_tagger(nodes, overrides, &self.tagger)
    }

    /// Classifies nodes using an arbitrary [`SequenceTagger`] to predict BIO tags.
    pub fn classify_with_tagger(
        &self,
        nodes: &[Node],
        overrides: &ScopeOverrides,
        tagger: &dyn SequenceTagger,
    ) -> Classification {
        let features: Vec<NodeFeatures> = nodes
            .iter()
            .map(|node| self.lexicon.extract(node))
            .collect();
        let scores: Vec<NodeScore> = nodes
            .iter()
            .zip(&features)
            .map(|(node, feat)| self.tagger.score_node_with_features(node, feat))
            .collect();
        let tags = tagger.tag(nodes, &features);
        self.tags_to_blocks(nodes, &tags, &scores, overrides)
    }

    /// Reconstructs [`SemanticBlock`] spans from BIO sequence tags while preserving
    /// [`EnvSpan`] overrides, manual [`ScopeOverrides`], proof argument links,
    /// and cross-references.
    pub fn tags_to_blocks(
        &self,
        nodes: &[Node],
        tags: &[TaggedNode],
        scores: &[NodeScore],
        overrides: &ScopeOverrides,
    ) -> Classification {
        let n = nodes.len();
        let resolved = overrides.resolve(nodes);

        // LaTeX environments: `env_first` opens, `covered` belongs to one.
        let mut env_first = vec![false; n];
        let mut covered = vec![false; n];
        let mut i = 0;
        while i < n {
            match &nodes[i].env {
                Some(env) if !resolved.suppressed.contains(&i) => {
                    let end = (i + env.len.max(1)).min(n);
                    env_first[i] = true;
                    covered[i + 1..end].fill(true);
                    i = end;
                }
                _ => i += 1,
            }
        }

        let retype = |i: usize| {
            resolved
                .retype
                .get(&i)
                .copied()
                .filter(|kind| kind.is_semantic())
        };
        let base_opens = |i: usize| {
            !covered[i]
                && (env_first[i]
                    || (!resolved.suppressed.contains(&i)
                        && (retype(i).is_some() || tags.get(i).is_some_and(|t| t.tag.is_begin()))))
        };
        let is_heading = |i: usize| matches!(nodes[i].kind, NodeKind::Heading(_)) && !covered[i];
        // An explicit end swallows the openers before it, up to a heading or
        // an environment.
        let mut absorbed = vec![false; n];
        for (&i, &end) in &resolved.end {
            if i >= n || absorbed[i] || env_first[i] || !base_opens(i) {
                continue;
            }
            for j in i + 1..end.min(n) {
                if is_heading(j) || env_first[j] || covered[j] {
                    break;
                }
                absorbed[j] = true;
            }
        }
        let opens = |i: usize| !absorbed[i] && base_opens(i);
        let boundary = |i: usize| opens(i) || is_heading(i);

        let mut builder = Builder::new(nodes);
        let mut i = 0;
        while i < n {
            // Nothing absorbs past the next opener or heading.
            let limit = (i + 1..n).find(|&j| boundary(j)).unwrap_or(n);
            let at_limit = if limit == n {
                EndReason::EndOfDocument
            } else if opens(limit) {
                EndReason::NextOpener
            } else {
                EndReason::Heading
            };
            // A thematic break separates content. A new top-level item
            // also separates a bulleted block, whose nested steps are
            // already part of its opener.
            let bulleted = nodes[i].kind == NodeKind::ListItem;
            let (scope_limit, at_scope_limit) = if let Some(separator) = (i + 1..limit).find(|&j| {
                nodes[j].kind == NodeKind::Rule || (bulleted && nodes[j].kind == NodeKind::ListItem)
            }) {
                (separator, EndReason::NoContinuation)
            } else {
                (limit, at_limit)
            };
            let adjust = |natural: usize, reason: EndReason| {
                let end = match resolved.end.get(&i) {
                    Some(&end) => end.clamp(i + 1, limit),
                    None => {
                        let delta = resolved.delta.get(&i).copied().unwrap_or(0);
                        (natural as isize + delta).clamp(i as isize + 1, limit as isize) as usize
                    }
                };
                (
                    end,
                    if end == natural {
                        reason
                    } else {
                        EndReason::Override
                    },
                )
            };
            let features = Some(&scores[i].features);

            if env_first[i] {
                let env = nodes[i].env.as_ref().expect("env_first implies env");
                let end = (i + env.len.max(1)).min(n);
                let opener = Opener {
                    score: 1.0,
                    features,
                    env_name: env.name.as_deref(),
                    retyped: retype(i).is_some(),
                };
                let kind = retype(i).unwrap_or(env.kind);
                builder.push(kind, i..end, opener, EndReason::Environment);
                i = end;
            } else if opens(i) {
                let kind = match retype(i) {
                    Some(kind) => kind,
                    None => tags
                        .get(i)
                        .and_then(|t| t.tag.block_type())
                        .unwrap_or(scores[i].best.0),
                };
                let score = if retype(i).is_some() {
                    scores[i].score_of(kind)
                } else if let Some(t) = tags.get(i)
                    && t.tag.is_begin()
                {
                    if t.confidence <= 1.0 {
                        t.confidence
                    } else {
                        scores[i].score_of(kind)
                    }
                } else {
                    scores[i].best.1
                };

                let original_kind = tags.get(i).and_then(|t| t.tag.block_type());
                let (natural, reason) = if retype(i).is_some()
                    && (tags.get(i).is_none_or(|t| !t.tag.is_begin())
                        || original_kind != Some(kind))
                {
                    match kind {
                        BlockType::Proof => {
                            self.tagger
                                .argument_end(scores, i, scope_limit, at_scope_limit)
                        }
                        BlockType::Solution => {
                            match self
                                .tagger
                                .argument_end(scores, i, scope_limit, at_scope_limit)
                            {
                                (end, EndReason::Qed) => (end, EndReason::Qed),
                                _ => self.tagger.statement_end(
                                    nodes,
                                    scores,
                                    i,
                                    scope_limit,
                                    at_scope_limit,
                                ),
                            }
                        }
                        _ => {
                            self.tagger
                                .statement_end(nodes, scores, i, scope_limit, at_scope_limit)
                        }
                    }
                } else {
                    let mut end = i + 1;
                    while end < scope_limit
                        && tags.get(end).is_some_and(|t| t.tag == BioTag::Inside(kind))
                    {
                        end += 1;
                    }
                    let reason = if matches!(kind, BlockType::Proof | BlockType::Solution)
                        && (i..end).any(|j| scores[j].features.qed)
                    {
                        EndReason::Qed
                    } else if end
                        == (i + if kind.is_argument() {
                            self.config.scope.proof_max_nodes
                        } else {
                            self.config.scope.statement_max_nodes
                        }
                        .max(1))
                        .min(scope_limit)
                        && end < scope_limit
                    {
                        EndReason::SizeLimit
                    } else if end == scope_limit {
                        if at_scope_limit == EndReason::NextOpener
                            && !scores[end].opens
                            && !env_first[end]
                            && retype(end).is_none()
                        {
                            EndReason::NoContinuation
                        } else {
                            at_scope_limit
                        }
                    } else {
                        EndReason::NoContinuation
                    };
                    (end, reason)
                };

                let (end, reason) = adjust(natural, reason);
                let opener = Opener {
                    score,
                    features,
                    env_name: None,
                    retyped: retype(i).is_some(),
                };
                builder.push(kind, i..end, opener, reason);
                i = end;
            } else if is_heading(i) {
                builder.push(
                    BlockType::Heading,
                    i..i + 1,
                    Opener::NONE,
                    EndReason::Structural,
                );
                i += 1;
            } else {
                builder.push(
                    BlockType::Text,
                    i..limit,
                    Opener::NONE,
                    EndReason::Structural,
                );
                i = limit;
            }
        }

        let mut blocks = builder.blocks;
        link_arguments(&mut blocks);
        crossref::link_references(&mut blocks, &self.lexicon);

        let mut block_of = vec![0; n];
        for (index, block) in blocks.iter().enumerate() {
            block_of[block.span.clone()].fill(index);
        }
        Classification {
            blocks,
            scores: scores.to_vec(),
            block_of,
        }
    }

    /// Classify, let `judge` review every semantic block, apply its verdicts
    /// as scope overrides and re-run — at most `max_rounds` times. The result
    /// is deterministic whenever the judge is.
    pub fn classify_with_judge(
        &self,
        nodes: &[Node],
        judge: &dyn BlockJudge,
        max_rounds: usize,
    ) -> Vec<SemanticBlock> {
        self.classify_with_judge_detailed(nodes, judge, max_rounds, ScopeOverrides::default())
            .blocks
    }

    /// [`Self::classify_with_judge`] starting from `initial` overrides (for
    /// example corrections made by hand), returning the final overrides and
    /// a log of every verdict.
    ///
    /// Policy: a block is judged once per distinct (scope, type); a state
    /// that comes back — the judge contradicting itself, or a change blocked
    /// by a heading — is not asked about again, so the loop always ends.
    pub fn classify_with_judge_detailed(
        &self,
        nodes: &[Node],
        judge: &dyn BlockJudge,
        max_rounds: usize,
        initial: ScopeOverrides,
    ) -> JudgedClassification {
        let resolved_initial = initial.resolve(nodes);
        let mut overrides = initial;
        let mut judged: BTreeSet<(usize, usize, BlockType)> = BTreeSet::new();
        let mut log = Vec::new();
        let mut rounds = 0;

        // Step 1: Run EnvSpan bypass first, then CrfTagger across the document
        let features: Vec<NodeFeatures> = nodes.iter().map(|n| self.lexicon.extract(n)).collect();
        let crf_tags = self.crf.tag(nodes, &features);
        let scores: Vec<NodeScore> = nodes
            .iter()
            .zip(&features)
            .map(|(node, feat)| self.tagger.score_node_with_features(node, feat))
            .collect();

        let mut blocks = self
            .tags_to_blocks(nodes, &crf_tags, &scores, &overrides)
            .blocks;

        while rounds < max_rounds {
            let pending: Vec<&SemanticBlock> = blocks
                .iter()
                .filter(|b| b.kind.is_semantic())
                .filter(|b| !judged.contains(&(b.span.start, b.span.end, b.kind)))
                .filter(|b| {
                    // EnvSpan bypass: LaTeX environments have score 1.0 and exact spans;
                    // never escalate them to math-judge.
                    if b.end == EndReason::Environment
                        || nodes
                            .get(b.span.start)
                            .and_then(|n| n.env.as_ref())
                            .is_some()
                    {
                        return false;
                    }

                    // ScopeOverrides always win over both CrfTagger and math-judge.
                    // If an opener was manually overridden in initial by the user, do not escalate.
                    if resolved_initial.end.contains_key(&b.span.start)
                        || resolved_initial.retype.contains_key(&b.span.start)
                        || resolved_initial.delta.contains_key(&b.span.start)
                        || resolved_initial.suppressed.contains(&b.span.start)
                    {
                        return false;
                    }

                    // Confidence Gating:
                    // 1. CrfTagger confidence margin falls below config.ml_confidence_threshold
                    let low_confidence = (b.span.start..b.span.end.min(nodes.len())).any(|idx| {
                        crf_tags
                            .get(idx)
                            .is_some_and(|t| t.confidence < self.config.ml_confidence_threshold)
                    }) || crf_tags
                        .get(b.span.end)
                        .is_some_and(|t| t.confidence < self.config.ml_confidence_threshold);

                    // 2. An Outside paragraph with high logic_density immediately follows a Theorem or Lemma
                    let logic_after_thm = matches!(b.kind, BlockType::Theorem | BlockType::Lemma)
                        && b.span.end < nodes.len()
                        && matches!(nodes[b.span.end].kind, NodeKind::Paragraph)
                        && crf_tags
                            .get(b.span.end)
                            .is_some_and(|t| t.tag == BioTag::Outside)
                        && (crate::crf::bucket_logic_density(features[b.span.end].logic_density)
                            == "high"
                            || features[b.span.end].logic_density >= 0.75);

                    low_confidence || logic_after_thm
                })
                .collect();

            if pending.is_empty() {
                break;
            }
            rounds += 1;
            let verdicts = judge.judge_batch(&pending, nodes);
            let before = overrides.clone();
            for (block, &verdict) in pending.iter().zip(&verdicts) {
                judged.insert((block.span.start, block.span.end, block.kind));
                let applied = apply_verdict(&mut overrides, block, verdict, nodes);
                log.push(AppliedVerdict {
                    round: rounds,
                    block: block.id.clone(),
                    kind: block.kind,
                    span: block.span.clone(),
                    verdict,
                    applied,
                });
            }
            let changed = overrides != before;
            if !changed {
                break;
            }
            blocks = self
                .tags_to_blocks(nodes, &crf_tags, &scores, &overrides)
                .blocks;
        }
        JudgedClassification {
            blocks,
            overrides,
            log,
            rounds,
        }
    }
}

/// Turns one verdict into overrides. Returns whether it could be applied.
fn apply_verdict(
    overrides: &mut ScopeOverrides,
    block: &SemanticBlock,
    verdict: Verdict,
    nodes: &[Node],
) -> bool {
    let opener_idx = block.span.start;
    let opener = OverrideKey::for_node(nodes, opener_idx);
    let environment = block.end == EndReason::Environment;
    let mut nudge = |by: isize| {
        let existing_end = overrides
            .end
            .iter()
            .find(|(k, _)| {
                **k == opener
                    || (k.anchor.is_some() && k.anchor == opener.anchor)
                    || (k.fallback.is_some() && k.fallback == opener.fallback)
            })
            .map(|(k, _)| *k);
        if let Some(k) = existing_end {
            let end = overrides.end.get_mut(&k).unwrap();
            *end = end.saturating_add_signed(by).max(opener_idx + 1);
            true
        } else {
            let existing_delta = overrides
                .delta
                .iter()
                .find(|(k, _)| {
                    **k == opener
                        || (k.anchor.is_some() && k.anchor == opener.anchor)
                        || (k.fallback.is_some() && k.fallback == opener.fallback)
                })
                .map(|(k, _)| *k);
            if let Some(k) = existing_delta {
                *overrides.delta.get_mut(&k).unwrap() += by;
            } else {
                overrides.delta.insert(opener, by);
            }
            true
        }
    };
    match verdict {
        Verdict::Accept => true,
        Verdict::Incomplete | Verdict::Overreach if environment => false,
        Verdict::Incomplete => nudge(1),
        Verdict::Overreach => nudge(-1),
        Verdict::Reject => {
            overrides.clear_key(&opener);
            overrides.suppressed.insert(opener);
            true
        }
        Verdict::Retype(kind) if kind.is_semantic() => {
            if kind != block.kind {
                overrides.retype.insert(opener, kind);
            }
            true
        }
        Verdict::Retype(_) => false,
        Verdict::EndAt(end) if !environment && end > opener_idx && end <= nodes.len() => {
            overrides.delta.remove(&opener);
            overrides.end.insert(opener, end);
            true
        }
        Verdict::EndAt(_) => false,
        Verdict::StartAt(start) if start == opener_idx => true,
        Verdict::StartAt(start) if !environment && start < block.span.end => {
            let end = block.span.end;
            let start_key = OverrideKey::for_node(nodes, start);
            overrides.clear_key(&opener);
            overrides.suppressed.insert(opener);
            overrides.clear_key(&start_key);
            overrides.retype.insert(start_key, block.kind);
            overrides.end.insert(start_key, end);
            true
        }
        Verdict::StartAt(_) => false,
    }
}

/// Links every proof to the statement it proves and every solution to the
/// example or exercise it answers: by an explicit reference in the opener
/// ("Proof of Theorem 2"), otherwise by adjacency (plain text in between is
/// allowed, anything else is not).
fn link_arguments(blocks: &mut [SemanticBlock]) {
    for index in 0..blocks.len() {
        let block = &blocks[index];
        if !block.kind.is_argument() {
            continue;
        }
        let before = &blocks[..index];
        let referred = block
            .features
            .as_ref()
            .and_then(|f| f.lead.as_ref())
            .and_then(|hit| hit.refers_to.as_ref())
            .and_then(|target| {
                let number = target.number.as_deref().map(fold_digits);
                before.iter().rev().find(|b| {
                    b.kind == target.kind
                        && (number.is_none() || b.number.as_deref().map(fold_digits) == number)
                })
            });
        let adjacent = || {
            let previous = before.iter().rev().find(|b| b.kind != BlockType::Text)?;
            let fits = match block.kind {
                BlockType::Proof => previous.kind.is_statement(),
                _ => matches!(previous.kind, BlockType::Example | BlockType::Exercise),
            };
            fits.then_some(previous)
        };
        let proves = referred.or_else(adjacent).map(|b| b.id.clone());
        blocks[index].proves = proves;
    }
}

/// What made a node open a block.
#[derive(Clone, Copy)]
struct Opener<'a> {
    score: f32,
    features: Option<&'a Features>,
    /// The `[Name]` of a LaTeX environment.
    env_name: Option<&'a str>,
    /// The type was set by a [`ScopeOverrides::retype`]: keep the label's
    /// number and name although its keyword names another type.
    retyped: bool,
}

impl Opener<'_> {
    /// Headings and plain text.
    const NONE: Opener<'static> = Opener {
        score: 0.0,
        features: None,
        env_name: None,
        retyped: false,
    };
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Assigns ids and assembles [`SemanticBlock`]s in document order.
struct Builder<'a> {
    nodes: &'a [Node],
    fallback_dir: Direction,
    counters: BTreeMap<BlockType, usize>,
    blocks: Vec<SemanticBlock>,
}

impl<'a> Builder<'a> {
    fn new(nodes: &'a [Node]) -> Self {
        Builder {
            nodes,
            fallback_dir: document_direction(nodes),
            counters: BTreeMap::new(),
            blocks: Vec::new(),
        }
    }

    fn push(&mut self, kind: BlockType, span: Range<usize>, opener: Opener, end: EndReason) {
        let counter = self.counters.entry(kind).or_insert(0);
        *counter += 1;
        let id = match kind {
            BlockType::Heading => format!("sec-{counter}"),
            _ => format!("block-{}-{counter}", kind.slug()),
        };

        let nodes = &self.nodes[span.clone()];
        let text: String = nodes
            .iter()
            .map(|n| n.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let direction = Direction::detect(&text).unwrap_or(self.fallback_dir);

        let heading_level = match (kind, nodes[0].kind) {
            (BlockType::Heading, NodeKind::Heading(level)) => Some(level),
            _ => None,
        };
        let semantic = kind.is_semantic();
        let lead = opener
            .features
            .and_then(|f| f.lead.as_ref())
            .filter(|hit| semantic && (hit.kind == kind || opener.retyped));
        let number = lead.and_then(|hit| hit.number.clone());
        let name = opener
            .env_name
            .map(str::to_string)
            .or_else(|| lead.and_then(|hit| hit.name.clone()));

        let title = match kind {
            BlockType::Heading => nodes[0].text.trim().to_string(),
            _ => {
                let mut title = kind.label(direction).to_string();
                if let Some(number) = &number {
                    title.push(' ');
                    title.push_str(number);
                }
                if let Some(name) = &name {
                    title.push_str(&format!(" ({name})"));
                }
                title
            }
        };

        let mut html = String::new();
        if let Some(name) = opener.env_name {
            // The environment's own `\begin{…}[Name]` line produced no node.
            html.push_str(&format!(
                "<p class=\"mx-block-title\">{}</p>\n",
                escape_html(name)
            ));
        }
        let mut at = 0;
        while at < nodes.len() {
            let node = &nodes[at];
            if node.kind == NodeKind::ListItem {
                let ordered = node.list.is_some_and(|list| list.ordered);
                let mut end = at + 1;
                while end < nodes.len()
                    && nodes[end].kind == NodeKind::ListItem
                    && nodes[end].list.is_some_and(|list| list.ordered) == ordered
                {
                    end += 1;
                }
                if kind == BlockType::Text {
                    html.push_str(if ordered { "<ol>\n" } else { "<ul>\n" });
                    for item in &nodes[at..end] {
                        html.push_str("<li>");
                        html.push_str(&item.html);
                        html.push_str("</li>\n");
                    }
                    html.push_str(if ordered { "</ol>\n" } else { "</ul>\n" });
                } else {
                    // Inside a semantic block the item is the block's body,
                    // not an entry of a surrounding list.
                    for item in &nodes[at..end] {
                        html.push_str(&item.html);
                    }
                }
                at = end;
                continue;
            }
            match node.kind {
                // A heading that opened a semantic block becomes its title line.
                NodeKind::Heading(_) if kind != BlockType::Heading => {
                    html.push_str(&format!("<p class=\"mx-block-title\">{}</p>\n", node.html));
                }
                _ => html.push_str(&node.html),
            }
            at += 1;
        }

        self.blocks.push(SemanticBlock {
            id,
            kind,
            span,
            heading_level,
            number,
            name,
            title,
            direction,
            score: opener.score,
            features: if semantic {
                opener.features.cloned()
            } else {
                None
            },
            proves: None,
            end,
            html,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Scorer;
    use crate::parser::parse_markdown;

    fn classify(md: &str) -> Vec<SemanticBlock> {
        HeuristicClassifier::default().classify(&parse_markdown(md))
    }

    fn kinds(blocks: &[SemanticBlock]) -> Vec<BlockType> {
        blocks.iter().map(|b| b.kind).collect()
    }

    fn best(md: &str) -> NodeScore {
        HeuristicClassifier::default().score_node(&parse_markdown(md)[0])
    }

    #[test]
    fn labelled_keywords_open_blocks() {
        for (md, kind) in [
            ("**قضیه ۱.** اگر $a>0$ آنگاه $a^2>0$.", BlockType::Theorem),
            (
                "لم ۲: هر دنبالهٔ کراندار زیردنبالهٔ همگرا دارد.",
                BlockType::Lemma,
            ),
            (
                "**تعریف.** مجموعهٔ $A$ را باز گوییم هرگاه ...",
                BlockType::Definition,
            ),
            ("برهان. فرض کنید $x \\in A$.", BlockType::Proof),
            ("اثبات: بدیهی است.", BlockType::Proof),
            ("گزاره ۳. هر گروه دوری آبلی است.", BlockType::Proposition),
            ("نتیجه ۴. پس $f$ کراندار است.", BlockType::Corollary),
            (
                "مثال ۵. تابع $f(x)=x^2$ را در نظر بگیرید.",
                BlockType::Example,
            ),
            ("تذکر. عکس این حکم درست نیست.", BlockType::Remark),
            (
                "Theorem 1. Every bounded sequence has a convergent subsequence.",
                BlockType::Theorem,
            ),
            ("**Proof.** Suppose not.", BlockType::Proof),
            ("Definition 2. A set is open if ...", BlockType::Definition),
            (
                "Lemma 3 (Zorn). Every chain has an upper bound.",
                BlockType::Lemma,
            ),
            ("Corollary. Hence $f$ is bounded.", BlockType::Corollary),
        ] {
            let s = best(md);
            assert!(s.opens, "should open a block: {md} (score {:?})", s.best);
            assert_eq!(s.best.0, kind, "{md}");
        }
    }

    #[test]
    fn prose_that_merely_uses_a_keyword_stays_text() {
        for md in [
            "طبق قضیه ۳.۲ می‌توان نتیجه گرفت که $f$ کراندار است.",
            "نتیجه می‌گیریم که دنباله همگراست.",
            "Note that $f$ is continuous.",
            "Proof techniques vary widely between fields.",
            "By the lemma above, $x \\implies y$ and $\\forall z \\exists w$.",
            "## Definitions and notation",
            "## Proof techniques",
        ] {
            let s = best(md);
            assert!(!s.opens, "should stay text: {md} (score {:?})", s.best);
        }
    }

    #[test]
    fn bulleted_labels_open_blocks() {
        let blocks = classify(
            "- **قضیه ۵.۳.** اگر $W$ زیرفضا باشد آنگاه حکم برقرار است.\n\n\
             - **تعریف ۲.۱.** مجموعهٔ $V$ را برداری گوییم.\n\n\
             1. **Proof.** Suppose not.",
        );
        assert_eq!(
            kinds(&blocks),
            [BlockType::Theorem, BlockType::Definition, BlockType::Proof]
        );
    }

    #[test]
    fn plain_bullet_list_stays_text() {
        let blocks = classify("- یک\n- دو\n\n1. اول\n2. دوم");
        assert_eq!(kinds(&blocks), [BlockType::Text]);
    }

    #[test]
    fn scores_are_weighted_sums() {
        let c = HeuristicClassifier::default();
        let w = &c.config().weights;
        let s = best("**Proof.** Suppose not. Then $a \\implies b$, a contradiction.");
        let expected = w.lead + w.label + w.emphasis + w.logic / 3.0 + w.cue;
        assert_eq!(s.best.0, BlockType::Proof);
        assert!(
            (s.best.1 - expected).abs() < 1e-6,
            "{} vs {expected}",
            s.best.1
        );
    }

    #[test]
    fn threshold_is_configurable() {
        let nodes = parse_markdown("Note that $f$ is continuous.");
        let strict = HeuristicClassifier::default();
        assert_eq!(kinds(&strict.classify(&nodes)), [BlockType::Text]);

        let lenient = HeuristicClassifier::new(ScoringConfig {
            threshold: 0.4,
            ..Default::default()
        });
        assert_eq!(kinds(&lenient.classify(&nodes)), [BlockType::Remark]);
    }

    #[test]
    fn proof_extends_to_its_qed_marker() {
        let blocks = classify(
            "**قضیه ۱.** اگر $f$ پیوسته باشد آنگاه کراندار است.\n\n\
             **برهان.** فرض کنید چنین نباشد.\n\n\
             آنگاه دنباله‌ای داریم که\n$$|f(x_n)| > n.$$\n\n\
             که تناقض است. حکم ثابت شد. $\\blacksquare$\n\n\
             این بند دیگر جزو برهان نیست.",
        );
        assert_eq!(
            kinds(&blocks),
            [BlockType::Theorem, BlockType::Proof, BlockType::Text]
        );
        assert_eq!(blocks[1].span, 1..4);
        assert_eq!(blocks[1].id, "block-proof-1");
    }

    #[test]
    fn statement_absorbs_display_math_and_continuations_only() {
        let blocks = classify(
            "Theorem 2. For every $x$ we have\n\n$$x^2 \\ge 0,$$\n\n\
             where equality holds only at zero.\n\n\
             This paragraph is ordinary discussion.",
        );
        assert_eq!(kinds(&blocks), [BlockType::Theorem, BlockType::Text]);
        assert_eq!(blocks[0].span, 0..3);
    }

    #[test]
    fn heading_openers_take_the_next_paragraph_as_body() {
        let blocks = classify(
            "## قضیه ۲.۱ (رول)\n\nاگر $f(a)=f(b)$ آنگاه $c$ ای هست که $f'(c)=0$.\n\n\
             ## بخش بعد\n\nمتن.",
        );
        assert_eq!(
            kinds(&blocks),
            [BlockType::Theorem, BlockType::Heading, BlockType::Text]
        );
        assert_eq!(blocks[0].span, 0..2);
        assert_eq!(blocks[0].title, "قضیه ۲.۱ (رول)");
        assert!(blocks[0].html.contains("mx-block-title"));
        assert_eq!(blocks[1].id, "sec-1");
    }

    #[test]
    fn ids_are_deterministic_and_counted_per_type() {
        let md = "Lemma 1. A.\n\nProof. B. □\n\nLemma 2. C.\n\nProof. D. □\n\nTheorem 3. E.";
        let ids = |blocks: Vec<SemanticBlock>| blocks.into_iter().map(|b| b.id).collect::<Vec<_>>();
        let first = ids(classify(md));
        assert_eq!(
            first,
            [
                "block-lemma-1",
                "block-proof-1",
                "block-lemma-2",
                "block-proof-2",
                "block-theorem-1"
            ]
        );
        assert_eq!(first, ids(classify(md)));
    }

    #[test]
    fn direction_is_detected_per_block() {
        let blocks = classify("**قضیه ۱.** متن فارسی.\n\nTheorem 2. English text.");
        assert_eq!(blocks[0].direction, Direction::Rtl);
        assert_eq!(blocks[1].direction, Direction::Ltr);
        assert_eq!(blocks[1].title, "Theorem 2");
    }

    #[test]
    fn references_to_a_block_do_not_open_one() {
        for md in [
            "قضیه ۲ نتیجه می‌دهد که برای هر $x$ داریم $f(x) > 0$.",
            "Theorem 2 implies that for all $x$ we have $f(x) > 0$.",
            "Lemma 3 is used twice in what follows.",
            "تمرین ۴ را به عنوان تکلیف حل کنید.",
        ] {
            let s = best(md);
            assert!(!s.opens, "should stay text: {md} (score {:?})", s.best);
        }
        // Emphasis still carries an unpunctuated label.
        assert!(best("**قضیه ۲** اگر $a>0$ آنگاه $a^2>0$.").opens);
    }

    #[test]
    fn exercises_solutions_claims_and_notation() {
        let blocks = classify(
            "Exercise 1. Show that $2$ is prime.\n\n\
             Solution. Its only divisors are $1$ and $2$.\n\n\
             **Claim.** The set is closed.\n\n\
             **Notation.** We write $\\mathbb{N}$ for the naturals.\n\n\
             تمرین ۲. ثابت کنید $۳$ اول است.\n\nحل: بدیهی است.",
        );
        assert!(!best("حل این معادله ساده است.").opens);
        assert_eq!(
            kinds(&blocks),
            [
                BlockType::Exercise,
                BlockType::Solution,
                BlockType::Claim,
                BlockType::Notation,
                BlockType::Exercise,
                BlockType::Solution,
            ]
        );
        assert_eq!(blocks[1].proves.as_deref(), Some("block-exercise-1"));
        assert_eq!(blocks[5].proves.as_deref(), Some("block-exercise-2"));
        assert_eq!(blocks[4].title, "تمرین ۲");

        // Without a QED mark a solution does not swallow the discussion after it.
        let blocks =
            classify("Exercise 1. Do it.\n\nSolution. Done:\n\n$$x = 1$$\n\nUnrelated prose.");
        assert_eq!(blocks[1].span, 1..3);
        assert_eq!(kinds(&blocks)[2], BlockType::Text);
    }

    #[test]
    fn keywordless_proof_after_a_statement() {
        let blocks = classify(
            "Lemma 1. Every $x > 0$ has a square root.\n\n\
             Consider the set $S = \\{t : t^2 \\le x\\}$.\n\n\
             Its supremum works. $\\blacksquare$\n\n\
             Unrelated discussion.",
        );
        assert_eq!(
            kinds(&blocks),
            [BlockType::Lemma, BlockType::Proof, BlockType::Text]
        );
        assert_eq!(blocks[1].span, 1..3);
        assert_eq!(blocks[1].end, EndReason::Qed);
        assert_eq!(blocks[1].proves.as_deref(), Some("block-lemma-1"));
        assert!((blocks[1].score - 0.55).abs() < 1e-6);

        // No QED and no proof profile: ordinary discussion stays text, and a
        // definition is never followed by an implicit proof.
        let blocks = classify("Lemma 1. Statement.\n\nSome discussion follows.");
        assert_eq!(kinds(&blocks), [BlockType::Lemma, BlockType::Text]);
        let blocks = classify("Definition 1. A set is open if so.\n\nThis is fine. □");
        assert_eq!(kinds(&blocks), [BlockType::Definition, BlockType::Text]);
    }

    #[test]
    fn proofs_link_by_reference_before_adjacency() {
        let blocks = classify(
            "Theorem 1. A.\n\nTheorem 2. B.\n\nSome text.\n\n\
             Proof of Theorem 1. Easy. □\n\nProof. Also easy. □\n\n\
             Remark. Unrelated.\n\nProof. Orphan. □",
        );
        let proves: Vec<_> = blocks
            .iter()
            .filter(|b| b.kind == BlockType::Proof)
            .map(|b| b.proves.as_deref())
            .collect();
        assert_eq!(proves, [Some("block-theorem-1"), None, None]);

        let blocks = classify("قضیه ۲.۱. الف.\n\nمتن میانی.\n\nبرهان. ساده است. □");
        assert_eq!(blocks[2].proves.as_deref(), Some("block-theorem-1"));
    }

    #[test]
    fn latex_environments_are_exact_blocks() {
        let md = "\\begin{theorem}[Rolle]\nIf $f(a)=f(b)$ then\n\n$$f'(c)=0.$$\n\n\
                  Proof techniques vary. Note: this stays inside.\n\\end{theorem}\n\n\
                  \\begin{proof}\nFirst step.\n\nSecond step.\n\\end{proof}\n\nAfter.";
        let blocks = classify(md);
        assert_eq!(
            kinds(&blocks),
            [BlockType::Theorem, BlockType::Proof, BlockType::Text]
        );
        assert_eq!(blocks[0].span, 0..3);
        assert_eq!(blocks[0].score, 1.0);
        assert_eq!(blocks[0].end, EndReason::Environment);
        assert_eq!(blocks[0].title, "Theorem (Rolle)");
        assert!(
            blocks[0]
                .html
                .starts_with("<p class=\"mx-block-title\">Rolle</p>")
        );
        assert_eq!(blocks[1].span, 3..5);
        assert_eq!(blocks[1].proves.as_deref(), Some("block-theorem-1"));

        // A judge can still reject the environment; its nodes are then scored
        // like any others. Scope deltas do not apply to it.
        let c = HeuristicClassifier::default();
        let nodes = parse_markdown(md);
        let mut overrides = ScopeOverrides::default();
        overrides.delta.insert(0.into(), -1);
        assert_eq!(c.classify_with_overrides(&nodes, &overrides)[0].span, 0..3);
        overrides.suppressed.insert(0.into());
        let kinds_now = kinds(&c.classify_with_overrides(&nodes, &overrides));
        assert_eq!(kinds_now[0], BlockType::Text);
    }

    #[test]
    fn end_reasons_explain_every_scope() {
        let blocks = classify(
            "Theorem 1. For all $x$,\n\n$$x = x.$$\n\nDiscussion.\n\n\
             Proof. Clear. □\n\nLemma 2. A.\n\nLemma 3. B.\n\n## Next\n\nRemark. C.",
        );
        let ends: Vec<_> = blocks.iter().map(|b| (b.kind, b.end)).collect();
        assert_eq!(
            ends,
            [
                (BlockType::Theorem, EndReason::NoContinuation),
                (BlockType::Text, EndReason::Structural),
                (BlockType::Proof, EndReason::Qed),
                (BlockType::Lemma, EndReason::NextOpener),
                (BlockType::Lemma, EndReason::Heading),
                (BlockType::Heading, EndReason::Structural),
                (BlockType::Remark, EndReason::EndOfDocument),
            ]
        );

        let c = HeuristicClassifier::new(ScoringConfig {
            scope: crate::config::ScopeConfig {
                proof_max_nodes: 2,
                ..Default::default()
            },
            ..Default::default()
        });
        let nodes = parse_markdown("Proof. One.\n\nTwo.\n\nThree.\n\nFour.");
        let blocks = c.classify(&nodes);
        assert_eq!(
            (blocks[0].span.clone(), blocks[0].end),
            (0..2, EndReason::SizeLimit)
        );

        let mut overrides = ScopeOverrides::default();
        overrides.delta.insert(0.into(), 1);
        assert_eq!(
            c.classify_with_overrides(&nodes, &overrides)[0].end,
            EndReason::Override
        );
    }

    #[test]
    fn detailed_classification_covers_every_node() {
        let c = HeuristicClassifier::default();
        let nodes = parse_markdown("# T\n\nTheorem 1. A.\n\n$$x$$\n\nText.");
        let detail = c.classify_detailed(&nodes, &ScopeOverrides::default());
        assert_eq!(detail.scores.len(), nodes.len());
        assert_eq!(detail.block_of, [0, 1, 1, 2]);
        assert_eq!(detail.scores[1].scores.len(), BlockType::SEMANTIC.len());
        assert_eq!(detail.scores[1].best.0, BlockType::Theorem);
    }

    /// Asks for more until the block has `want` nodes.
    struct WantNodes(usize);
    impl BlockJudge for WantNodes {
        fn judge(&self, block: &SemanticBlock, _: &[Node]) -> Verdict {
            match block.span.len().cmp(&self.0) {
                std::cmp::Ordering::Less => Verdict::Incomplete,
                std::cmp::Ordering::Equal => Verdict::Accept,
                std::cmp::Ordering::Greater => Verdict::Overreach,
            }
        }
    }

    #[test]
    fn judge_can_widen_and_narrow_scope() {
        let c = HeuristicClassifier::default();
        let nodes = parse_markdown("Theorem 1. Statement.\n\nSecond half.\n\nThird.\n\nUnrelated.");
        assert_eq!(c.classify(&nodes)[0].span, 0..1);

        let widened = c.classify_with_judge(&nodes, &WantNodes(3), 8);
        assert_eq!(widened[0].span, 0..3);
        assert_eq!(kinds(&widened), [BlockType::Theorem, BlockType::Text]);

        let nodes = parse_markdown("Proof. One.\n\nTwo.\n\nThree. □");
        assert_eq!(c.classify(&nodes)[0].span, 0..3);
        assert_eq!(
            c.classify_with_judge(&nodes, &WantNodes(2), 8)[0].span,
            0..2
        );
    }

    #[test]
    fn judge_cannot_push_scope_through_a_hard_boundary() {
        let c = HeuristicClassifier::default();
        let nodes = parse_markdown("Theorem 1. Statement.\n\nProof. Trivial. □");
        let blocks = c.classify_with_judge(&nodes, &WantNodes(1), 8);
        assert_eq!(kinds(&blocks), [BlockType::Theorem, BlockType::Proof]);

        struct Never;
        impl BlockJudge for Never {
            fn judge(&self, _: &SemanticBlock, _: &[Node]) -> Verdict {
                Verdict::Incomplete
            }
        }
        // Terminates, and the theorem still stops at the proof.
        assert_eq!(c.classify_with_judge(&nodes, &Never, 8)[0].span, 0..1);
    }

    #[test]
    fn judge_can_reject_an_opener() {
        struct RejectRemarks;
        impl BlockJudge for RejectRemarks {
            fn judge(&self, block: &SemanticBlock, _: &[Node]) -> Verdict {
                if block.kind == BlockType::Remark {
                    Verdict::Reject
                } else {
                    Verdict::Accept
                }
            }
        }
        let c = HeuristicClassifier::default();
        let nodes = parse_markdown("Theorem 1. A.\n\nNote: this is just prose.");
        assert_eq!(
            kinds(&c.classify(&nodes)),
            [BlockType::Theorem, BlockType::Remark]
        );
        assert_eq!(
            kinds(&c.classify_with_judge(&nodes, &RejectRemarks, 4)),
            [BlockType::Theorem, BlockType::Text]
        );
    }

    #[test]
    fn decision_tree_scorer_makes_the_same_opening_calls() {
        let tree = HeuristicClassifier::new(ScoringConfig {
            scorer: Scorer::Tree(crate::tree::DecisionTree::default()),
            ..Default::default()
        });
        for (md, kind) in [
            (
                "**قضیه ۱.** اگر $a>0$ آنگاه $a^2>0$.",
                Some(BlockType::Theorem),
            ),
            ("برهان. فرض کنید $x \\in A$.", Some(BlockType::Proof)),
            (
                "Lemma 3 (Zorn). Every chain has an upper bound.",
                Some(BlockType::Lemma),
            ),
            (
                "Definition 2. A set is open if ...",
                Some(BlockType::Definition),
            ),
            ("طبق قضیه ۳.۲ می‌توان نتیجه گرفت که $f$ کراندار است.", None),
            (
                "Theorem 2 implies that for all $x$ we have $f(x) > 0$.",
                None,
            ),
            ("Proof techniques vary widely between fields.", None),
            ("Note that $f$ is continuous.", None),
        ] {
            let s = tree.score_node(&parse_markdown(md)[0]);
            assert_eq!(s.opens.then_some(s.best.0), kind, "{md} ({:?})", s.best);
        }
    }

    #[test]
    fn overrides_retype_and_force_openers() {
        let c = HeuristicClassifier::default();
        let nodes = parse_markdown("Theorem 1. A.\n\nPlain prose here.\n\nMore prose.");
        let mut overrides = ScopeOverrides::default();
        overrides.retype.insert(0.into(), BlockType::Lemma);
        overrides.retype.insert(1.into(), BlockType::Remark);
        let blocks = c.classify_with_overrides(&nodes, &overrides);
        assert_eq!(
            kinds(&blocks),
            [BlockType::Lemma, BlockType::Remark, BlockType::Text]
        );
        assert_eq!(blocks[0].id, "block-lemma-1");
        assert_eq!(blocks[0].title, "Lemma 1");
        assert!(overrides.validate(nodes.len()).is_ok());
        overrides.retype.insert(2.into(), BlockType::Text);
        assert!(overrides.validate(nodes.len()).is_err());
    }

    #[test]
    fn explicit_end_absorbs_openers_but_not_headings() {
        let c = HeuristicClassifier::default();
        let nodes =
            parse_markdown("Theorem 1. A.\n\nLemma 2. B.\n\nText.\n\n## Next\n\nRemark. C.");
        let mut overrides = ScopeOverrides::default();
        overrides.end.insert(0.into(), 3);
        let blocks = c.classify_with_overrides(&nodes, &overrides);
        assert_eq!(
            kinds(&blocks),
            [BlockType::Theorem, BlockType::Heading, BlockType::Remark]
        );
        assert_eq!(
            (blocks[0].span.clone(), blocks[0].end),
            (0..3, EndReason::Override)
        );

        overrides.end.insert(0.into(), 5);
        assert_eq!(c.classify_with_overrides(&nodes, &overrides)[0].span, 0..3);

        // Serialisable for the desktop app and for replay logs.
        let json = serde_json::to_string(&overrides).unwrap();
        assert_eq!(
            serde_json::from_str::<ScopeOverrides>(&json).unwrap(),
            overrides
        );
    }

    /// Plays back a fixed verdict per opener node, then accepts.
    struct Script(BTreeMap<(usize, usize), Verdict>);
    impl BlockJudge for Script {
        fn judge(&self, block: &SemanticBlock, _: &[Node]) -> Verdict {
            match self.0.get(&(block.span.start, block.span.end)) {
                Some(Verdict::Retype(kind)) if *kind == block.kind => Verdict::Accept,
                Some(verdict) => *verdict,
                None => Verdict::Accept,
            }
        }
    }

    #[test]
    fn judge_can_retype_split_and_move_starts() {
        let c = HeuristicClassifier::default();
        let nodes = parse_markdown(
            "Some setup we need.\n\nTheorem 1. Statement.\n\nSecond half.\n\nThird.\n\nUnrelated.",
        );
        assert_eq!(c.classify(&nodes)[1].span, 1..2);

        let judge = Script(BTreeMap::from([
            ((1, 2), Verdict::EndAt(4)),
            ((1, 4), Verdict::StartAt(0)),
            ((0, 4), Verdict::Retype(BlockType::Proposition)),
        ]));
        let run = c.classify_with_judge_detailed(&nodes, &judge, 8, ScopeOverrides::default());
        assert_eq!(
            run.blocks
                .iter()
                .map(|b| (b.kind, b.span.clone()))
                .collect::<Vec<_>>(),
            [(BlockType::Proposition, 0..4), (BlockType::Text, 4..5)]
        );
        assert_eq!(
            run.log
                .iter()
                .filter(|v| v.verdict != Verdict::Accept)
                .count(),
            3,
            "{:?}",
            run.log
        );
        // The overrides alone replay the judged result.
        assert_eq!(
            c.classify_with_overrides(&nodes, &run.overrides),
            run.blocks
        );

        // A split point inside the block narrows it; the rest becomes text.
        let judge = Script(BTreeMap::from([((0, 3), Verdict::EndAt(1))]));
        let nodes = parse_markdown("Proof. One.\n\nTwo.\n\nThree. □");
        let blocks = c.classify_with_judge(&nodes, &judge, 4);
        assert_eq!(kinds(&blocks), [BlockType::Proof, BlockType::Text]);
        assert_eq!(blocks[0].span, 0..1);
    }

    #[test]
    fn unusable_verdicts_are_logged_and_ignored() {
        let c = HeuristicClassifier::default();
        let nodes = parse_markdown("Theorem 1. A.\n\nText.");
        let judge = Script(BTreeMap::from([((0, 1), Verdict::EndAt(9))]));
        let run = c.classify_with_judge_detailed(&nodes, &judge, 4, ScopeOverrides::default());
        assert_eq!(run.blocks, c.classify(&nodes));
        assert!(!run.log[0].applied);
        assert_eq!(run.rounds, 1);
    }

    #[test]
    fn a_self_contradicting_judge_terminates() {
        /// Wants the theorem longer when short, shorter when long.
        struct FlipFlop;
        impl BlockJudge for FlipFlop {
            fn judge(&self, block: &SemanticBlock, _: &[Node]) -> Verdict {
                if block.span.len() == 1 {
                    Verdict::Incomplete
                } else {
                    Verdict::Overreach
                }
            }
        }
        let c = HeuristicClassifier::default();
        let nodes = parse_markdown("Theorem 1. A.\n\nB.\n\nC.");
        let run = c.classify_with_judge_detailed(&nodes, &FlipFlop, 100, ScopeOverrides::default());
        assert!(run.rounds <= 3, "{}", run.rounds);
    }

    #[test]
    fn number_first_textbook_blocks_are_classified_and_linked() {
        let blocks = classify(
            "1.1 Definition. A group is a set with an associative binary operation.\n\n\
             1.2 Lemma (Euclid). If $p \\mid ab$, then $p \\mid a$ or $p \\mid b$.\n\n\
             **1.3. Theorem (Lagrange).** The order of a subgroup divides $|G|$.\n\n\
             Proof of Theorem 1.3. Cosets partition $G$. □\n\n\
             ۳.۲ قضیه. هر دنبالهٔ کراندار زیر‌دنبالهٔ همگرا دارد.",
        );
        assert_eq!(
            kinds(&blocks),
            [
                BlockType::Definition,
                BlockType::Lemma,
                BlockType::Theorem,
                BlockType::Proof,
                BlockType::Theorem,
            ]
        );
        assert_eq!(blocks[0].title, "Definition 1.1");
        assert_eq!(blocks[1].title, "Lemma 1.2 (Euclid)");
        assert_eq!(blocks[2].title, "Theorem 1.3 (Lagrange)");
        assert_eq!(blocks[3].proves.as_deref(), Some("block-theorem-1"));
        assert_eq!(blocks[4].title, "قضیه ۳.۲");
    }

    #[test]
    fn stable_anchors_survive_editing_above_block() {
        let c = HeuristicClassifier::default();

        // Original document: Theorem 1 is at node index 1 (after the heading)
        let doc_v1 = "# Introduction\n\n**Theorem 1.** Every finite field has prime power order.\n\nProof. Follows from algebra.";
        let nodes_v1 = parse_markdown(doc_v1);
        assert_eq!(nodes_v1[1].kind, NodeKind::Paragraph);

        // User overrides Theorem 1 (node 1) to be a Proposition using a content anchor
        let mut overrides = ScopeOverrides::default();
        let key = OverrideKey::for_node(&nodes_v1, 1);
        overrides.retype.insert(key, BlockType::Proposition);

        // Verify v1 classification: node 1 is retyped as Proposition
        let blocks_v1 = c.classify_with_overrides(&nodes_v1, &overrides);
        assert_eq!(blocks_v1[1].kind, BlockType::Proposition);

        // Now user edits text ABOVE the theorem, inserting 2 new paragraphs
        let doc_v2 = "# Introduction\n\nFirst newly inserted paragraph.\n\nSecond newly inserted paragraph.\n\n**Theorem 1.** Every finite field has prime power order.\n\nProof. Follows from algebra.";
        let nodes_v2 = parse_markdown(doc_v2);
        // In v2, Theorem 1 has moved from index 1 to index 3!
        assert_eq!(nodes_v2[3].kind, NodeKind::Paragraph);
        assert!(nodes_v2[3].flat.contains("Theorem 1"));

        // With content-based anchors, the override STILL finds Theorem 1 at node 3!
        let blocks_v2 = c.classify_with_overrides(&nodes_v2, &overrides);
        // Newly inserted paragraphs remain ordinary text
        assert_eq!(blocks_v2[1].kind, BlockType::Text);
        // The theorem (now at node 3) is correctly retyped to Proposition!
        let prop = blocks_v2
            .iter()
            .find(|b| b.kind == BlockType::Proposition)
            .expect("proposition found");
        assert_eq!(prop.span.start, 3);
    }

    #[test]
    fn fallback_to_node_index_when_anchor_does_not_match() {
        let c = HeuristicClassifier::default();
        let doc = "# Title\n\n**Theorem 1.** Some statement.\n\nMore text.";
        let nodes = parse_markdown(doc);

        // Key with an unmatched anchor hash, but a valid fallback index of 1
        let key = OverrideKey {
            anchor: Some(ContentAnchor {
                hash: 0xdeadbeef,
                occurrence: 0,
            }),
            fallback: Some(1),
        };
        let mut overrides = ScopeOverrides::default();
        overrides.retype.insert(key, BlockType::Lemma);

        let blocks = c.classify_with_overrides(&nodes, &overrides);
        assert_eq!(blocks[1].kind, BlockType::Lemma);
    }

    #[test]
    fn override_key_round_trips_json() {
        let key_anchor = OverrideKey::anchored(0x1234abcd, 0, 5);
        let key_index = OverrideKey::index(3);

        let mut overrides = ScopeOverrides::default();
        overrides.retype.insert(key_anchor, BlockType::Lemma);
        overrides.delta.insert(key_index, 2);
        overrides.suppressed.insert(key_anchor);
        overrides.end.insert(key_anchor, 7);

        let json = serde_json::to_string(&overrides).unwrap();
        let parsed: ScopeOverrides = serde_json::from_str(&json).unwrap();
        assert_eq!(overrides, parsed);
    }

    #[test]
    fn end_override_preserves_length_when_anchored_opener_shifts() {
        let c = HeuristicClassifier::default();

        // v1 document: Theorem 1 is at index 1, followed by 2 continuation paragraphs.
        let doc_v1 = "# Section\n\n**Theorem 1.** Statement.\n\nContinuation 1.\n\nContinuation 2.\n\nAfter.";
        let nodes_v1 = parse_markdown(doc_v1);
        assert_eq!(nodes_v1[1].kind, NodeKind::Paragraph);

        // An end override on Theorem 1 (node 1) extending it to end at node 4 (length = 4 - 1 = 3 nodes: 1, 2, 3)
        let key = OverrideKey::for_node(&nodes_v1, 1);
        let mut overrides = ScopeOverrides::default();
        overrides.end.insert(key, 4);

        let blocks_v1 = c.classify_with_overrides(&nodes_v1, &overrides);
        let thm_v1 = blocks_v1
            .iter()
            .find(|b| b.kind == BlockType::Theorem)
            .unwrap();
        assert_eq!(thm_v1.span, 1..4);

        // v2: 2 new paragraphs inserted ABOVE Theorem 1.
        let doc_v2 = "# Section\n\nInserted paragraph A.\n\nInserted paragraph B.\n\n**Theorem 1.** Statement.\n\nContinuation 1.\n\nContinuation 2.\n\nAfter.";
        let nodes_v2 = parse_markdown(doc_v2);
        // Theorem 1 moved from index 1 to index 3.
        assert_eq!(nodes_v2[3].kind, NodeKind::Paragraph);
        assert!(nodes_v2[3].flat.contains("Theorem 1"));

        // Overrides should resolve anchor to node 3 and shift end by (3 - 1) = +2: new end = 4 + 2 = 6.
        let blocks_v2 = c.classify_with_overrides(&nodes_v2, &overrides);
        let thm_v2 = blocks_v2
            .iter()
            .find(|b| b.kind == BlockType::Theorem)
            .unwrap();
        assert_eq!(thm_v2.span, 3..6);
        assert_eq!(thm_v2.span.len(), 3);
    }

    #[test]
    fn override_key_from_str_rejects_invalid_strings() {
        use std::str::FromStr;
        assert!(OverrideKey::from_str("0").is_ok());
        assert!(OverrideKey::from_str("12").is_ok());
        assert!(OverrideKey::from_str("deadbeef:0").is_ok());
        assert!(OverrideKey::from_str("deadbeef:0:2").is_ok());

        assert!(OverrideKey::from_str("not_a_key").is_err());
        assert!(OverrideKey::from_str("deadbeef:not_num").is_err());
        assert!(OverrideKey::from_str("deadbeef:0:not_num").is_err());
        assert!(OverrideKey::from_str("a:b:c:d").is_err());
    }

    #[test]
    fn classify_with_tagger_accepts_sequence_tagger() {
        let c = HeuristicClassifier::default();
        let nodes = parse_markdown("**Theorem 1.** Statement.\n\nProof. Follows.");
        let res = c.classify_with_tagger(&nodes, &ScopeOverrides::default(), &c.tagger);
        assert_eq!(res.blocks.len(), 2);
        assert_eq!(res.blocks[0].kind, BlockType::Theorem);
        assert_eq!(res.blocks[1].kind, BlockType::Proof);
    }

    struct CallCountJudge(std::sync::Mutex<usize>);
    impl BlockJudge for CallCountJudge {
        fn judge(&self, _: &SemanticBlock, _: &[Node]) -> Verdict {
            *self.0.lock().unwrap() += 1;
            Verdict::Accept
        }
    }

    #[test]
    fn clean_documents_never_trigger_math_judge() {
        let c = HeuristicClassifier::default();

        // 1. Explicit LaTeX environment is bypassed (0 judge calls)
        let latex_nodes =
            parse_markdown("\\begin{theorem}\nFor all $x$, $f(x) \\ge 0$.\n\\end{theorem}");
        let judge1 = CallCountJudge(std::sync::Mutex::new(0));
        let run1 =
            c.classify_with_judge_detailed(&latex_nodes, &judge1, 4, ScopeOverrides::default());
        assert_eq!(
            *judge1.0.lock().unwrap(),
            0,
            "LaTeX EnvSpan must never trigger math-judge"
        );
        assert_eq!(run1.rounds, 0);

        // 2. High-confidence clean Markdown document is bypassed (0 judge calls)
        let clean_nodes = parse_markdown(
            "# Section\n\n**Theorem 1.** Let $x \\in \\mathbb{R}$. Then $x^2 \\ge 0$.\n\nProof. Obvious. $\\blacksquare$",
        );
        let judge2 = CallCountJudge(std::sync::Mutex::new(0));
        let run2 =
            c.classify_with_judge_detailed(&clean_nodes, &judge2, 4, ScopeOverrides::default());
        assert_eq!(
            *judge2.0.lock().unwrap(),
            0,
            "Clean high-confidence doc must never trigger math-judge"
        );
        assert_eq!(run2.rounds, 0);
    }

    #[test]
    fn scope_overrides_always_win_over_both_crf_and_judge() {
        struct RejectAll;
        impl BlockJudge for RejectAll {
            fn judge(&self, _: &SemanticBlock, _: &[Node]) -> Verdict {
                Verdict::Reject
            }
        }

        let c = HeuristicClassifier::default();
        let nodes = parse_markdown("Theorem 1. A.\n\nB.\n\nC.");

        // User specifies a manual ScopeOverride in initial
        let mut initial = ScopeOverrides::default();
        initial
            .retype
            .insert(OverrideKey::index(0), BlockType::Lemma);
        initial.end.insert(OverrideKey::index(0), 3);

        let run = c.classify_with_judge_detailed(&nodes, &RejectAll, 4, initial);
        // The block must retain the user's override and NOT be overridden by the judge
        assert_eq!(run.blocks[0].kind, BlockType::Lemma);
        assert_eq!(run.blocks[0].span, 0..3);
    }

    #[test]
    fn confidence_gated_ambiguous_span_triggers_judge() {
        let c = HeuristicClassifier::default();
        let nodes = parse_markdown("Theorem 1. A.\n\nNote: this is just prose.");
        let judge = CallCountJudge(std::sync::Mutex::new(0));
        let run = c.classify_with_judge_detailed(&nodes, &judge, 4, ScopeOverrides::default());
        assert!(
            *judge.0.lock().unwrap() > 0,
            "Ambiguous span must escalate to judge"
        );
        assert!(run.rounds > 0);
    }

    #[test]
    fn outside_paragraph_with_high_logic_density_triggers_judge() {
        let c = HeuristicClassifier::default();
        // A theorem followed by an outside paragraph packed with logic implication symbols
        let nodes = parse_markdown(
            "**Theorem 1.** Main statement.\n\nSince $x > 0 \\implies x^2 > 0 \\implies x^3 > 0$, the claim holds.",
        );
        let judge = CallCountJudge(std::sync::Mutex::new(0));
        let run = c.classify_with_judge_detailed(&nodes, &judge, 4, ScopeOverrides::default());
        assert!(
            *judge.0.lock().unwrap() > 0,
            "Outside paragraph with high logic density must escalate"
        );
        assert!(run.rounds > 0);
    }

    #[test]
    fn telemetry_generation_and_anonymization() {
        let c = HeuristicClassifier::default();
        let md = "**Theorem 1.** Sensitive user secrets $x = 1$:\n\nProof. Hidden details. $\\blacksquare$";
        let mut ov = ScopeOverrides::default();
        ov.retype
            .insert(OverrideKey::index(0), BlockType::Proposition);

        let tel = crate::features::create_override_telemetry(md, &c.config, &ov)
            .expect("telemetry created");
        assert!(tel.doc_id.starts_with("telemetry_"));
        assert_eq!(tel.language, "en");
        assert_eq!(tel.nodes[0].kind, "paragraph");
        assert_eq!(tel.nodes[0].flat_text, ":"); // trailing colon preserved for CRF feature
        assert!(!tel.nodes[0].flat_text.contains("Sensitive")); // sensitive prose redacted
        assert_eq!(tel.nodes[1].flat_text, ""); // no colon -> empty flat_text
        assert!(!tel.nodes[1].flat_text.contains("Hidden")); // sensitive prose redacted
        assert!(tel.bio_tags.contains(&"B-PROPOSITION".to_string())); // corrected BIO tag

        let tmp = std::env::temp_dir().join(format!("mathia-tel-{}.jsonl", std::process::id()));
        crate::features::append_telemetry_entry(&tmp, &tel).unwrap();
        let line = std::fs::read_to_string(&tmp).unwrap();
        let parsed: crate::features::TelemetryEntry = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(parsed, tel);
        std::fs::remove_file(&tmp).unwrap();
    }
}
