//! A static decision tree over [`Features`]: the alternative to the linear
//! weighted sum, selected with [`crate::config::Scorer::Tree`].
//!
//! The tree is plain data (serde), evaluated once per (node, candidate type).
//! Every test is a yes/no question about the node's features *relative to the
//! candidate type* — "does the node start with a keyword of this type?" — so
//! one tree scores all types. Leaves hold the score.
//!
//! Like the linear scorer it is pure: same features, same tree, same score.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::features::Features;
use crate::model::BlockType;

/// Deepest tree [`DecisionTree::validate`] accepts; keeps evaluation and
/// recursive (de)serialisation bounded.
pub const MAX_TREE_DEPTH: usize = 32;

/// `[scorer] model = "tree"` with no `root` selects [`DecisionTree::default`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DecisionTree {
    pub root: TreeNode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "node", rename_all = "snake_case")]
pub enum TreeNode {
    Split {
        test: Test,
        yes: Box<TreeNode>,
        no: Box<TreeNode>,
    },
    Leaf {
        score: f32,
    },
}

/// A question about the features, asked for one candidate type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "feature", rename_all = "snake_case")]
pub enum Test {
    /// The node starts with a keyword of the candidate type (not a reference
    /// such as "Theorem 2 implies…").
    Lead,
    /// …and the keyword reads as a label.
    Labelled,
    /// The node starts with bold / italic text.
    Emphasized,
    /// A keyword of the candidate type appears anywhere in the node.
    Mention,
    Qed,
    ProofCue,
    StatementCue,
    Definitional,
    Continuation,
    /// The candidate type is one of these.
    Kind {
        kinds: Vec<BlockType>,
    },
    /// The candidate type is a statement (theorem, lemma, …, claim).
    Statement,
    MathAtLeast {
        value: f32,
    },
    LogicAtLeast {
        value: f32,
    },
}

impl Test {
    fn holds(&self, f: &Features, kind: BlockType) -> bool {
        let lead = f
            .lead
            .as_ref()
            .filter(|hit| hit.kind == kind && !hit.reference);
        match self {
            Test::Lead => lead.is_some(),
            Test::Labelled => lead.is_some_and(|hit| hit.labelled),
            Test::Emphasized => f.emphasized,
            Test::Mention => f.mentions.contains(&kind),
            Test::Qed => f.qed,
            Test::ProofCue => f.proof_cue,
            Test::StatementCue => f.statement_cue,
            Test::Definitional => f.definitional,
            Test::Continuation => f.continuation,
            Test::Kind { kinds } => kinds.contains(&kind),
            Test::Statement => kind.is_statement(),
            Test::MathAtLeast { value } => f.math_density >= *value,
            Test::LogicAtLeast { value } => f.logic_density >= *value,
        }
    }
}

fn split(test: Test, yes: TreeNode, no: TreeNode) -> TreeNode {
    TreeNode::Split {
        test,
        yes: Box::new(yes),
        no: Box::new(no),
    }
}

fn leaf(score: f32) -> TreeNode {
    TreeNode::Leaf { score }
}

impl Default for DecisionTree {
    /// A hand-built tree that makes the same opening decisions as the default
    /// linear weights on typical input: a labelled or emphasised keyword
    /// opens, a bare keyword needs type-specific support, and without a
    /// keyword only a strongly argued, QED-terminated paragraph opens a proof.
    fn default() -> Self {
        use BlockType::*;
        let support = split(
            Test::Kind { kinds: vec![Proof] },
            split(
                Test::Qed,
                leaf(0.75),
                split(Test::ProofCue, leaf(0.55), leaf(0.45)),
            ),
            split(
                Test::Kind {
                    kinds: vec![Definition, Notation],
                },
                split(Test::Definitional, leaf(0.7), leaf(0.45)),
                split(
                    Test::Statement,
                    split(
                        Test::StatementCue,
                        leaf(0.55),
                        split(Test::MathAtLeast { value: 0.5 }, leaf(0.5), leaf(0.45)),
                    ),
                    leaf(0.45),
                ),
            ),
        );
        let keyword = split(
            Test::Labelled,
            split(Test::Emphasized, leaf(0.95), leaf(0.8)),
            split(Test::Emphasized, leaf(0.6), support),
        );
        let no_keyword = split(
            Test::Kind { kinds: vec![Proof] },
            split(
                Test::Qed,
                split(
                    Test::LogicAtLeast { value: 0.6 },
                    split(Test::ProofCue, leaf(0.6), leaf(0.45)),
                    leaf(0.35),
                ),
                split(Test::Mention, leaf(0.1), leaf(0.0)),
            ),
            split(Test::Mention, leaf(0.1), leaf(0.0)),
        );
        DecisionTree {
            root: split(Test::Lead, keyword, no_keyword),
        }
    }
}

impl DecisionTree {
    pub fn score(&self, features: &Features, kind: BlockType) -> f32 {
        let mut node = &self.root;
        loop {
            match node {
                TreeNode::Leaf { score } => return score.clamp(0.0, 1.0),
                TreeNode::Split { test, yes, no } => {
                    node = if test.holds(features, kind) { yes } else { no };
                }
            }
        }
    }

    pub fn validate(&self) -> Result<()> {
        fn walk(node: &TreeNode, depth: usize, path: &mut String) -> Result<()> {
            let fail =
                |message: String| Err(Error::Config(format!("scorer.root{path}: {message}")));
            if depth > MAX_TREE_DEPTH {
                return fail(format!("tree is deeper than {MAX_TREE_DEPTH}"));
            }
            match node {
                TreeNode::Leaf { score } => {
                    if !(0.0..=1.0).contains(score) {
                        return fail(format!("leaf score must be in [0, 1], got {score}"));
                    }
                }
                TreeNode::Split { test, yes, no } => {
                    match test {
                        Test::Kind { kinds } => {
                            if kinds.is_empty() || kinds.iter().any(|k| !k.is_semantic()) {
                                return fail("`kind` needs a non-empty list of block types".into());
                            }
                        }
                        Test::MathAtLeast { value } | Test::LogicAtLeast { value }
                            if !(0.0..=1.0).contains(value) =>
                        {
                            return fail(format!("threshold must be in [0, 1], got {value}"));
                        }
                        _ => {}
                    }
                    let len = path.len();
                    path.push_str(".yes");
                    walk(yes, depth + 1, path)?;
                    path.truncate(len);
                    path.push_str(".no");
                    walk(no, depth + 1, path)?;
                    path.truncate(len);
                }
            }
            Ok(())
        }
        walk(&self.root, 0, &mut String::new())
    }
}
