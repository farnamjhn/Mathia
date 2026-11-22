"""Evaluation Harness for Mathia Block Detection (Slice 2, Task 4).

Benchmarks HeuristicTagger vs. CrfTagger on sequence labeling using seqeval
exact-span F1 metrics, broken down by:
- Overall exact-span F1, Precision, and Recall
- Language breakdown (English vs. Persian)
- BlockType breakdown (Theorem, Lemma, Proof, Definition, Example, Remark)
- Challenging structural slices (multi-paragraph statements, QED-less proofs)
"""

from __future__ import annotations

import argparse
import json
import logging
from pathlib import Path
from typing import Any, Mapping, Optional, Sequence, Union

from seqeval.metrics import classification_report, f1_score, precision_score, recall_score

from mathia_ml.features import extract_sequence_features
from mathia_ml.schema import LabeledDocument
from mathia_ml.train_crf import viterbi_decode

logger = logging.getLogger(__name__)

DEFAULT_TEST_PATH = Path("ml/data/test.jsonl")
DEFAULT_WEIGHTS_PATH = Path("crates/math-core/src/crf_weights.json")

DEFAULT_WEIGHTS = {
    "lead": 0.45,
    "label": 0.20,
    "emphasis": 0.15,
    "mention": 0.10,
    "qed": 0.30,
    "logic": 0.15,
    "math": 0.10,
    "definitional": 0.25,
    "cue": 0.10,
    "after_statement": 0.25,
}
DEFAULT_THRESHOLD = 0.50
STATEMENT_MAX_NODES = 6
PROOF_MAX_NODES = 12

STATEMENT_TYPES = {
    "theorem",
    "lemma",
    "proposition",
    "corollary",
    "claim",
}
SEMANTIC_TYPES = [
    "theorem",
    "lemma",
    "proposition",
    "corollary",
    "definition",
    "proof",
    "example",
    "remark",
    "exercise",
    "solution",
    "claim",
    "notation",
]


def _get(obj: Any, key: str, default: Any = None) -> Any:
    """Safely get an attribute or dictionary item."""
    if isinstance(obj, Mapping):
        return obj.get(key, default)
    return getattr(obj, key, default)


def linear_score(
    feat: Any,
    kind: str,
    w: Mapping[str, float] = DEFAULT_WEIGHTS,
) -> float:
    """Compute heuristic linear opener score for candidate kind matching math-core."""
    lead = _get(feat, "lead")
    lead_kind = _get(lead, "kind") if lead else None
    lead_ref = _get(lead, "reference", False) if lead else False
    labelled = _get(lead, "labelled", False) if lead else False
    emphasized = bool(_get(feat, "emphasized", False))
    mentions = _get(feat, "mentions", []) or []

    if lead and str(lead_kind).lower().strip() == kind.lower().strip() and not lead_ref:
        score = (
            w["lead"]
            + w["label"] * (1.0 if labelled else 0.0)
            + w["emphasis"] * (1.0 if emphasized else 0.0)
        )
    else:
        score = w["mention"] * (
            1.0 if kind.lower().strip() in [str(m).lower().strip() for m in mentions] else 0.0
        )

    math_density = float(_get(feat, "math_density", 0.0) or 0.0)
    logic_density = float(_get(feat, "logic_density", 0.0) or 0.0)
    qed = bool(_get(feat, "qed", False))
    proof_cue = bool(_get(feat, "proof_cue", False))
    statement_cue = bool(_get(feat, "statement_cue", False))
    definitional = bool(_get(feat, "definitional", False))

    kl = kind.lower().strip()
    if kl == "proof":
        score += (
            w["qed"] * (1.0 if qed else 0.0)
            + w["logic"] * logic_density
            + w["cue"] * (1.0 if proof_cue else 0.0)
        )
    elif kl in ("definition", "notation"):
        score += w["definitional"] * (1.0 if definitional else 0.0) + w["math"] * math_density
    elif kl in STATEMENT_TYPES:
        score += (
            w["math"] * math_density
            + 0.5 * w["logic"] * logic_density
            + w["cue"] * (1.0 if statement_cue else 0.0)
        )

    return max(0.0, min(1.0, score))


