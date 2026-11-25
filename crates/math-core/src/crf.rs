//! Pure-Rust Linear-Chain CRF Viterbi decoder with embedded weights.
//!
//! Provides [`CrfTagger`], a high-performance pure-Rust implementation of
//! sequence labeling using Viterbi dynamic programming. Features are extracted
//! identically to the Python dev flywheel (`ml/src/mathia_ml/features.py`),
//! and model weights (`crf_weights.json`) are embedded at compile time via
//! [`include_str!`] with zero external file I/O or runtime dependencies.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::features::{DumpNode, NodeFeatures};
use crate::model::{Node, NodeKind};
use crate::tagger::{BioTag, SequenceTagger, TaggedNode};

/// Embedded JSON weights trained via `sklearn-crfsuite` in Slice 2, Task 2.
pub const EMBEDDED_WEIGHTS_JSON: &str = include_str!("crf_weights.json");

/// Window context offsets (-1, 0, +1).
pub const WINDOW_OFFSETS: [isize; 3] = [-1, 0, 1];

/// Math density bucket boundaries.
pub const MATH_ZERO_THRESHOLD: f32 = 0.001;
pub const MATH_LOW_THRESHOLD: f32 = 0.30;
pub const MATH_MED_THRESHOLD: f32 = 0.70;

/// Logic density bucket boundaries.
pub const LOGIC_ZERO_THRESHOLD: f32 = 0.001;
pub const LOGIC_LOW_THRESHOLD: f32 = 0.35;
pub const LOGIC_MED_THRESHOLD: f32 = 0.70;

/// Math density change threshold for delta feature.
pub const MATH_DELTA_THRESHOLD: f32 = 0.05;

/// Character-length bucket boundaries for the block-size feature.
pub const CHAR_TINY_MAX: usize = 40;
pub const CHAR_SHORT_MAX: usize = 140;
pub const CHAR_MEDIUM_MAX: usize = 400;

/// Damping factor applied to every learned transition weight.
///
/// The training corpus is small and synthetic, so the learned transition prior
/// is overconfident: a proof self-loop of `+5` dwarfs the emission evidence and
/// Viterbi never leaves a proof once it enters one. Damping the transitions
/// lets the (lexically and structurally grounded) emission model decide block
/// boundaries. Mirrors `TRANSITION_TEMPERATURE` in `ml/src/mathia_ml/train_crf.py`.
pub const TRANSITION_TEMPERATURE: f32 = 0.4;

/// View trait abstracting over parsed AST [`Node`] and deserialized [`DumpNode`].
pub trait CrfNode {
    fn node_kind(&self) -> &str;
    fn flat_text(&self) -> &str;
    /// Character length of the node's flattened text.
    ///
    /// [`DumpNode`] carries this explicitly so anonymized telemetry (whose
    /// `flat_text` is stripped) still exposes its true length.
    fn char_len(&self) -> usize;
}

impl CrfNode for Node {
    fn node_kind(&self) -> &str {
        match &self.kind {
            NodeKind::Paragraph => "paragraph",
            NodeKind::DisplayMath => "display_math",
            NodeKind::List => "list",
            NodeKind::ListItem => "list_item",
            NodeKind::Heading(_) => "heading",
            NodeKind::Quote => "quote",
            NodeKind::Rule => "rule",
            NodeKind::Table => "table",
            NodeKind::Code => "code",
            NodeKind::Other => "other",
        }
    }

    fn flat_text(&self) -> &str {
        &self.flat
    }

    fn char_len(&self) -> usize {
        self.flat.chars().count()
    }
}

impl CrfNode for DumpNode {
    fn node_kind(&self) -> &str {
        &self.kind
    }

    fn flat_text(&self) -> &str {
        &self.flat_text
    }

    fn char_len(&self) -> usize {
        self.char_len
    }
}

/// Return canonical prefix for feature offset (`"-1:"`, `"0:"`, `"+1:"`).
#[inline]
pub fn offset_prefix(offset: isize) -> &'static str {
    match offset {
        -1 => "-1:",
        0 => "0:",
        1 => "+1:",
        _ => "0:",
    }
}

/// Bucket continuous math density `[0.0, 1.0]` into discrete category strings.
#[inline]
pub fn bucket_math_density(density: f32) -> &'static str {
    if density <= MATH_ZERO_THRESHOLD {
        "zero"
    } else if density <= MATH_LOW_THRESHOLD {
        "low"
    } else if density <= MATH_MED_THRESHOLD {
        "medium"
    } else {
        "high"
    }
}

/// Bucket continuous logic density `[0.0, 1.0]` into discrete category strings.
#[inline]
pub fn bucket_logic_density(density: f32) -> &'static str {
    if density <= LOGIC_ZERO_THRESHOLD {
        "zero"
    } else if density <= LOGIC_LOW_THRESHOLD {
        "low"
    } else if density <= LOGIC_MED_THRESHOLD {
        "medium"
    } else {
        "high"
    }
}

