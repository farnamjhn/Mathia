"""Shared Feature Window Specification for Mathia Lightweight ML (Slice 2, Task 1).

Extracts joint opener and scoping features over a sliding window [i-1, i, i+1] for each node i:
- Static node features (node_kind, lead, labelled, emphasized, body_empty, qed,
  math_density bucketed, logic_density bucketed, cues, mentions, ends_colon).
- Relational / delta features (math_density_delta: up/down/flat,
  is_first_node, is_last_node, bias).

This specification is shared identically between Python training (sklearn-crfsuite)
and the pure-Rust Viterbi inference engine (crates/math-core/src/crf.rs).
"""

from __future__ import annotations

import argparse
import json
import logging
from pathlib import Path
from typing import Any, Mapping, Optional, Sequence, Union

logger = logging.getLogger(__name__)

from mathia_ml.schema import DumpNode, FeatureDump, LabeledDocument, NodeFeatures

FEATURE_SPEC_VERSION: str = "1.0.0"

# Offsets for the sliding context window
WINDOW_OFFSETS: tuple[int, ...] = (-1, 0, 1)

# Math and logic density bucket boundaries
MATH_ZERO_THRESHOLD: float = 0.001
MATH_LOW_THRESHOLD: float = 0.30
MATH_MED_THRESHOLD: float = 0.70

LOGIC_ZERO_THRESHOLD: float = 0.001
LOGIC_LOW_THRESHOLD: float = 0.35
LOGIC_MED_THRESHOLD: float = 0.70

# Delta threshold for math density change
MATH_DELTA_THRESHOLD: float = 0.05


def offset_prefix(offset: int) -> str:
    """Return canonical prefix for feature offset ('-1:', '0:', '+1:')."""
    if offset == -1:
        return "-1:"
    elif offset == 0:
        return "0:"
    elif offset == 1:
        return "+1:"
    elif offset > 0:
        return f"+{offset}:"
    else:
        return f"{offset}:"


def bucket_math_density(density: float) -> str:
    """Bucket continuous math density [0.0, 1.0] into discrete categories."""
    if density <= MATH_ZERO_THRESHOLD:
        return "zero"
    elif density <= MATH_LOW_THRESHOLD:
        return "low"
    elif density <= MATH_MED_THRESHOLD:
        return "medium"
    else:
        return "high"


def bucket_logic_density(density: float) -> str:
    """Bucket continuous logic density [0.0, 1.0] into discrete categories."""
    if density <= LOGIC_ZERO_THRESHOLD:
        return "zero"
    elif density <= LOGIC_LOW_THRESHOLD:
        return "low"
    elif density <= LOGIC_MED_THRESHOLD:
        return "medium"
    else:
        return "high"


def get_math_density_delta(curr_density: float, prev_density: Optional[float]) -> str:
    """Compute math density direction relative to previous node."""
    if prev_density is None:
        return "none"
    diff = curr_density - prev_density
    if diff > MATH_DELTA_THRESHOLD:
        return "up"
    elif diff < -MATH_DELTA_THRESHOLD:
        return "down"
    else:
        return "flat"


def node_ends_with_colon(flat_text: str) -> bool:
    """Return True if flat_text ends with an ASCII or full-width colon."""
    t = flat_text.strip()
    return t.endswith(":") or t.endswith("：")


def _get_attr_or_item(obj: Any, key: str, default: Any = None) -> Any:
    """Extract a field from either an object attribute or a dictionary."""
    if isinstance(obj, Mapping):
        return obj.get(key, default)
    return getattr(obj, key, default)