def score_node(
    node: Any,
    feat: Any,
    threshold: float = DEFAULT_THRESHOLD,
    w: Mapping[str, float] = DEFAULT_WEIGHTS,
) -> tuple[bool, str, float]:
    """Score a node across all candidate types and return (opens, best_kind, best_score)."""
    best_kind = "theorem"
    best_score = -1.0
    for kind in SEMANTIC_TYPES:
        s = linear_score(feat, kind, w=w)
        if s > best_score:
            best_score = s
            best_kind = kind

    kind_str = str(_get(node, "kind", "")).lower().strip()
    can_open = kind_str in ("paragraph", "list_item", "heading", "quote")
    opens = can_open and (best_score >= threshold)
    return opens, best_kind, best_score


def statement_end(
    nodes: Sequence[Any],
    features: Sequence[Any],
    start: int,
    limit: int,
    max_nodes: int = STATEMENT_MAX_NODES,
) -> int:
    """Find statement end boundary matching math-core Stage 4 statement_end."""
    cap = min(start + max(1, max_nodes), limit)
    start_lead = _get(features[start], "lead")
    needs_body = bool(_get(start_lead, "body_empty", False)) if start_lead else False

    end = start + 1
    while end < cap:
        k = str(_get(nodes[end], "kind", "")).lower().strip()
        if k in ("display_math", "list", "list_item", "quote", "table"):
            take = True
        elif k == "paragraph":
            cont = bool(_get(features[end], "continuation", False))
            prev_flat = str(_get(nodes[end - 1], "flat_text", "")).rstrip()
            ends_punct = any(prev_flat.endswith(c) for c in (":", "،", ","))
            take = needs_body or cont or ends_punct
        else:
            take = False

        if not take:
            return end
        if k == "paragraph":
            needs_body = False
        end += 1
    return end


def argument_end(
    features: Sequence[Any],
    start: int,
    limit: int,
    max_nodes: int = PROOF_MAX_NODES,
) -> int:
    """Find argument end boundary matching math-core Stage 4 argument_end."""
    cap = min(start + max(1, max_nodes), limit)
    for j in range(start, cap):
        if bool(_get(features[j], "qed", False)):
            return j + 1
    return cap


def implicit_proof_score(
    nodes: Sequence[Any],
    features: Sequence[Any],
    prev_is_statement: bool,
    i: int,
    limit: int,
    w: Mapping[str, float] = DEFAULT_WEIGHTS,
    threshold: float = DEFAULT_THRESHOLD,
    max_nodes: int = PROOF_MAX_NODES,
) -> Optional[float]:
    """Score implicit keywordless proof following a statement."""
    if not prev_is_statement:
        return None
    k = str(_get(nodes[i], "kind", "")).lower().strip()
    if k not in ("paragraph", "list_item"):
        return None

    cap = min(i + max(1, max_nodes), limit)
    has_qed = any(bool(_get(features[j], "qed", False)) for j in range(i, cap))
    logic_density = float(_get(features[i], "logic_density", 0.0) or 0.0)
    proof_cue = bool(_get(features[i], "proof_cue", False))

    score = (
        w["after_statement"]
        + (w["qed"] if has_qed else 0.0)
        + w["logic"] * logic_density
        + (w["cue"] if proof_cue else 0.0)
    )
    if score >= threshold:
        return max(0.0, min(1.0, score))
    return None


