//! Deterministic block classification.
//!
//! Two stages, both pure:
//!
//! 1. **Scoring** — every node gets a weighted feature score per [`BlockType`];
//!    a node whose best score reaches [`ScoringConfig::threshold`] is an
//!    *opener*.
//! 2. **Scoping** — each opener absorbs the nodes after it until a terminator
//!    (QED marker, next opener, heading, or a size limit).
//!
//! Scope is the part a later decision model may disagree with, so it is
//! adjustable from outside through [`ScopeOverrides`] without touching the
//! scoring: see [`BlockJudge`] and [`HeuristicClassifier::classify_with_judge`].

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use crate::config::ScoringConfig;
use crate::features::{Features, Lexicon};
use crate::model::{BlockType, Direction, Node, NodeKind, SemanticBlock, document_direction};

/// Scoring result for one node.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeScore {
    pub features: Features,
    /// Best semantic type and its score, whether or not it passes the threshold.
    pub best: (BlockType, f32),
    /// The node may open a block: right kind of node and score ≥ threshold.
    pub opens: bool,
}

/// Externally requested corrections to the default scoping, keyed by the
/// index of a block's opener node. An empty value reproduces [`HeuristicClassifier::classify`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopeOverrides {
    /// Nodes to add to (positive) or drop from (negative) the block's end.
    pub delta: BTreeMap<usize, isize>,
    /// Opener nodes to treat as ordinary content instead.
    pub suppressed: BTreeSet<usize>,
}

/// What a decision model thinks of one proposed block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The block is a complete unit of its type.
    Accept,
    /// It stops too early: widen the scope by one node and ask again.
    Incomplete,
    /// It runs on into unrelated content: narrow the scope by one node.
    Overreach,
    /// It is not a block of this type at all: drop the opener.
    Reject,
}

/// Seam for decision models. `math-core` only defines the contract; the
/// implementations (local models, remote calls) live outside this crate.
pub trait BlockJudge {
    fn judge(&self, block: &SemanticBlock, nodes: &[Node]) -> Verdict;
}

#[derive(Debug, Clone)]
pub struct HeuristicClassifier {
    config: ScoringConfig,
    lexicon: Lexicon,
}

impl Default for HeuristicClassifier {
    fn default() -> Self {
        Self::new(ScoringConfig::default())
    }
}

impl HeuristicClassifier {
    pub fn new(config: ScoringConfig) -> Self {
        let lexicon = Lexicon::new(&config);
        HeuristicClassifier { config, lexicon }
    }

    pub fn config(&self) -> &ScoringConfig {
        &self.config
    }