/// Compute math density direction relative to previous node.
#[inline]
pub fn get_math_density_delta(curr_density: f32, prev_density: Option<f32>) -> &'static str {
    match prev_density {
        None => "none",
        Some(prev) => {
            let diff = curr_density - prev;
            if diff > MATH_DELTA_THRESHOLD {
                "up"
            } else if diff < -MATH_DELTA_THRESHOLD {
                "down"
            } else {
                "flat"
            }
        }
    }
}

/// Return true if `flat_text` ends with an ASCII or full-width colon.
#[inline]
pub fn node_ends_with_colon(flat_text: &str) -> bool {
    let t = flat_text.trim();
    t.ends_with(':') || t.ends_with('：')
}

/// Bucket a node's character length into the block-size feature categories.
#[inline]
pub fn bucket_char_len(char_len: usize) -> &'static str {
    if char_len <= CHAR_TINY_MAX {
        "tiny"
    } else if char_len <= CHAR_SHORT_MAX {
        "short"
    } else if char_len <= CHAR_MEDIUM_MAX {
        "medium"
    } else {
        "long"
    }
}

/// Return true if `flat_text` starts (after leading whitespace) with an ASCII
/// lowercase letter — a strong English continuation signal.
#[inline]
pub fn starts_with_ascii_lowercase(flat_text: &str) -> bool {
    flat_text
        .trim_start()
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase())
}

/// Return true if `flat_text` starts (after leading whitespace) with `$` or `$$`.
#[inline]
pub fn starts_with_math(flat_text: &str) -> bool {
    flat_text.trim_start().starts_with('$')
}

/// Return true if `flat_text` ends with a continuation punctuation mark.
#[inline]
pub fn ends_with_continuation_punct(flat_text: &str) -> bool {
    flat_text
        .trim_end()
        .chars()
        .next_back()
        .is_some_and(|c| matches!(c, ',' | '،' | ':' | '：'))
}

/// Bucket the distance (in nodes) since the most recent labelled opener.
#[inline]
pub fn bucket_dist_from_lead(dist: usize) -> &'static str {
    match dist {
        0 => "0",
        1 => "1",
        2 => "2",
        3..=5 => "3_5",
        _ => "6_plus",
    }
}

/// Distance in nodes since the most recent non-reference labelled opener,
/// resetting at headings. Returns `"none"` when no such opener precedes `i`.
pub fn dist_from_lead<N: CrfNode>(
    nodes: &[N],
    features: &[NodeFeatures],
    i: usize,
) -> &'static str {
    for j in (0..=i).rev() {
        if nodes[j].node_kind().trim().eq_ignore_ascii_case("heading") {
            return "none";
        }
        if let Some(lead) = &features[j].lead
            && lead.labelled
            && !lead.reference
        {
            return bucket_dist_from_lead(i - j);
        }
    }
    "none"
}

/// Extract base features for a node at given context window offset into a visitor callback.
pub fn extract_node_base_features_visitor<F: FnMut(&str)>(
    node_kind: &str,
    flat_text: &str,
    feat: &NodeFeatures,
    offset: isize,
    buf: &mut String,
    mut visit: F,
) {
    let p = offset_prefix(offset);

    // 1. node_kind
    let kind_trimmed = node_kind.trim().to_ascii_lowercase();
    buf.clear();
    let _ = write!(buf, "{p}node_kind={kind_trimmed}");
    visit(buf);

    // 2. ends_colon
    if node_ends_with_colon(flat_text) {
        buf.clear();
        let _ = write!(buf, "{p}ends_colon=true");
        visit(buf);
    }

    // 3. lead information
    if let Some(lead) = &feat.lead {
        buf.clear();
        let _ = write!(buf, "{p}has_lead=true");
        visit(buf);

        let lk = lead.kind.slug().to_ascii_lowercase();
        let lk_trim = lk.trim();

        buf.clear();
        let _ = write!(buf, "{p}lead={lk_trim}");
        visit(buf);

        if lead.labelled {
            buf.clear();
            let _ = write!(buf, "{p}labelled=true");
            visit(buf);
        }

        if lead.body_empty {
            buf.clear();
            let _ = write!(buf, "{p}body_empty=true");
            visit(buf);
        }

        if lead.reference {
            buf.clear();
            let _ = write!(buf, "{p}lead_reference=true");
            visit(buf);
        }

        if lead.number.is_some() {
            buf.clear();
            let _ = write!(buf, "{p}lead_has_number=true");
            visit(buf);
        }

        if lead.name.is_some() {
            buf.clear();
            let _ = write!(buf, "{p}lead_has_name=true");
            visit(buf);
        }

        if let Some(refers_to) = &lead.refers_to {
            let ref_kind = refers_to.kind.slug().to_ascii_lowercase();
            buf.clear();
            let _ = write!(buf, "{p}lead_refers_to={}", ref_kind.trim());
            visit(buf);
        }
    } else {
        buf.clear();
        let _ = write!(buf, "{p}has_lead=false");
        visit(buf);
    }

    // 4. emphasized
    if feat.emphasized {
        buf.clear();
        let _ = write!(buf, "{p}emphasized=true");
        visit(buf);
    }

    // 5. qed
    if feat.qed {
        buf.clear();
        let _ = write!(buf, "{p}qed=true");
        visit(buf);
    }

    // 6. math_density (bucketed)
    buf.clear();
    let _ = write!(
        buf,
        "{p}math_density={}",
        bucket_math_density(feat.math_density)
    );
    visit(buf);

    // 7. logic_density (bucketed)
    buf.clear();
    let _ = write!(
        buf,
        "{p}logic_density={}",
        bucket_logic_density(feat.logic_density)
    );
    visit(buf);

    // 8. cues
    if feat.proof_cue {
        buf.clear();
        let _ = write!(buf, "{p}proof_cue=true");
        visit(buf);
    }
    if feat.statement_cue {
        buf.clear();
        let _ = write!(buf, "{p}statement_cue=true");
        visit(buf);
    }
    if feat.definitional {
        buf.clear();
        let _ = write!(buf, "{p}definitional=true");
        visit(buf);
    }
    if feat.continuation {
        buf.clear();
        let _ = write!(buf, "{p}continuation=true");
        visit(buf);
    }

    // 9. mentions (deduplicated)
    for (idx, m) in feat.mentions.iter().enumerate() {
        if !feat.mentions[..idx].contains(m) {
            let m_slug = m.slug().to_ascii_lowercase();
            buf.clear();
            let _ = write!(buf, "{p}mention={}", m_slug.trim());
            visit(buf);
        }
    }
}

