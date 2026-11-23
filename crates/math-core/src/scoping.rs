//! Deterministic structural scoping for block openers.
//!
//! The CRF tagger predicts where a block *starts*; these helpers decide how
//! far a strongly labelled opener extends, independent of the tagger's `I-`
//! run. They are pure functions of the parsed nodes, the static features and
//! the configured scope limits.

use crate::config::ScopeConfig;
use crate::features::NodeFeatures;
use crate::model::{EndReason, Node, NodeKind};

/// End of an argument (proof or solution): the first QED marker, or the
/// argument size limit.
pub fn argument_end(
    features: &[NodeFeatures],
    scope: &ScopeConfig,
    start: usize,
    limit: usize,
    at_limit: EndReason,
) -> (usize, EndReason) {
    let cap = (start + scope.proof_max_nodes.max(1)).min(limit);
    match (start..cap).find(|&j| features[j].qed) {
        Some(j) => (j + 1, EndReason::Qed),
        None if cap == limit => (cap, at_limit),
        None => (cap, EndReason::SizeLimit),
    }
}

/// End of a statement-like block: display math, lists and continuations are
/// absorbed up to the statement size limit.
pub fn statement_end(
    nodes: &[Node],
    features: &[NodeFeatures],
    scope: &ScopeConfig,
    start: usize,
    limit: usize,
    at_limit: EndReason,
) -> (usize, EndReason) {
    let cap = (start + scope.statement_max_nodes.max(1)).min(limit);
    let mut needs_body = features[start]
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
                    || features[end].continuation
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