    /// Weighted score of `features` for one candidate type, in `[0, 1]`.
    pub fn score(&self, features: &Features, kind: BlockType) -> f32 {
        let w = &self.config.weights;
        let on = |flag: bool| if flag { 1.0 } else { 0.0 };

        let mut score = match &features.lead {
            Some(hit) if hit.kind == kind => {
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
            BlockType::Definition => {
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

    pub fn score_node(&self, node: &Node) -> NodeScore {
        let features = self.lexicon.extract(node);
        let mut best = (BlockType::Theorem, f32::MIN);
        for kind in BlockType::SEMANTIC {
            let s = self.score(&features, kind);
            if s > best.1 {
                best = (kind, s);
            }
        }
        let can_open =
            matches!(node.kind, NodeKind::Paragraph | NodeKind::Heading(_) | NodeKind::Quote);
        NodeScore { opens: can_open && best.1 >= self.config.threshold, features, best }
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
        let scores: Vec<NodeScore> = nodes.iter().map(|n| self.score_node(n)).collect();
        let opens = |i: usize| scores[i].opens && !overrides.suppressed.contains(&i);
        let is_heading = |i: usize| matches!(nodes[i].kind, NodeKind::Heading(_));
        let boundary = |i: usize| opens(i) || is_heading(i);

        let mut builder = Builder::new(nodes);
        let mut i = 0;
        while i < nodes.len() {
            // Nothing absorbs past the next opener or heading.
            let limit = (i + 1..nodes.len()).find(|&j| boundary(j)).unwrap_or(nodes.len());

            if opens(i) {
                let kind = scores[i].best.0;
                let natural = if kind == BlockType::Proof {
                    self.proof_end(&scores, i, limit)
                } else {
                    self.statement_end(nodes, &scores, i, limit)
                };
                let delta = overrides.delta.get(&i).copied().unwrap_or(0);
                let end = (natural as isize + delta).clamp(i as isize + 1, limit as isize) as usize;
                builder.push(kind, i..end, Some(&scores[i]));
                i = end;
            } else if is_heading(i) {
                builder.push(BlockType::Heading, i..i + 1, None);
                i += 1;
            } else {
                builder.push(BlockType::Text, i..limit, None);
                i = limit;
            }
        }
        builder.blocks
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
        let mut overrides = ScopeOverrides::default();
        let mut accepted: BTreeSet<(usize, usize)> = BTreeSet::new();
        let mut blocks = self.classify_with_overrides(nodes, &overrides);

        for _ in 0..max_rounds {
            let mut changed = false;
            for block in blocks.iter().filter(|b| b.kind.is_semantic()) {
                let key = (block.span.start, block.span.end);
                if accepted.contains(&key) {
                    continue;
                }
                let opener = block.span.start;
                match judge.judge(block, nodes) {
                    Verdict::Accept => {
                        accepted.insert(key);
                    }
                    Verdict::Incomplete => {
                        *overrides.delta.entry(opener).or_insert(0) += 1;
                        changed = true;
                    }
                    Verdict::Overreach => {
                        *overrides.delta.entry(opener).or_insert(0) -= 1;
                        changed = true;
                    }
                    Verdict::Reject => {
                        overrides.suppressed.insert(opener);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
            let next = self.classify_with_overrides(nodes, &overrides);
            let stuck = next.iter().map(|b| (&b.span, b.kind)).eq(blocks.iter().map(|b| (&b.span, b.kind)));
            blocks = next;
            if stuck {
                // The requested change hit a hard boundary; asking again cannot help.
                break;
            }
        }
        blocks
    }

    /// A proof runs through its QED marker, or to `limit` / the size cap.
    fn proof_end(&self, scores: &[NodeScore], start: usize, limit: usize) -> usize {
        let cap = (start + self.config.scope.proof_max_nodes.max(1)).min(limit);
        (start..cap).find(|&j| scores[j].features.qed).map_or(cap, |j| j + 1)
    }

    /// A statement keeps the display math, lists and quotes that follow it,
    /// plus paragraphs that visibly continue it.
    fn statement_end(
        &self,
        nodes: &[Node],
        scores: &[NodeScore],
        start: usize,
        limit: usize,
    ) -> usize {
        let cap = (start + self.config.scope.statement_max_nodes.max(1)).min(limit);
        let mut needs_body =
            scores[start].features.lead.as_ref().is_some_and(|hit| hit.body_empty);
        let mut end = start + 1;
        while end < cap {
            let take = match nodes[end].kind {
                NodeKind::DisplayMath | NodeKind::List | NodeKind::Quote | NodeKind::Table => true,
                NodeKind::Paragraph => {
                    needs_body
                        || scores[end].features.continuation
                        || nodes[end - 1].flat.trim_end().ends_with([':', '،', ','])
                }
                _ => false,
            };
            if !take {
                break;
            }
            if nodes[end].kind == NodeKind::Paragraph {
                needs_body = false;
            }
            end += 1;
        }
        end
    }
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

    fn push(&mut self, kind: BlockType, span: Range<usize>, score: Option<&NodeScore>) {
        let counter = self.counters.entry(kind).or_insert(0);
        *counter += 1;
        let id = match kind {
            BlockType::Heading => format!("sec-{counter}"),
            _ => format!("block-{}-{counter}", kind.slug()),
        };

        let nodes = &self.nodes[span.clone()];
        let text: String = nodes.iter().map(|n| n.text.as_str()).collect::<Vec<_>>().join(" ");
        let direction = Direction::detect(&text).unwrap_or(self.fallback_dir);

        let heading_level = match (kind, nodes[0].kind) {
            (BlockType::Heading, NodeKind::Heading(level)) => Some(level),
            _ => None,
        };
        let lead = score.and_then(|s| s.features.lead.as_ref()).filter(|hit| hit.kind == kind);
        let number = lead.and_then(|hit| hit.number.clone());
        let name = lead.and_then(|hit| hit.name.clone());

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
        for node in nodes {
            match node.kind {
                // A heading that opened a semantic block becomes its title line.
                NodeKind::Heading(_) if kind != BlockType::Heading => {
                    html.push_str(&format!("<p class=\"mx-block-title\">{}</p>\n", node.html));
                }
                _ => html.push_str(&node.html),
            }
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
            score: score.map_or(0.0, |s| s.best.1),
            features: score.map(|s| s.features.clone()),
            html,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            ("لم ۲: هر دنبالهٔ کراندار زیردنبالهٔ همگرا دارد.", BlockType::Lemma),
            ("**تعریف.** مجموعهٔ $A$ را باز گوییم هرگاه ...", BlockType::Definition),
            ("برهان. فرض کنید $x \\in A$.", BlockType::Proof),
            ("اثبات: بدیهی است.", BlockType::Proof),
            ("گزاره ۳. هر گروه دوری آبلی است.", BlockType::Proposition),
            ("نتیجه ۴. پس $f$ کراندار است.", BlockType::Corollary),
            ("مثال ۵. تابع $f(x)=x^2$ را در نظر بگیرید.", BlockType::Example),
            ("تذکر. عکس این حکم درست نیست.", BlockType::Remark),
            ("Theorem 1. Every bounded sequence has a convergent subsequence.", BlockType::Theorem),
            ("**Proof.** Suppose not.", BlockType::Proof),
            ("Definition 2. A set is open if ...", BlockType::Definition),
            ("Lemma 3 (Zorn). Every chain has an upper bound.", BlockType::Lemma),
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
    fn scores_are_weighted_sums() {
        let c = HeuristicClassifier::default();
        let w = &c.config().weights;
        let s = best("**Proof.** Suppose not. Then $a \\implies b$, a contradiction.");
        let expected = w.lead + w.label + w.emphasis + w.logic / 3.0 + w.cue;
        assert_eq!(s.best.0, BlockType::Proof);
        assert!((s.best.1 - expected).abs() < 1e-6, "{} vs {expected}", s.best.1);
    }

    #[test]
    fn threshold_is_configurable() {
        let nodes = parse_markdown("Note that $f$ is continuous.");
        let strict = HeuristicClassifier::default();
        assert_eq!(kinds(&strict.classify(&nodes)), [BlockType::Text]);

        let lenient = HeuristicClassifier::new(ScoringConfig { threshold: 0.4, ..Default::default() });
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
        assert_eq!(kinds(&blocks), [BlockType::Theorem, BlockType::Proof, BlockType::Text]);
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
            ["block-lemma-1", "block-proof-1", "block-lemma-2", "block-proof-2", "block-theorem-1"]
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
        assert_eq!(c.classify_with_judge(&nodes, &WantNodes(2), 8)[0].span, 0..2);
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
                if block.kind == BlockType::Remark { Verdict::Reject } else { Verdict::Accept }
            }
        }
        let c = HeuristicClassifier::default();
        let nodes = parse_markdown("Theorem 1. A.\n\nNote: this is just prose.");
        assert_eq!(kinds(&c.classify(&nodes)), [BlockType::Theorem, BlockType::Remark]);
        assert_eq!(
            kinds(&c.classify_with_judge(&nodes, &RejectRemarks, 4)),
            [BlockType::Theorem, BlockType::Text]
        );
    }
}