def tag_heuristic(
    nodes: Sequence[Any],
    features: Sequence[Any],
    w: Mapping[str, float] = DEFAULT_WEIGHTS,
    threshold: float = DEFAULT_THRESHOLD,
) -> list[str]:
    """Generate BIO tags for a sequence using HeuristicTagger (Stages 3 & 4)."""
    n = len(nodes)
    if n == 0:
        return []

    node_scores = [score_node(nodes[i], features[i], threshold, w) for i in range(n)]
    tags = ["O"] * n
    covered = [False] * n
    env_first = [False] * n

    for i in range(n):
        env = _get(nodes[i], "env_span")
        if env:
            env_kind = str(_get(env, "kind", "theorem")).lower().strip()
            env_len = int(_get(env, "len", 1) or 1)
            end = min(n, i + max(1, env_len))
            env_first[i] = True
            for j in range(i + 1, end):
                covered[j] = True
            tags[i] = f"B-{env_kind.upper()}"
            for j in range(i + 1, end):
                tags[j] = f"I-{env_kind.upper()}"

    def is_heading(idx: int) -> bool:
        k = str(_get(nodes[idx], "kind", "")).lower().strip()
        return k == "heading" and not covered[idx]

    def base_opens(idx: int) -> bool:
        return not covered[idx] and (env_first[idx] or node_scores[idx][0])

    def boundary(idx: int) -> bool:
        return base_opens(idx) or is_heading(idx)

    prev_statement = False
    i = 0
    while i < n:
        if env_first[i]:
            env = _get(nodes[i], "env_span")
            env_kind = str(_get(env, "kind", "theorem")).lower().strip()
            prev_statement = env_kind in STATEMENT_TYPES
            env_len = int(_get(env, "len", 1) or 1)
            i = min(n, i + max(1, env_len))
            continue

        limit = n
        for j in range(i + 1, n):
            if boundary(j):
                limit = j
                break

        bulleted = str(_get(nodes[i], "kind", "")).lower().strip() == "list_item"
        scope_limit = limit
        for j in range(i + 1, limit):
            jk = str(_get(nodes[j], "kind", "")).lower().strip()
            if jk == "rule" or (bulleted and jk == "list_item"):
                scope_limit = j
                break

        if base_opens(i):
            _, kind, _ = node_scores[i]
            if kind == "proof":
                natural = argument_end(features, i, scope_limit)
            elif kind == "solution":
                has_qed = any(
                    bool(_get(features[j], "qed", False))
                    for j in range(i, min(scope_limit, i + PROOF_MAX_NODES))
                )
                if has_qed:
                    natural = argument_end(features, i, scope_limit)
                else:
                    natural = statement_end(nodes, features, i, scope_limit)
            else:
                natural = statement_end(nodes, features, i, scope_limit)

            tags[i] = f"B-{kind.upper()}"
            for j in range(i + 1, natural):
                tags[j] = f"I-{kind.upper()}"
            prev_statement = kind in STATEMENT_TYPES
            i = natural
        elif is_heading(i):
            prev_statement = False
            i += 1
        else:
            imp_score = implicit_proof_score(
                nodes, features, prev_statement, i, scope_limit, w, threshold
            )
            if imp_score is not None:
                natural = argument_end(features, i, scope_limit)
                tags[i] = "B-PROOF"
                for j in range(i + 1, natural):
                    tags[j] = "I-PROOF"
                prev_statement = False
                i = natural
            else:
                prev_statement = False
                i = limit

    return tags


def compute_metrics(
    y_true: Sequence[Sequence[str]],
    y_pred: Sequence[Sequence[str]],
) -> dict[str, Any]:
    """Compute seqeval exact-span classification metrics."""
    f1 = float(f1_score(y_true, y_pred))
    prec = float(precision_score(y_true, y_pred))
    rec = float(recall_score(y_true, y_pred))

    # Per-class metrics from classification_report
    report_dict = classification_report(y_true, y_pred, output_dict=True, zero_division=0)

    per_class = {}
    for cls_name, vals in report_dict.items():
        if cls_name not in ("micro avg", "macro avg", "weighted avg"):
            per_class[cls_name] = {
                "precision": round(float(vals.get("precision", 0.0)), 4),
                "recall": round(float(vals.get("recall", 0.0)), 4),
                "f1_score": round(float(vals.get("f1-score", 0.0)), 4),
                "support": int(vals.get("support", 0)),
            }

    return {
        "f1_score": round(f1, 4),
        "precision": round(prec, 4),
        "recall": round(rec, 4),
        "per_class": per_class,
    }


def find_weights_file(weights_path: Optional[Path] = None) -> Path:
    """Locate crf_weights.json from current working directory or repo root."""
    if weights_path and Path(weights_path).exists():
        return Path(weights_path).resolve()

    candidates = [
        DEFAULT_WEIGHTS_PATH,
        Path("../crates/math-core/src/crf_weights.json"),
        Path("../../crates/math-core/src/crf_weights.json"),
    ]
    for c in candidates:
        if c.exists():
            return c.resolve()

    raise FileNotFoundError(
        f"Could not locate CRF weights file at {DEFAULT_WEIGHTS_PATH}. Run train_crf.py first."
    )


