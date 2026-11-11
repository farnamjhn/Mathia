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

use crate::config::ScoringConfig;
use crate::crossref;
use crate::features::{Features, Lexicon};
use crate::model::{
    BlockType, Direction, EndReason, Node, NodeKind, SemanticBlock, document_direction,
};
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

/// Blocks plus everything needed to explain them.
#[derive(Debug, Clone, PartialEq)]
pub struct Classification {
    pub blocks: Vec<SemanticBlock>,
    /// One entry per node.
    pub scores: Vec<NodeScore>,
    /// For every node, the index in `blocks` of the block that contains it.
    pub block_of: Vec<usize>,
}

/// Externally requested corrections to the default scoping, keyed by the
/// index of a block's opener node. An empty value reproduces [`HeuristicClassifier::classify`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopeOverrides {
    /// Nodes to add to (positive) or drop from (negative) the block's end.
    /// Ignored for LaTeX environments, whose extent is explicit.
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

fn on(flag: bool) -> f32 {
    if flag { 1.0 } else { 0.0 }
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

        let mut score = match &features.lead {
            // "Theorem 2 implies…" talks about the theorem: a mention, no more.
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

    pub fn score_node(&self, node: &Node) -> NodeScore {
        let features = self.lexicon.extract(node);
        let scores: Vec<(BlockType, f32)> = BlockType::SEMANTIC
            .iter()
            .map(|&kind| (kind, self.score(&features, kind)))
            .collect();
        let mut best = (BlockType::Theorem, f32::MIN);
        for &(kind, s) in &scores {
            if s > best.1 {
                best = (kind, s);
            }
        }
        let can_open = matches!(
            node.kind,
            NodeKind::Paragraph | NodeKind::Heading(_) | NodeKind::Quote
        );
        NodeScore {
            opens: can_open && best.1 >= self.config.threshold,
            features,
            scores,
            best,
        }
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
        let n = nodes.len();
        let scores: Vec<NodeScore> = nodes.iter().map(|node| self.score_node(node)).collect();

        // LaTeX environments: `env_first` opens, `covered` belongs to one.
        let mut env_first = vec![false; n];
        let mut covered = vec![false; n];
        let mut i = 0;
        while i < n {
            match &nodes[i].env {
                Some(env) if !overrides.suppressed.contains(&i) => {
                    let end = (i + env.len.max(1)).min(n);
                    env_first[i] = true;
                    covered[i + 1..end].fill(true);
                    i = end;
                }
                _ => i += 1,
            }
        }

        let opens = |i: usize| {
            !covered[i] && (env_first[i] || (scores[i].opens && !overrides.suppressed.contains(&i)))
        };
        let is_heading = |i: usize| matches!(nodes[i].kind, NodeKind::Heading(_)) && !covered[i];
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
            let adjust = |natural: usize, reason: EndReason| {
                let delta = overrides.delta.get(&i).copied().unwrap_or(0);
                let end = (natural as isize + delta).clamp(i as isize + 1, limit as isize) as usize;
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
                };
                builder.push(env.kind, i..end, opener, EndReason::Environment);
                i = end;
            } else if opens(i) {
                let (kind, score) = scores[i].best;
                let (natural, reason) = match kind {
                    BlockType::Proof => self.argument_end(&scores, i, limit, at_limit),
                    // Solutions rarely end in a QED mark; without one they are
                    // scoped like a statement instead of running to the cap.
                    BlockType::Solution => match self.argument_end(&scores, i, limit, at_limit) {
                        (end, EndReason::Qed) => (end, EndReason::Qed),
                        _ => self.statement_end(nodes, &scores, i, limit, at_limit),
                    },
                    _ => self.statement_end(nodes, &scores, i, limit, at_limit),
                };
                let (end, reason) = adjust(natural, reason);
                let opener = Opener {
                    score,
                    features,
                    env_name: None,
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
            } else if !overrides.suppressed.contains(&i)
                && let Some(score) =
                    self.implicit_proof_score(nodes, &scores, builder.blocks.last(), i, limit)
            {
                let (natural, reason) = self.argument_end(&scores, i, limit, at_limit);
                let (end, reason) = adjust(natural, reason);
                let opener = Opener {
                    score,
                    features,
                    env_name: None,
                };
                builder.push(BlockType::Proof, i..end, opener, reason);
                i = end;
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
            scores,
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
            let stuck = next
                .iter()
                .map(|b| (&b.span, b.kind))
                .eq(blocks.iter().map(|b| (&b.span, b.kind)));
            blocks = next;
            if stuck {
                // The requested change hit a hard boundary; asking again cannot help.
                break;
            }
        }
        blocks
    }

    /// Score of node `i` as a proof that has no "Proof." keyword: an ordinary
    /// paragraph directly after a statement, with a QED marker before the next
    /// boundary and/or a strong proof profile. `None` below the threshold.
    fn implicit_proof_score(
        &self,
        nodes: &[Node],
        scores: &[NodeScore],
        previous: Option<&SemanticBlock>,
        i: usize,
        limit: usize,
    ) -> Option<f32> {
        if !previous?.kind.is_statement() || nodes[i].kind != NodeKind::Paragraph {
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

    /// A proof or solution runs through its QED marker, or to `limit` / the
    /// size cap.
    fn argument_end(
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

    /// A statement keeps the display math, lists and quotes that follow it,
    /// plus paragraphs that visibly continue it.
    fn statement_end(
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
                NodeKind::DisplayMath | NodeKind::List | NodeKind::Quote | NodeKind::Table => true,
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
}

impl Opener<'_> {
    /// Headings and plain text.
    const NONE: Opener<'static> = Opener {
        score: 0.0,
        features: None,
        env_name: None,
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
            .filter(|hit| semantic && hit.kind == kind);
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
        overrides.delta.insert(0, -1);
        assert_eq!(c.classify_with_overrides(&nodes, &overrides)[0].span, 0..3);
        overrides.suppressed.insert(0);
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
        overrides.delta.insert(0, 1);
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
}