/// Extract all sliding-window [i-1, i, i+1] and relational features for node `i` into a visitor callback.
pub fn extract_node_features_visitor<N: CrfNode, F: FnMut(&str)>(
    nodes: &[N],
    features: &[NodeFeatures],
    i: usize,
    buf: &mut String,
    mut visit: F,
) {
    let n = nodes.len();
    assert!(
        i < n,
        "Node index {i} out of bounds for sequence length {n}"
    );

    // Bias indicator
    visit("bias");

    // Sliding window offsets [-1, 0, +1]
    if i > 0 {
        extract_node_base_features_visitor(
            nodes[i - 1].node_kind(),
            nodes[i - 1].flat_text(),
            &features[i - 1],
            -1,
            buf,
            &mut visit,
        );
    }
    extract_node_base_features_visitor(
        nodes[i].node_kind(),
        nodes[i].flat_text(),
        &features[i],
        0,
        buf,
        &mut visit,
    );
    if i + 1 < n {
        extract_node_base_features_visitor(
            nodes[i + 1].node_kind(),
            nodes[i + 1].flat_text(),
            &features[i + 1],
            1,
            buf,
            &mut visit,
        );
    }

    // Boundary and relational features
    if i == 0 {
        visit("0:is_first_node=true");
    }
    if i == n - 1 {
        visit("0:is_last_node=true");
    }

    if i > 0 {
        let delta =
            get_math_density_delta(features[i].math_density, Some(features[i - 1].math_density));
        buf.clear();
        let _ = write!(buf, "0:math_density_delta={delta}");
        visit(buf);
    } else {
        let delta = get_math_density_delta(features[i].math_density, None);
        buf.clear();
        let _ = write!(buf, "0:math_density_delta={delta}");
        visit(buf);
    }

    // Boundary & scope features (block-size learning). These are derivable
    // from `kind`, `flat_text`, `char_len` and `NodeFeatures` alone, so the
    // offline Python extractor and live Rust inference stay in lockstep.
    buf.clear();
    let _ = write!(
        buf,
        "0:char_len_bucket={}",
        bucket_char_len(nodes[i].char_len())
    );
    visit(buf);

    if starts_with_ascii_lowercase(nodes[i].flat_text()) {
        visit("0:starts_lowercase=true");
    }

    if starts_with_math(nodes[i].flat_text()) {
        visit("0:starts_math=true");
    }

    if i > 0 && ends_with_continuation_punct(nodes[i - 1].flat_text()) {
        visit("0:prev_ends_continuation_punct=true");
    }

    buf.clear();
    let _ = write!(
        buf,
        "0:dist_from_lead={}",
        dist_from_lead(nodes, features, i)
    );
    visit(buf);
}

/// Extract all sliding-window [i-1, i, i+1] and relational feature keys for node `i` as a `Vec<String>`.
pub fn extract_node_features<N: CrfNode>(
    nodes: &[N],
    features: &[NodeFeatures],
    i: usize,
) -> Vec<String> {
    let mut out = Vec::with_capacity(32);
    let mut buf = String::with_capacity(64);
    extract_node_features_visitor(nodes, features, i, &mut buf, |key| {
        out.push(key.to_string());
    });
    out
}

