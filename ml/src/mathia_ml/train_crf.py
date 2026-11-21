"""CRF Training & Weights Exporter for Mathia ML (Slice 2, Task 2).

Trains a Linear-Chain Conditional Random Field (CRF) using `sklearn-crfsuite`,
tunes L1 (c1) and L2 (c2) regularization penalties against validation exact-span F1,
and exports compact, deterministic weights to `crates/math-core/src/crf_weights.json`.
"""

from __future__ import annotations

import argparse
import itertools
import json
import logging
from pathlib import Path
from typing import Any, Mapping, Optional, Sequence, Union

logger = logging.getLogger(__name__)

import numpy as np
from seqeval.metrics import classification_report, f1_score
import sklearn_crfsuite

from mathia_ml.features import extract_dataset
from mathia_ml.schema import LabeledDocument

DEFAULT_TRAIN_PATH = Path("ml/data/train.jsonl")
DEFAULT_VAL_PATH = Path("ml/data/val.jsonl")
DEFAULT_OUTPUT_WEIGHTS = Path("crates/math-core/src/crf_weights.json")

# Default grid for c1 (L1) and c2 (L2) regularization tuning
DEFAULT_C1_CANDIDATES: tuple[float, ...] = (0.05, 0.1, 0.2, 0.3)
DEFAULT_C2_CANDIDATES: tuple[float, ...] = (0.05, 0.1, 0.2, 0.3)


def load_jsonl_dataset(path: Path) -> list[dict[str, Any]]:
    """Load JSONL dataset as list of raw document dictionaries."""
    docs: list[dict[str, Any]] = []
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                docs.append(json.loads(line))
    return docs


def train_crf_model(
    X_train: Sequence[Sequence[Mapping[str, float]]],
    y_train: Sequence[Sequence[str]],
    c1: float = 0.2,
    c2: float = 0.2,
    max_iterations: int = 100,
) -> sklearn_crfsuite.CRF:
    """Train a Linear-Chain CRF on sequence features using L-BFGS.

    Args:
        X_train: Sequence feature dictionaries.
        y_train: Ground truth BIO label sequences.
        c1: L1 regularization penalty (promotes sparsity).
        c2: L2 regularization penalty.
        max_iterations: Maximum L-BFGS iterations.

    Returns:
        Fitted sklearn_crfsuite.CRF instance.
    """
    crf = sklearn_crfsuite.CRF(
        algorithm="lbfgs",
        c1=c1,
        c2=c2,
        max_iterations=max_iterations,
        all_possible_transitions=True,
    )
    crf.fit(X_train, y_train)
    return crf


def tune_hyperparameters(
    X_train: Sequence[Sequence[Mapping[str, float]]],
    y_train: Sequence[Sequence[str]],
    X_val: Sequence[Sequence[Mapping[str, float]]],
    y_val: Sequence[Sequence[str]],
    c1_candidates: Sequence[float] = DEFAULT_C1_CANDIDATES,
    c2_candidates: Sequence[float] = DEFAULT_C2_CANDIDATES,
    max_iterations: int = 80,
) -> tuple[tuple[float, float], float, list[dict[str, Any]]]:
    """Tune c1 and c2 hyperparameters against validation exact-span F1.

    Args:
        X_train: Training sequence features.
        y_train: Training label sequences.
        X_val: Validation sequence features.
        y_val: Validation label sequences.
        c1_candidates: Grid values for c1.
        c2_candidates: Grid values for c2.
        max_iterations: Max L-BFGS iterations per trial.

    Returns:
        Tuple of ((best_c1, best_c2), best_val_f1, all_tuning_results).
    """
    best_params = (0.2, 0.2)
    best_score = -1.0
    results: list[dict[str, Any]] = []

    for c1, c2 in itertools.product(c1_candidates, c2_candidates):
        model = sklearn_crfsuite.CRF(
            algorithm="lbfgs",
            c1=c1,
            c2=c2,
            max_iterations=max_iterations,
            all_possible_transitions=True,
        )
        model.fit(X_train, y_train)
        y_pred = model.predict(X_val)
        score = float(f1_score(y_val, y_pred))

        trial = {
            "c1": c1,
            "c2": c2,
            "val_f1": round(score, 4),
            "num_state_features": len(model.state_features_),
            "num_transition_features": len(model.transition_features_),
        }
        results.append(trial)
        logger.info(
            "Tuning trial: c1=%.3f, c2=%.3f -> val F1=%.4f (features: %d)",
            c1,
            c2,
            score,
            len(model.state_features_),
        )

        if score > best_score:
            best_score = score
            best_params = (c1, c2)

    return best_params, best_score, results