def extract_node_base_features(
    node: Any,
    feat: Any,
    offset: int,
) -> dict[str, float]:
    """Extract position-specific base features for a node at given window offset.

    Args:
        node: DumpNode or dict with 'kind', 'flat_text', 'char_len'.
        feat: NodeFeatures or dict with static signals.
        offset: Context window offset (-1, 0, or +1).

    Returns:
        Dictionary of feature_key -> 1.0 indicator weight.
    """
    p = offset_prefix(offset)
    feats: dict[str, float] = {}

    # 1. node_kind
    kind = _get_attr_or_item(node, "kind", "other")
    if isinstance(kind, str):
        feats[f"{p}node_kind={kind.lower().strip()}"] = 1.0

    # 2. ends_colon
    flat_text = _get_attr_or_item(node, "flat_text", "")
    if node_ends_with_colon(flat_text):
        feats[f"{p}ends_colon=true"] = 1.0

    # 3. lead information
    lead = _get_attr_or_item(feat, "lead")
    if lead is not None:
        feats[f"{p}has_lead=true"] = 1.0
        lead_kind = _get_attr_or_item(lead, "kind")
        if lead_kind:
            lk = str(lead_kind).lower().strip()
            feats[f"{p}lead={lk}"] = 1.0

        if _get_attr_or_item(lead, "labelled", False):
            feats[f"{p}labelled=true"] = 1.0

        if _get_attr_or_item(lead, "body_empty", False):
            feats[f"{p}body_empty=true"] = 1.0

        if _get_attr_or_item(lead, "reference", False):
            feats[f"{p}lead_reference=true"] = 1.0

        if _get_attr_or_item(lead, "number") is not None:
            feats[f"{p}lead_has_number=true"] = 1.0

        if _get_attr_or_item(lead, "name") is not None:
            feats[f"{p}lead_has_name=true"] = 1.0

        refers_to = _get_attr_or_item(lead, "refers_to")
        if refers_to is not None:
            ref_kind = _get_attr_or_item(refers_to, "kind")
            if ref_kind:
                feats[f"{p}lead_refers_to={str(ref_kind).lower().strip()}"] = 1.0
    else:
        feats[f"{p}has_lead=false"] = 1.0

    # 4. emphasized
    if _get_attr_or_item(feat, "emphasized", False):
        feats[f"{p}emphasized=true"] = 1.0

    # 5. qed
    if _get_attr_or_item(feat, "qed", False):
        feats[f"{p}qed=true"] = 1.0

    # 6. math_density (bucketed)
    math_density = float(_get_attr_or_item(feat, "math_density", 0.0) or 0.0)
    feats[f"{p}math_density={bucket_math_density(math_density)}"] = 1.0

    # 7. logic_density (bucketed)
    logic_density = float(_get_attr_or_item(feat, "logic_density", 0.0) or 0.0)
    feats[f"{p}logic_density={bucket_logic_density(logic_density)}"] = 1.0

    # 8. cues
    if _get_attr_or_item(feat, "proof_cue", False):
        feats[f"{p}proof_cue=true"] = 1.0

    if _get_attr_or_item(feat, "statement_cue", False):
        feats[f"{p}statement_cue=true"] = 1.0

    if _get_attr_or_item(feat, "definitional", False):
        feats[f"{p}definitional=true"] = 1.0

    if _get_attr_or_item(feat, "continuation", False):
        feats[f"{p}continuation=true"] = 1.0

    # 9. mentions
    mentions = _get_attr_or_item(feat, "mentions", []) or []
    for m in mentions:
        if m:
            feats[f"{p}mention={str(m).lower().strip()}"] = 1.0

    return feats


def extract_node_features(
    nodes: Sequence[Any],
    features: Sequence[Any],
    i: int,
) -> dict[str, float]:
    """Generate string feature keys for node i using the [i-1, i, i+1] context window.

    Args:
        nodes: Sequence of nodes (DumpNode or dicts).
        features: Sequence of feature entries (NodeFeatures or dicts).
        i: Current node index (0 <= i < len(nodes)).

    Returns:
        Dictionary of feature_key -> 1.0 indicator weight.
    """
    n_nodes = len(nodes)
    if not (0 <= i < n_nodes):
        raise IndexError(f"Node index {i} out of bounds for sequence length {n_nodes}")

    feats: dict[str, float] = {"bias": 1.0}

    # Extract base features for each valid offset in WINDOW_OFFSETS
    for offset in WINDOW_OFFSETS:
        target_idx = i + offset
        if 0 <= target_idx < n_nodes:
            offset_feats = extract_node_base_features(
                nodes[target_idx],
                features[target_idx],
                offset,
            )
            feats.update(offset_feats)

    # Relational & sequence boundary features
    if i == 0:
        feats["0:is_first_node=true"] = 1.0

    if i == n_nodes - 1:
        feats["0:is_last_node=true"] = 1.0

    if i > 0:
        curr_math = float(_get_attr_or_item(features[i], "math_density", 0.0) or 0.0)
        prev_math = float(_get_attr_or_item(features[i - 1], "math_density", 0.0) or 0.0)
        feats[f"0:math_density_delta={get_math_density_delta(curr_math, prev_math)}"] = 1.0
    else:
        curr_math = float(_get_attr_or_item(features[i], "math_density", 0.0) or 0.0)
        feats[f"0:math_density_delta={get_math_density_delta(curr_math, None)}"] = 1.0

    return feats


def extract_sequence_features(
    nodes: Sequence[Any],
    features: Sequence[Any],
) -> list[dict[str, float]]:
    """Extract features for an entire sequence of nodes.

    Args:
        nodes: Sequence of nodes.
        features: Sequence of feature entries aligned with nodes.

    Returns:
        List of feature dictionaries, one per node.
    """
    if len(nodes) != len(features):
        raise ValueError(
            f"Mismatched sequence lengths: nodes={len(nodes)}, features={len(features)}"
        )

    return [extract_node_features(nodes, features, i) for i in range(len(nodes))]