/// Extract sliding-window feature keys for every node in a sequence.
pub fn extract_sequence_features<N: CrfNode>(
    nodes: &[N],
    features: &[NodeFeatures],
) -> Vec<Vec<String>> {
    assert_eq!(
        nodes.len(),
        features.len(),
        "Mismatched sequence lengths: nodes={}, features={}",
        nodes.len(),
        features.len()
    );
    (0..nodes.len())
        .map(|i| extract_node_features(nodes, features, i))
        .collect()
}

/// Raw JSON representation exported from `ml/src/mathia_ml/train_crf.py`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RawCrfWeights {
    pub labels: Vec<String>,
    pub transitions: Vec<Vec<f64>>,
    pub state_features: HashMap<String, HashMap<String, f64>>,
}

/// In-memory optimized Linear-Chain CRF model weights.
#[derive(Debug, Clone)]
pub struct CrfModel {
    pub labels: Vec<String>,
    pub bio_tags: Vec<BioTag>,
    /// Transition weights: `transitions[from_idx][to_idx]`.
    pub transitions: Vec<Vec<f64>>,
    /// State feature lookup: `feature_key -> Vec<(label_idx, weight)>`.
    pub state_features: HashMap<String, Vec<(usize, f64)>>,
}

impl CrfModel {
    /// Return the global singleton instance parsed from embedded `crf_weights.json`.
    pub fn embedded_shared() -> Arc<Self> {
        static INSTANCE: std::sync::LazyLock<Arc<CrfModel>> = std::sync::LazyLock::new(|| {
            let model = CrfModel::from_json(EMBEDDED_WEIGHTS_JSON)
                .expect("embedded crf_weights.json must be valid");
            Arc::new(model)
        });
        Arc::clone(&INSTANCE)
    }

    /// Return a reference to the global singleton instance.
    pub fn embedded() -> &'static Self {
        // Safe leak-free reference via Arc deref in LazyLock
        static REF: std::sync::LazyLock<Arc<CrfModel>> =
            std::sync::LazyLock::new(CrfModel::embedded_shared);
        &REF
    }

    /// Parse a [`CrfModel`] from a JSON string conforming to the target schema.
    pub fn from_json(json_str: &str) -> Result<Self> {
        let raw: RawCrfWeights = serde_json::from_str(json_str)
            .map_err(|e| Error::Config(format!("Failed to parse CRF weights JSON: {e}")))?;
        Self::from_raw(raw)
    }

    /// Construct a [`CrfModel`] from validated [`RawCrfWeights`].
    pub fn from_raw(raw: RawCrfWeights) -> Result<Self> {
        let k = raw.labels.len();
        if raw.transitions.len() != k || raw.transitions.iter().any(|row| row.len() != k) {
            return Err(Error::Config(format!(
                "CRF transition matrix must be {k}x{k}, got {} rows",
                raw.transitions.len()
            )));
        }

        let bio_tags = raw
            .labels
            .iter()
            .map(|s| BioTag::from_label(s).unwrap_or(BioTag::Outside))
            .collect();

        let mut state_features = HashMap::with_capacity(raw.state_features.len());
        for (feat_key, label_weights) in raw.state_features {
            let mut list = Vec::with_capacity(label_weights.len());
            for (label_idx_str, weight) in label_weights {
                if let Ok(idx) = label_idx_str.parse::<usize>()
                    && idx < k
                {
                    list.push((idx, weight));
                }
            }
            list.sort_unstable_by_key(|&(idx, _)| idx);
            state_features.insert(feat_key, list);
        }

        Ok(Self {
            labels: raw.labels,
            bio_tags,
            transitions: raw.transitions,
            state_features,
        })
    }
}

/// Pure-Rust SequenceTagger using Linear-Chain CRF and Viterbi dynamic programming.
#[derive(Debug, Clone)]
pub struct CrfTagger {
    model: Arc<CrfModel>,
}

impl Default for CrfTagger {
    fn default() -> Self {
        Self::new()
    }
}

impl CrfTagger {
    /// Construct a tagger with the embedded `crf_weights.json` model.
    pub fn new() -> Self {
        Self {
            model: CrfModel::embedded_shared(),
        }
    }

    /// Construct a tagger with custom model weights.
    pub fn with_model(model: CrfModel) -> Self {
        Self {
            model: Arc::new(model),
        }
    }

    /// Access the underlying [`CrfModel`].
    pub fn model(&self) -> &CrfModel {
        &self.model
    }