def format_crf_weights(
    crf: sklearn_crfsuite.CRF,
    precision: int = 5,
    min_weight_threshold: float = 1e-4,
) -> dict[str, Any]:
    """Convert fitted CRF weights into the canonical JSON schema for Rust inference.

    Target Schema:
        {
          "labels": ["O", "B-THEOREM", "I-THEOREM", ...],
          "transitions": [[0.12, -0.45, ...]],
          "state_features": {
            "0:lead=theorem": { "1": 2.41 },
            "-1:ends_colon=true": { "2": 0.88 }
          }
        }

    Args:
        crf: Fitted sklearn-crfsuite CRF instance.
        precision: Decimal rounding precision for weights.
        min_weight_threshold: Minimum absolute weight to retain.

    Returns:
        Dictionary adhering to the target Rust Viterbi weights schema.
    """
    # Deterministic label order with "O" as index 0
    labels = list(crf.classes_)
    if "O" in labels and labels[0] != "O":
        labels.remove("O")
        labels.insert(0, "O")

    K = len(labels)
    label_to_idx = {label: idx for idx, label in enumerate(labels)}

    # K x K transition matrix: transitions[from_idx][to_idx]
    transitions = [[0.0] * K for _ in range(K)]
    for (from_label, to_label), weight in crf.transition_features_.items():
        if from_label in label_to_idx and to_label in label_to_idx:
            from_idx = label_to_idx[from_label]
            to_idx = label_to_idx[to_label]
            transitions[from_idx][to_idx] = round(float(weight), precision)

    # State features mapping: feature_string -> { label_idx_string: weight }
    state_features: dict[str, dict[str, float]] = {}
    for (feat_name, label), weight in crf.state_features_.items():
        if label in label_to_idx:
            w = round(float(weight), precision)
            if abs(w) >= min_weight_threshold:
                label_idx_str = str(label_to_idx[label])
                if feat_name not in state_features:
                    state_features[feat_name] = {}
                state_features[feat_name][label_idx_str] = w

    return {
        "labels": labels,
        "transitions": transitions,
        "state_features": state_features,
    }


def export_crf_weights_json(
    weights: dict[str, Any],
    output_path: Path,
    compact: bool = False,
) -> Path:
    """Serialize weights dictionary to JSON file."""
    output_path = Path(output_path)
    output_path.parent.mkdir(parents=True, exist_ok=True)

    with open(output_path, "w", encoding="utf-8") as f:
        if compact:
            json.dump(weights, f, separators=(",", ":"))
        else:
            json.dump(weights, f, indent=2)

    return output_path