def extract_document_features(
    doc: Union[LabeledDocument, FeatureDump, Mapping[str, Any]],
) -> list[dict[str, float]]:
    """Extract windowed features for a document structure.

    Args:
        doc: LabeledDocument, FeatureDump, or dictionary with 'nodes' and 'features'.

    Returns:
        List of feature dictionaries, one per node.
    """
    nodes = _get_attr_or_item(doc, "nodes", [])
    features = _get_attr_or_item(doc, "features", [])
    return extract_sequence_features(nodes, features)


def extract_dataset(
    docs: Sequence[Union[LabeledDocument, Mapping[str, Any]]],
) -> tuple[list[list[dict[str, float]]], list[list[str]]]:
    """Extract feature sequences (X) and label sequences (y) for CRF training/eval.

    Args:
        docs: List of LabeledDocuments or dictionaries with 'nodes', 'features', 'bio_tags'.

    Returns:
        Tuple of (X, y) where X is list of sequence feature dicts and y is list of BIO label lists.
    """
    X: list[list[dict[str, float]]] = []
    y: list[list[str]] = []

    for doc in docs:
        nodes = _get_attr_or_item(doc, "nodes", [])
        features = _get_attr_or_item(doc, "features", [])
        bio_tags = _get_attr_or_item(doc, "bio_tags", [])

        if len(nodes) != len(features) or len(nodes) != len(bio_tags):
            doc_id = _get_attr_or_item(doc, "doc_id", "unknown")
            logger.warning(
                "Skipping inconsistent document %s: nodes=%d, features=%d, bio_tags=%d",
                doc_id,
                len(nodes),
                len(features),
                len(bio_tags),
            )
            continue

        feats_seq = extract_sequence_features(nodes, features)
        X.append(feats_seq)
        y.append(list(bio_tags))

    return X, y


def summarize_features(X: Sequence[Sequence[Mapping[str, float]]]) -> dict[str, Any]:
    """Compute summary statistics over an extracted feature corpus."""
    total_docs = len(X)
    total_nodes = sum(len(seq) for seq in X)
    unique_keys: set[str] = set()
    total_active_features = 0

    for seq in X:
        for node_dict in seq:
            total_active_features += len(node_dict)
            unique_keys.update(node_dict.keys())

    avg_features_per_node = (
        total_active_features / total_nodes if total_nodes > 0 else 0.0
    )

    return {
        "total_documents": total_docs,
        "total_nodes": total_nodes,
        "unique_feature_keys": len(unique_keys),
        "total_active_features": total_active_features,
        "avg_features_per_node": round(avg_features_per_node, 2),
    }


def main() -> None:
    """CLI to inspect and test feature extraction on a JSONL file."""
    parser = argparse.ArgumentParser(
        description="Extract and inspect CRF features for a Mathia JSONL dataset."
    )
    parser.add_argument("input_jsonl", type=Path, help="Path to input JSONL dataset")
    parser.add_argument(
        "--sample-doc",
        type=int,
        default=0,
        help="Index of document to print detailed features for (default: 0)",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Output summary statistics as JSON",
    )
    args = parser.parse_args()

    docs: list[dict[str, Any]] = []
    with open(args.input_jsonl, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                docs.append(json.loads(line))

    X, y = extract_dataset(docs)
    stats = summarize_features(X)

    if args.json:
        print(json.dumps(stats, indent=2))
    else:
        print(f"=== Mathia CRF Feature Summary: {args.input_jsonl} ===")
        print(f"Documents:               {stats['total_documents']}")
        print(f"Total Nodes:             {stats['total_nodes']}")
        print(f"Unique Feature Keys:     {stats['unique_feature_keys']}")
        print(f"Avg Features / Node:     {stats['avg_features_per_node']}")

        if 0 <= args.sample_doc < len(docs):
            doc = docs[args.sample_doc]
            doc_id = doc.get("doc_id", "sample")
            print(f"\n--- Sample Document: {doc_id} (Nodes: {len(doc.get('nodes', []))}) ---")
            seq_feats = X[args.sample_doc]
            seq_tags = y[args.sample_doc]
            for i, (nf, tag) in enumerate(zip(seq_feats, seq_tags)):
                node_text = doc["nodes"][i].get("flat_text", "")
                preview = (node_text[:50] + "...") if len(node_text) > 50 else node_text
                print(f"Node {i} [{tag}] ({len(nf)} features): {preview!r}")
                sample_keys = sorted(k for k in nf.keys() if k != "bias")[:10]
                print(f"    Sample keys: {sample_keys}")


if __name__ == "__main__":
    main()