    /// Run Viterbi decoding over any type implementing [`CrfNode`] and [`NodeFeatures`].
    ///
    /// Computes optimal BIO label sequence and per-node confidence margin
    /// (difference between global Viterbi path score and best alternative path).
    pub fn decode_nodes<N: CrfNode>(
        &self,
        nodes: &[N],
        features: &[NodeFeatures],
    ) -> Vec<TaggedNode> {
        let n = nodes.len();
        if n == 0 {
            return Vec::new();
        }

        let k = self.model.labels.len();
        if k == 0 {
            return Vec::new();
        }

        // 1. Compute state emissions: emissions[t][label_idx]
        let mut emissions = vec![vec![0.0f64; k]; n];
        let mut buf = String::with_capacity(64);

        for (t, emission_t) in emissions.iter_mut().enumerate().take(n) {
            extract_node_features_visitor(nodes, features, t, &mut buf, |key| {
                if let Some(weights) = self.model.state_features.get(key) {
                    for &(label_idx, w) in weights {
                        emission_t[label_idx] += w;
                    }
                }
            });
        }

        // 2. Forward Viterbi dynamic programming
        let mut dp = vec![vec![0.0f64; k]; n];
        let mut backpointers = vec![vec![0usize; k]; n];

        dp[0] = emissions[0].clone();

        for t in 1..n {
            for j in 0..k {
                let mut best_prev = 0;
                let mut best_score = f64::NEG_INFINITY;
                for (prev, &prev_dp) in dp[t - 1].iter().enumerate().take(k) {
                    let score = prev_dp
                        + self.model.transitions[prev][j] * f64::from(TRANSITION_TEMPERATURE);
                    if score > best_score {
                        best_score = score;
                        best_prev = prev;
                    }
                }
                dp[t][j] = best_score + emissions[t][j];
                backpointers[t][j] = best_prev;
            }
        }

        // 3. Backtracking for the optimal sequence path
        let mut best_last = 0;
        let mut max_final = f64::NEG_INFINITY;
        for (j, &score) in dp[n - 1].iter().enumerate().take(k) {
            if score > max_final {
                max_final = score;
                best_last = j;
            }
        }

        let mut path = vec![0usize; n];
        path[n - 1] = best_last;
        for t in (1..n).rev() {
            path[t - 1] = backpointers[t][path[t]];
        }

        // 4. Backward pass for max-marginal Viterbi scores
        let mut b = vec![vec![0.0f64; k]; n];
        for t in (0..n - 1).rev() {
            for j in 0..k {
                let mut max_b = f64::NEG_INFINITY;
                for next_j in 0..k {
                    let score = self.model.transitions[j][next_j]
                        * f64::from(TRANSITION_TEMPERATURE)
                        + emissions[t + 1][next_j]
                        + b[t + 1][next_j];
                    if score > max_b {
                        max_b = score;
                    }
                }
                b[t][j] = max_b;
            }
        }

        // 5. Build TaggedNode results with confidence margin
        let mut tagged = Vec::with_capacity(n);
        for t in 0..n {
            let label_idx = path[t];
            let tag = self.model.bio_tags[label_idx];

            let best_score = dp[t][label_idx] + b[t][label_idx];
            let mut second_best = f64::NEG_INFINITY;
            for j in 0..k {
                if j != label_idx {
                    let s = dp[t][j] + b[t][j];
                    if s > second_best {
                        second_best = s;
                    }
                }
            }

            let confidence = if k > 1 && second_best.is_finite() {
                (best_score - second_best).max(0.0) as f32
            } else {
                1.0
            };

            tagged.push(TaggedNode { tag, confidence });
        }

        tagged
    }

    /// Decode parsed document AST nodes.
    pub fn decode(&self, nodes: &[Node], features: &[NodeFeatures]) -> Vec<TaggedNode> {
        self.decode_nodes(nodes, features)
    }
}