def evaluate_dataset(
    test_path: Path = DEFAULT_TEST_PATH,
    weights_path: Optional[Path] = None,
) -> dict[str, Any]:
    """Run full benchmark comparing HeuristicTagger vs. CrfTagger on a dataset."""
    test_path = Path(test_path)
    if not test_path.exists():
        # Fallback for cwd inside ml/
        if (Path("..") / test_path).exists():
            test_path = Path("..") / test_path
        elif Path("data/test.jsonl").exists():
            test_path = Path("data/test.jsonl")

    w_path = find_weights_file(weights_path)
    with open(w_path, "r", encoding="utf-8") as f:
        crf_weights = json.load(f)

    docs: list[dict[str, Any]] = []
    with open(test_path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                docs.append(json.loads(line))

    if not docs:
        raise ValueError(f"No documents found in {test_path}")

    # Generate predictions for both taggers
    y_true: list[list[str]] = []
    y_heur: list[list[str]] = []
    y_crf: list[list[str]] = []

    # Subset accumulators
    en_true, en_heur, en_crf = [], [], []
    fa_true, fa_heur, fa_crf = [], [], []

    multi_para_true, multi_para_heur, multi_para_crf = [], [], []
    qed_less_true, qed_less_heur, qed_less_crf = [], [], []

    for d in docs:
        nodes = d.get("nodes", [])
        features = d.get("features", [])
        bio_tags = d.get("bio_tags", [])
        lang = d.get("language", "en")

        if len(nodes) != len(features) or len(nodes) != len(bio_tags):
            continue

        # Heuristic tagging
        pred_h = tag_heuristic(nodes, features)

        # CRF tagging
        seq_feats = extract_sequence_features(nodes, features)
        pred_c = viterbi_decode(seq_feats, crf_weights)

        y_true.append(bio_tags)
        y_heur.append(pred_h)
        y_crf.append(pred_c)

        if lang == "en":
            en_true.append(bio_tags)
            en_heur.append(pred_h)
            en_crf.append(pred_c)
        else:
            fa_true.append(bio_tags)
            fa_heur.append(pred_h)
            fa_crf.append(pred_c)

        # Multi-paragraph statements check (> 1 node in a statement)
        has_multi_para = False
        i = 0
        while i < len(bio_tags):
            tag = bio_tags[i]
            if tag.startswith("B-") and tag[2:].lower() in STATEMENT_TYPES:
                # Count span length
                span_len = 1
                while i + span_len < len(bio_tags) and bio_tags[i + span_len] == f"I-{tag[2:]}":
                    span_len += 1
                if span_len > 1:
                    has_multi_para = True
                i += span_len
            else:
                i += 1

        if has_multi_para:
            multi_para_true.append(bio_tags)
            multi_para_heur.append(pred_h)
            multi_para_crf.append(pred_c)

        # QED-less proofs check (proof with no QED marker in features)
        has_qedless_proof = False
        i = 0
        while i < len(bio_tags):
            tag = bio_tags[i]
            if tag == "B-PROOF":
                span_len = 1
                while i + span_len < len(bio_tags) and bio_tags[i + span_len] == "I-PROOF":
                    span_len += 1
                # Check if any node in span has qed marker
                any_qed = any(bool(_get(features[j], "qed", False)) for j in range(i, i + span_len))
                if not any_qed:
                    has_qedless_proof = True
                i += span_len
            else:
                i += 1

        if has_qedless_proof:
            qed_less_true.append(bio_tags)
            qed_less_heur.append(pred_h)
            qed_less_crf.append(pred_c)

    # Compute overall metrics
    overall_heur = compute_metrics(y_true, y_heur)
    overall_crf = compute_metrics(y_true, y_crf)

    # Compute language subsets
    en_heur_m = compute_metrics(en_true, en_heur) if en_true else None
    en_crf_m = compute_metrics(en_true, en_crf) if en_true else None

    fa_heur_m = compute_metrics(fa_true, fa_heur) if fa_true else None
    fa_crf_m = compute_metrics(fa_true, fa_crf) if fa_true else None

    # Compute challenging slice metrics
    multi_para_heur_m = compute_metrics(multi_para_true, multi_para_heur) if multi_para_true else None
    multi_para_crf_m = compute_metrics(multi_para_true, multi_para_crf) if multi_para_true else None

    qed_less_heur_m = compute_metrics(qed_less_true, qed_less_heur) if qed_less_true else None
    qed_less_crf_m = compute_metrics(qed_less_true, qed_less_crf) if qed_less_true else None

    return {
        "dataset_path": str(test_path),
        "total_documents": len(y_true),
        "total_tokens": sum(len(s) for s in y_true),
        "overall": {
            "heuristic": overall_heur,
            "crf": overall_crf,
            "delta_f1": round(overall_crf["f1_score"] - overall_heur["f1_score"], 4),
        },
        "by_language": {
            "english": {
                "count": len(en_true),
                "heuristic": en_heur_m,
                "crf": en_crf_m,
                "delta_f1": round(en_crf_m["f1_score"] - en_heur_m["f1_score"], 4) if (en_crf_m and en_heur_m) else 0.0,
            },
            "persian": {
                "count": len(fa_true),
                "heuristic": fa_heur_m,
                "crf": fa_crf_m,
                "delta_f1": round(fa_crf_m["f1_score"] - fa_heur_m["f1_score"], 4) if (fa_crf_m and fa_heur_m) else 0.0,
            },
        },
        "challenges": {
            "multi_paragraph_statements": {
                "count": len(multi_para_true),
                "heuristic": multi_para_heur_m,
                "crf": multi_para_crf_m,
                "delta_f1": round(multi_para_crf_m["f1_score"] - multi_para_heur_m["f1_score"], 4) if (multi_para_crf_m and multi_para_heur_m) else 0.0,
            },
            "qed_less_proofs": {
                "count": len(qed_less_true),
                "heuristic": qed_less_heur_m,
                "crf": qed_less_crf_m,
                "delta_f1": round(qed_less_crf_m["f1_score"] - qed_less_heur_m["f1_score"], 4) if (qed_less_crf_m and qed_less_heur_m) else 0.0,
            },
        },
    }


def format_report_markdown(eval_results: dict[str, Any]) -> str:
    """Format full benchmark comparison report as clean GitHub Markdown."""
    lines: list[str] = []
    lines.append("# Mathia Block Detection Evaluation Report (Slice 2, Task 4)")
    lines.append("")
    lines.append(f"- **Test Dataset:** `{eval_results['dataset_path']}`")
    lines.append(f"- **Documents Evaluated:** {eval_results['total_documents']}")
    lines.append(f"- **Total Nodes / Tokens:** {eval_results['total_tokens']}")
    lines.append("")

    # Overall Summary Table
    ov = eval_results["overall"]
    h = ov["heuristic"]
    c = ov["crf"]
    delta_f1 = ov["delta_f1"]
    sign = "+" if delta_f1 >= 0 else ""

    lines.append("## 1. Overall Exact-Span Performance")
    lines.append("")
    lines.append("| Metric | HeuristicTagger | CrfTagger | Delta (CRF - Heuristic) |")
    lines.append("| :--- | :---: | :---: | :---: |")
    lines.append(f"| **Exact-Span F1** | **{h['f1_score']:.4f}** | **{c['f1_score']:.4f}** | **{sign}{delta_f1:.4f}** |")
    lines.append(f"| Precision | {h['precision']:.4f} | {c['precision']:.4f} | {c['precision'] - h['precision']:+.4f} |")
    lines.append(f"| Recall | {h['recall']:.4f} | {c['recall']:.4f} | {c['recall'] - h['recall']:+.4f} |")
    lines.append("")

    # Language Breakdown
    lines.append("## 2. Breakdown by Language")
    lines.append("")
    lines.append("| Language | Docs | Heuristic F1 | CrfTagger F1 | Delta F1 |")
    lines.append("| :--- | :---: | :---: | :---: | :---: |")
    by_lang = eval_results["by_language"]
    for lang_key, label in [("english", "English (en)"), ("persian", "Persian (fa)")]:
        item = by_lang[lang_key]
        if item["heuristic"] and item["crf"]:
            hf1 = item["heuristic"]["f1_score"]
            cf1 = item["crf"]["f1_score"]
            df1 = item["delta_f1"]
            s = "+" if df1 >= 0 else ""
            lines.append(f"| {label} | {item['count']} | {hf1:.4f} | {cf1:.4f} | {s}{df1:.4f} |")
    lines.append("")

    # Per-BlockType Breakdown
    lines.append("## 3. Breakdown by Semantic BlockType")
    lines.append("")
    lines.append("| BlockType | Support | Heuristic F1 | CrfTagger F1 | Delta F1 |")
    lines.append("| :--- | :---: | :---: | :---: | :---: |")

    all_classes = sorted(
        set(h["per_class"].keys()) | set(c["per_class"].keys())
    )
    for cls_name in all_classes:
        h_cls = h["per_class"].get(cls_name, {})
        c_cls = c["per_class"].get(cls_name, {})
        supp = c_cls.get("support", h_cls.get("support", 0))
        hf1 = h_cls.get("f1_score", 0.0)
        cf1 = c_cls.get("f1_score", 0.0)
        df1 = cf1 - hf1
        s = "+" if df1 >= 0 else ""
        lines.append(f"| `{cls_name}` | {supp} | {hf1:.4f} | {cf1:.4f} | {s}{df1:.4f} |")
    lines.append("")

    # Structural Challenges Breakdown
    lines.append("## 4. Key Structural Challenge Slices")
    lines.append("")
    lines.append("| Challenge Slice | Docs | Heuristic F1 | CrfTagger F1 | Delta F1 |")
    lines.append("| :--- | :---: | :---: | :---: | :---: |")
    ch = eval_results["challenges"]
    mp = ch["multi_paragraph_statements"]
    if mp["heuristic"] and mp["crf"]:
        hf1 = mp["heuristic"]["f1_score"]
        cf1 = mp["crf"]["f1_score"]
        df1 = mp["delta_f1"]
        s = "+" if df1 >= 0 else ""
        lines.append(f"| **Multi-Paragraph Statements** | {mp['count']} | {hf1:.4f} | {cf1:.4f} | **{s}{df1:.4f}** |")

    qd = ch["qed_less_proofs"]
    if qd["heuristic"] and qd["crf"]:
        hf1 = qd["heuristic"]["f1_score"]
        cf1 = qd["crf"]["f1_score"]
        df1 = qd["delta_f1"]
        s = "+" if df1 >= 0 else ""
        lines.append(f"| **QED-less Proofs** | {qd['count']} | {hf1:.4f} | {cf1:.4f} | **{s}{df1:.4f}** |")
    lines.append("")

    return "\n".join(lines)


def main() -> None:
    """CLI entrypoint to run benchmark evaluation harness."""
    parser = argparse.ArgumentParser(
        description="Benchmark HeuristicTagger vs. CrfTagger on Mathia datasets."
    )
    parser.add_argument(
        "--test-file",
        type=Path,
        default=DEFAULT_TEST_PATH,
        help="Path to test JSONL dataset (default: ml/data/test.jsonl)",
    )
    parser.add_argument(
        "--weights-file",
        type=Path,
        default=None,
        help="Path to crf_weights.json (default: crates/math-core/src/crf_weights.json)",
    )
    parser.add_argument(
        "--output-markdown",
        type=Path,
        default=None,
        help="Optional path to write markdown evaluation report",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Output raw evaluation results as JSON",
    )
    args = parser.parse_args()

    results = evaluate_dataset(
        test_path=args.test_file,
        weights_path=args.weights_file,
    )

    if args.json:
        print(json.dumps(results, indent=2))
    else:
        report = format_report_markdown(results)
        print(report)

    if args.output_markdown:
        out_path = Path(args.output_markdown)
        out_path.parent.mkdir(parents=True, exist_ok=True)
        report = format_report_markdown(results)
        out_path.write_text(report, encoding="utf-8")
        print(f"\nWrote markdown report to {out_path}")


if __name__ == "__main__":
    main()