def viterbi_decode(
    sequence_features: Sequence[Mapping[str, float]],
    weights: Mapping[str, Any],
) -> list[str]:
    """Pure Python reference implementation of Viterbi decoder using exported weights.

    Validates that Rust Viterbi inference will produce identical results to Python CRFsuite.
    """
    labels: list[str] = weights["labels"]
    K = len(labels)
    if K == 0 or len(sequence_features) == 0:
        return []

    transitions = np.array(weights["transitions"], dtype=float)  # shape (K, K)
    state_features: Mapping[str, Mapping[str, float]] = weights["state_features"]

    N = len(sequence_features)
    emissions = np.zeros((N, K), dtype=float)

    for t, node_dict in enumerate(sequence_features):
        for feat_key in node_dict:
            if feat_key in state_features:
                for label_idx_str, w in state_features[feat_key].items():
                    emissions[t, int(label_idx_str)] += w

    dp = np.zeros((N, K), dtype=float)
    backpointers = np.zeros((N, K), dtype=int)

    dp[0] = emissions[0]
    for t in range(1, N):
        for j in range(K):
            scores = dp[t - 1] + transitions[:, j] + emissions[t, j]
            best_prev = int(np.argmax(scores))
            dp[t, j] = scores[best_prev]
            backpointers[t, j] = best_prev

    best_last = int(np.argmax(dp[N - 1]))
    path = [best_last]
    for t in range(N - 1, 0, -1):
        best_last = int(backpointers[t, best_last])
        path.append(best_last)

    path.reverse()
    return [labels[idx] for idx in path]


def verify_viterbi_parity(
    crf: sklearn_crfsuite.CRF,
    weights: Mapping[str, Any],
    X_val: Sequence[Sequence[Mapping[str, float]]],
) -> tuple[float, int, int]:
    """Compare sklearn_crfsuite predictions against Viterbi decoder predictions.

    Returns:
        Tuple of (parity_percentage, num_matches, total_tokens).
    """
    crf_preds = crf.predict(X_val)
    viterbi_preds = [viterbi_decode(seq, weights) for seq in X_val]

    total_tokens = sum(len(seq) for seq in X_val)
    matches = 0

    for py_seq, vit_seq in zip(crf_preds, viterbi_preds):
        for py_tag, vit_tag in zip(py_seq, vit_seq):
            if py_tag == vit_tag:
                matches += 1

    parity = (matches / total_tokens) if total_tokens > 0 else 1.0
    return parity, matches, total_tokens


def train_and_export(
    train_path: Path = DEFAULT_TRAIN_PATH,
    val_path: Path = DEFAULT_VAL_PATH,
    output_path: Path = DEFAULT_OUTPUT_WEIGHTS,
    c1: Optional[float] = None,
    c2: Optional[float] = None,
    tune: bool = True,
    max_iterations: int = 100,
    precision: int = 5,
    compact: bool = False,
) -> dict[str, Any]:
    """Train CRF, tune if requested, evaluate, and export weights to JSON."""
    logger.info("Loading training dataset from %s...", train_path)
    train_docs = load_jsonl_dataset(train_path)
    X_train, y_train = extract_dataset(train_docs)

    logger.info("Loading validation dataset from %s...", val_path)
    val_docs = load_jsonl_dataset(val_path)
    X_val, y_val = extract_dataset(val_docs)

    logger.info(
        "Extracted %d train sequences (%d nodes), %d val sequences (%d nodes).",
        len(X_train),
        sum(len(s) for s in X_train),
        len(X_val),
        sum(len(s) for s in X_val),
    )

    if tune and (c1 is None or c2 is None):
        logger.info("Tuning L1 (c1) and L2 (c2) regularization penalties on validation set...")
        best_params, best_score, tuning_trials = tune_hyperparameters(
            X_train,
            y_train,
            X_val,
            y_val,
            max_iterations=max_iterations,
        )
        selected_c1, selected_c2 = best_params
        logger.info(
            "Selected optimal hyperparameters: c1=%.3f, c2=%.3f (Val exact-span F1: %.4f)",
            selected_c1,
            selected_c2,
            best_score,
        )
    else:
        selected_c1 = c1 if c1 is not None else 0.2
        selected_c2 = c2 if c2 is not None else 0.2
        tuning_trials = []
        logger.info("Using fixed regularization: c1=%.3f, c2=%.3f", selected_c1, selected_c2)

    logger.info("Fitting final CRF model...")
    crf = train_crf_model(
        X_train,
        y_train,
        c1=selected_c1,
        c2=selected_c2,
        max_iterations=max_iterations,
    )

    # Validation evaluation
    y_pred = crf.predict(X_val)
    val_f1 = float(f1_score(y_val, y_pred))
    report = classification_report(y_val, y_pred)
    logger.info("Validation Exact-Span F1: %.4f\n%s", val_f1, report)

    # Export weights
    weights = format_crf_weights(crf, precision=precision)
    export_crf_weights_json(weights, output_path, compact=compact)
    file_size_kb = output_path.stat().st_size / 1024

    # Parity verification
    parity, matches, total_tokens = verify_viterbi_parity(crf, weights, X_val)
    logger.info(
        "Viterbi parity on validation set: %.2f%% (%d/%d tokens match)",
        parity * 100.0,
        matches,
        total_tokens,
    )

    summary = {
        "c1": selected_c1,
        "c2": selected_c2,
        "val_f1": round(val_f1, 4),
        "classes": weights["labels"],
        "num_classes": len(weights["labels"]),
        "num_state_features": len(weights["state_features"]),
        "output_path": str(output_path),
        "file_size_kb": round(file_size_kb, 2),
        "viterbi_parity": round(parity * 100.0, 2),
        "tuning_trials": tuning_trials,
    }
    return summary