impl SequenceTagger for CrfTagger {
    fn tag(&self, nodes: &[Node], features: &[NodeFeatures]) -> Vec<TaggedNode> {
        self.decode(nodes, features)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classifier::HeuristicClassifier;
    use crate::config::ScoringConfig;
    use crate::features::KeywordHit;
    use crate::model::BlockType;

    #[test]
    fn embedded_model_loads_and_is_valid() {
        let model = CrfModel::embedded();
        assert!(!model.labels.is_empty(), "Labels should not be empty");
        assert_eq!(model.labels[0], "O", "First label must be 'O'");

        let k = model.labels.len();
        assert_eq!(model.bio_tags.len(), k);
        assert_eq!(model.transitions.len(), k);
        for row in &model.transitions {
            assert_eq!(row.len(), k);
        }

        assert!(
            !model.state_features.is_empty(),
            "State features should not be empty"
        );
        assert!(model.state_features.contains_key("bias"));
    }

    #[test]
    fn feature_functions_parity() {
        assert_eq!(offset_prefix(-1), "-1:");
        assert_eq!(offset_prefix(0), "0:");
        assert_eq!(offset_prefix(1), "+1:");

        assert_eq!(bucket_math_density(0.0), "zero");
        assert_eq!(bucket_math_density(0.0005), "zero");
        assert_eq!(bucket_math_density(0.05), "low");
        assert_eq!(bucket_math_density(0.30), "low");
        assert_eq!(bucket_math_density(0.50), "medium");
        assert_eq!(bucket_math_density(0.70), "medium");
        assert_eq!(bucket_math_density(0.85), "high");

        assert_eq!(bucket_logic_density(0.0), "zero");
        assert_eq!(bucket_logic_density(0.10), "low");
        assert_eq!(bucket_logic_density(0.35), "low");
        assert_eq!(bucket_logic_density(0.50), "medium");
        assert_eq!(bucket_logic_density(0.70), "medium");
        assert_eq!(bucket_logic_density(0.90), "high");

        assert_eq!(get_math_density_delta(0.5, None), "none");
        assert_eq!(get_math_density_delta(0.6, Some(0.5)), "up");
        assert_eq!(get_math_density_delta(0.4, Some(0.5)), "down");
        assert_eq!(get_math_density_delta(0.51, Some(0.50)), "flat");

        assert!(node_ends_with_colon("Theorem 1:"));
        assert!(node_ends_with_colon("قضیه ۱: "));
        assert!(node_ends_with_colon("لم ۲："));
        assert!(!node_ends_with_colon("Prose without colon."));
    }

    #[test]
    fn extract_node_features_exact_keys() {
        let dump_node = DumpNode {
            index: 0,
            kind: "paragraph".to_string(),
            flat_text: "Theorem 1.1: Let $X$ be compact.".to_string(),
            char_len: 32,
            env_span: None,
        };

        let feat = NodeFeatures {
            lead: Some(KeywordHit {
                kind: BlockType::Theorem,
                number: Some("1.1".to_string()),
                name: None,
                labelled: true,
                body_empty: false,
                reference: false,
                refers_to: None,
            }),
            math_density: 0.25,
            logic_density: 0.0,
            emphasized: true,
            ..Default::default()
        };

        let nodes = vec![dump_node];
        let features = vec![feat];

        let keys = extract_node_features(&nodes, &features, 0);

        assert!(keys.contains(&"bias".to_string()));
        assert!(keys.contains(&"0:is_first_node=true".to_string()));
        assert!(keys.contains(&"0:is_last_node=true".to_string()));
        assert!(keys.contains(&"0:math_density_delta=none".to_string()));
        assert!(keys.contains(&"0:node_kind=paragraph".to_string()));
        assert!(keys.contains(&"0:has_lead=true".to_string()));
        assert!(keys.contains(&"0:lead=theorem".to_string()));
        assert!(!keys.contains(&"0:lead_kind=theorem".to_string()));
        assert!(!keys.contains(&"0:lead_labelled=true".to_string()));
        assert!(keys.contains(&"0:labelled=true".to_string()));
        assert!(keys.contains(&"0:lead_has_number=true".to_string()));
        assert!(keys.contains(&"0:emphasized=true".to_string()));
        assert!(keys.contains(&"0:math_density=low".to_string()));
        assert!(keys.contains(&"0:logic_density=zero".to_string()));
    }

    #[test]
    fn boundary_scope_feature_keys_exact() {
        // Mirrors `test_boundary_scope_feature_keys_exact` in
        // `ml/tests/test_features.py`; both must produce identical key sets.
        let nodes = vec![
            DumpNode {
                index: 0,
                kind: "paragraph".to_string(),
                flat_text: "Theorem 1: Let $X$ be compact.".to_string(),
                char_len: 30,
                env_span: None,
            },
            DumpNode {
                index: 1,
                kind: "paragraph".to_string(),
                flat_text: "then $f$ is bounded,".to_string(),
                char_len: 20,
                env_span: None,
            },
        ];
        let features = vec![
            NodeFeatures {
                lead: Some(KeywordHit {
                    kind: BlockType::Theorem,
                    number: Some("1".to_string()),
                    name: None,
                    labelled: true,
                    body_empty: false,
                    reference: false,
                    refers_to: None,
                }),
                math_density: 0.25,
                ..Default::default()
            },
            NodeFeatures::default(),
        ];

        let mut node0 = extract_node_features(&nodes, &features, 0);
        node0.sort();
        assert_eq!(
            node0,
            vec![
                "+1:has_lead=false",
                "+1:logic_density=zero",
                "+1:math_density=zero",
                "+1:node_kind=paragraph",
                "0:char_len_bucket=tiny",
                "0:dist_from_lead=0",
                "0:has_lead=true",
                "0:is_first_node=true",
                "0:labelled=true",
                "0:lead=theorem",
                "0:lead_has_number=true",
                "0:logic_density=zero",
                "0:math_density=low",
                "0:math_density_delta=none",
                "0:node_kind=paragraph",
                "bias",
            ]
        );

        let mut node1 = extract_node_features(&nodes, &features, 1);
        node1.sort();
        assert_eq!(
            node1,
            vec![
                "-1:has_lead=true",
                "-1:labelled=true",
                "-1:lead=theorem",
                "-1:lead_has_number=true",
                "-1:logic_density=zero",
                "-1:math_density=low",
                "-1:node_kind=paragraph",
                "0:char_len_bucket=tiny",
                "0:dist_from_lead=1",
                "0:has_lead=false",
                "0:is_last_node=true",
                "0:logic_density=zero",
                "0:math_density=zero",
                "0:math_density_delta=down",
                "0:node_kind=paragraph",
                "0:starts_lowercase=true",
                "bias",
            ]
        );
    }

    #[test]
    fn boundary_scope_feature_helpers() {
        assert_eq!(bucket_char_len(0), "tiny");
        assert_eq!(bucket_char_len(40), "tiny");
        assert_eq!(bucket_char_len(41), "short");
        assert_eq!(bucket_char_len(140), "short");
        assert_eq!(bucket_char_len(141), "medium");
        assert_eq!(bucket_char_len(400), "medium");
        assert_eq!(bucket_char_len(401), "long");

        assert!(starts_with_ascii_lowercase("  then x"));
        assert!(!starts_with_ascii_lowercase("Then x"));
        assert!(!starts_with_ascii_lowercase("$x$"));
        assert!(!starts_with_ascii_lowercase(""));

        assert!(starts_with_math("$$x$$"));
        assert!(starts_with_math("  $x$"));
        assert!(!starts_with_math("x"));

        assert!(ends_with_continuation_punct("a,"));
        assert!(ends_with_continuation_punct("a،"));
        assert!(ends_with_continuation_punct("a:"));
        assert!(ends_with_continuation_punct("a："));
        assert!(!ends_with_continuation_punct("a."));

        assert_eq!(bucket_dist_from_lead(0), "0");
        assert_eq!(bucket_dist_from_lead(2), "2");
        assert_eq!(bucket_dist_from_lead(3), "3_5");
        assert_eq!(bucket_dist_from_lead(5), "3_5");
        assert_eq!(bucket_dist_from_lead(6), "6_plus");
    }

    #[test]
    fn crf_tagger_decodes_empty_and_single_node() {
        let tagger = CrfTagger::new();

        let empty_nodes: Vec<DumpNode> = Vec::new();
        let empty_feats: Vec<NodeFeatures> = Vec::new();
        let res = tagger.decode_nodes(&empty_nodes, &empty_feats);
        assert!(res.is_empty());

        let node = DumpNode {
            index: 0,
            kind: "paragraph".to_string(),
            flat_text: "Simple text paragraph.".to_string(),
            char_len: 22,
            env_span: None,
        };
        let feat = NodeFeatures::default();

        let res = tagger.decode_nodes(&[node], &[feat]);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].tag, BioTag::Outside);
        assert!(res[0].confidence >= 0.0);
    }

    #[test]
    fn crf_tagger_decodes_theorem_and_proof() {
        let tagger = CrfTagger::new();
        let markdown = "# Section\n\n**Theorem 1.** For every $x \\in X$, $f(x) \\ge 0$.\n\nProof. This is trivial. $\\blacksquare$";
        let config = ScoringConfig::default();
        let lexicon = crate::features::Lexicon::new(&config);
        let nodes = crate::parser::parse_markdown(&crate::normalize::normalize_markdown(markdown));
        let features: Vec<NodeFeatures> = nodes.iter().map(|n| lexicon.extract(n)).collect();

        let tags = tagger.decode(&nodes, &features);
        assert_eq!(tags.len(), 3);
        assert_eq!(tags[0].tag, BioTag::Outside); // # Section
        assert_eq!(tags[1].tag, BioTag::Begin(BlockType::Theorem)); // **Theorem 1.** ...
        assert!(tags[1].confidence > 0.0);
        assert_eq!(tags[2].tag, BioTag::Begin(BlockType::Proof)); // Proof. ...
        assert!(tags[2].confidence > 0.0);
    }

    #[test]
    fn crf_tagger_integrates_with_heuristic_classifier() {
        let tagger = CrfTagger::new();
        let markdown = "# Section\n\n**Theorem 1.** For every $x \\in X$, $f(x) \\ge 0$.\n\nProof. This is trivial. $\\blacksquare$";
        let config = ScoringConfig::default();
        let classifier = HeuristicClassifier::new(config);

        let nodes = crate::parser::parse_markdown(&crate::normalize::normalize_markdown(markdown));
        let classification = classifier.classify_with_tagger(
            &nodes,
            &crate::classifier::ScopeOverrides::default(),
            &tagger,
        );

        assert!(!classification.blocks.is_empty());
        let kinds: Vec<BlockType> = classification.blocks.iter().map(|b| b.kind).collect();
        assert!(kinds.contains(&BlockType::Theorem));
        assert!(kinds.contains(&BlockType::Proof));
    }

    #[test]
    fn crf_tagger_latency_benchmark_500_nodes() {
        let tagger = CrfTagger::new();

        // Synthetic 500-node document alternating prose, theorems, and proofs
        let mut nodes = Vec::with_capacity(500);
        let mut features = Vec::with_capacity(500);

        for i in 0..500 {
            let (kind_str, flat, feat) = match i % 10 {
                0 => ("heading", "## Chapter Heading", NodeFeatures::default()),
                1 => {
                    let f = NodeFeatures {
                        lead: Some(KeywordHit {
                            kind: BlockType::Theorem,
                            number: Some(format!("{i}")),
                            name: None,
                            labelled: true,
                            body_empty: false,
                            reference: false,
                            refers_to: None,
                        }),
                        emphasized: true,
                        ..Default::default()
                    };
                    ("paragraph", "**Theorem.** Let $X$ be complete.", f)
                }
                2 => {
                    let f = NodeFeatures {
                        math_density: 0.5,
                        ..Default::default()
                    };
                    ("paragraph", "Then $f(x) = 0$ holds identically.", f)
                }
                3 => {
                    let f = NodeFeatures {
                        lead: Some(KeywordHit {
                            kind: BlockType::Proof,
                            number: None,
                            name: None,
                            labelled: true,
                            body_empty: false,
                            reference: false,
                            refers_to: None,
                        }),
                        qed: true,
                        ..Default::default()
                    };
                    ("paragraph", "Proof. Clear by inspection. □", f)
                }
                _ => (
                    "paragraph",
                    "General exposition discussing mathematical methods.",
                    NodeFeatures::default(),
                ),
            };

            nodes.push(DumpNode {
                index: i,
                kind: kind_str.to_string(),
                flat_text: flat.to_string(),
                char_len: flat.len(),
                env_span: None,
            });
            features.push(feat);
        }

        let start = std::time::Instant::now();
        let tags = tagger.decode_nodes(&nodes, &features);
        let elapsed = start.elapsed();

        assert_eq!(tags.len(), 500);
        println!("CRF 500-node inference elapsed time: {:?}", elapsed);
        assert!(
            elapsed.as_millis() < 50,
            "CRF inference must be fast (< 50 ms in debug mode, < 3 ms in release), took {:?}",
            elapsed
        );
    }

    #[test]
    fn crf_python_parity_and_accuracy_on_test_dataset() {
        let test_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ml/data/test.jsonl");
        let expected_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../ml/data/test_viterbi_expected.json");

        if !test_path.exists() {
            println!("Skipping test: test.jsonl not found at {:?}", test_path);
            return;
        }

        let test_content = std::fs::read_to_string(&test_path).expect("read test.jsonl");
        let py_expected: Option<Vec<Vec<String>>> = if expected_path.exists() {
            let s = std::fs::read_to_string(&expected_path).expect("read expected");
            Some(serde_json::from_str(&s).expect("parse expected"))
        } else {
            None
        };

        let tagger = CrfTagger::new();

        let mut total_tokens = 0;
        let mut matching_truth = 0;
        let mut matching_py = 0;
        let mut doc_idx = 0;

        for line in test_content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            #[derive(serde::Deserialize)]
            struct TestDoc {
                pub nodes: Vec<DumpNode>,
                pub features: Vec<NodeFeatures>,
                pub bio_tags: Option<Vec<String>>,
            }

            let doc: TestDoc = match serde_json::from_str(line) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("Failed to parse line in test.jsonl: {e}");
                    continue;
                }
            };

            let preds = tagger.decode_nodes(&doc.nodes, &doc.features);

            if let Some(ground_truth) = &doc.bio_tags
                && ground_truth.len() == doc.nodes.len()
            {
                for (pred, truth) in preds.iter().zip(ground_truth) {
                    total_tokens += 1;
                    if pred.tag.to_label() == *truth {
                        matching_truth += 1;
                    }
                }
            }

            if let Some(expected_list) = &py_expected
                && doc_idx < expected_list.len()
            {
                let exp = &expected_list[doc_idx];
                for (pred, py_tag) in preds.iter().zip(exp) {
                    if pred.tag.to_label() == *py_tag {
                        matching_py += 1;
                    }
                }
            }

            doc_idx += 1;
        }

        if total_tokens > 0 {
            let accuracy = (matching_truth as f64) / (total_tokens as f64) * 100.0;
            println!(
                "CRF accuracy on test.jsonl across {} docs: {:.2}% ({}/{})",
                doc_idx, accuracy, matching_truth, total_tokens
            );
            assert!(
                accuracy >= 80.0,
                "CRF accuracy on test.jsonl should be >= 80%, got {:.2}%",
                accuracy
            );

            assert!(
                py_expected.is_some(),
                "test_viterbi_expected.json must exist for parity verification"
            );
            let parity = (matching_py as f64) / (total_tokens as f64) * 100.0;
            println!(
                "Rust-to-Python parity on test.jsonl: {:.2}% ({}/{})",
                parity, matching_py, total_tokens
            );
            assert!(
                parity >= 99.9,
                "Rust CrfTagger must achieve >= 99.9% parity with Python, got {:.2}%",
                parity
            );
        }
    }
}