def main() -> None:
    """CLI entrypoint for training CRF and exporting weights."""
    parser = argparse.ArgumentParser(
        description="Train Mathia Linear-Chain CRF and export weights to JSON."
    )
    parser.add_argument(
        "--train-file",
        type=Path,
        default=DEFAULT_TRAIN_PATH,
        help="Path to training JSONL dataset (default: ml/data/train.jsonl)",
    )
    parser.add_argument(
        "--val-file",
        type=Path,
        default=DEFAULT_VAL_PATH,
        help="Path to validation JSONL dataset (default: ml/data/val.jsonl)",
    )
    parser.add_argument(
        "--output-weights",
        type=Path,
        default=DEFAULT_OUTPUT_WEIGHTS,
        help="Output path for crf_weights.json (default: crates/math-core/src/crf_weights.json)",
    )
    parser.add_argument(
        "--no-tune",
        action="store_true",
        help="Skip c1/c2 hyperparameter tuning",
    )
    parser.add_argument(
        "--c1",
        type=float,
        default=None,
        help="L1 regularization penalty (overrides tuning)",
    )
    parser.add_argument(
        "--c2",
        type=float,
        default=None,
        help="L2 regularization penalty (overrides tuning)",
    )
    parser.add_argument(
        "--max-iterations",
        type=int,
        default=100,
        help="Maximum L-BFGS iterations (default: 100)",
    )
    parser.add_argument(
        "--precision",
        type=int,
        default=5,
        help="Weight rounding precision (default: 5)",
    )
    parser.add_argument(
        "--compact",
        action="store_true",
        help="Export compact JSON without indentations",
    )
    args = parser.parse_args()

    logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")

    summary = train_and_export(
        train_path=args.train_file,
        val_path=args.val_file,
        output_path=args.output_weights,
        c1=args.c1,
        c2=args.c2,
        tune=not args.no_tune,
        max_iterations=args.max_iterations,
        precision=args.precision,
        compact=args.compact,
    )

    print("\n=== Mathia CRF Training & Export Complete ===")
    print(f"Optimal Regularization: c1={summary['c1']}, c2={summary['c2']}")
    print(f"Validation Exact-Span F1: {summary['val_f1']}")
    print(f"Number of Classes:       {summary['num_classes']} ({summary['classes']})")
    print(f"Number of State Features:{summary['num_state_features']}")
    print(f"Weights File:            {summary['output_path']} ({summary['file_size_kb']} KB)")
    print(f"Viterbi Parity:          {summary['viterbi_parity']}%")


if __name__ == "__main__":
    main()
