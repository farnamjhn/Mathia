"""Document-Level Dataset Splitter for Mathia ML (Task 4).

Splits labeled sequences strictly by document (80/10/10 by default),
stratified by language (English and Persian), guaranteeing ZERO document leakage.
"""

from __future__ import annotations

import argparse
import json
import random
import sys
from collections import Counter
from pathlib import Path
from typing import Any, Optional

from mathia_ml.schema import LabeledDocument


def split_documents(
    docs: list[LabeledDocument],
    train_ratio: float = 0.80,
    val_ratio: float = 0.10,
    test_ratio: float = 0.10,
    seed: int = 42,
) -> tuple[list[LabeledDocument], list[LabeledDocument], list[LabeledDocument]]:
    """Split documents strictly at document level, stratified by language.

    Args:
        docs: List of LabeledDocument objects.
        train_ratio: Proportion of documents for training (default: 0.80).
        val_ratio: Proportion of documents for validation (default: 0.10).
        test_ratio: Proportion of documents for testing (default: 0.10).
        seed: Random seed for deterministic reproducibility.

    Returns:
        tuple of (train_docs, val_docs, test_docs).

    Raises:
        ValueError: If ratios do not sum to 1.0 or if document leakage is detected.
    """
    total_ratio = train_ratio + val_ratio + test_ratio
    if abs(total_ratio - 1.0) > 1e-5:
        raise ValueError(
            f"Ratios must sum to 1.0, got train={train_ratio}, val={val_ratio}, test={test_ratio} (sum={total_ratio})"
        )

    # Group documents by language
    by_lang: dict[str, list[LabeledDocument]] = {}
    for d in docs:
        lang = d.language.lower().strip() if d.language else "en"
        by_lang.setdefault(lang, []).append(d)

    train_all: list[LabeledDocument] = []
    val_all: list[LabeledDocument] = []
    test_all: list[LabeledDocument] = []

    rng = random.Random(seed)

    for lang, lang_docs in sorted(by_lang.items()):
        items = list(lang_docs)
        rng.shuffle(items)
        n = len(items)

        if n == 0:
            continue
        elif n == 1:
            train_all.extend(items)
            continue
        elif n == 2:
            train_all.append(items[0])
            val_all.append(items[1])
            continue

        n_val = max(1, int(round(val_ratio * n))) if n >= 10 else 1
        n_test = max(1, int(round(test_ratio * n))) if n >= 10 else 1
        if n_val + n_test >= n:
            n_val = 1
            n_test = 1 if n >= 3 else 0
        n_train = n - n_val - n_test

        train_slice = items[:n_train]
        val_slice = items[n_train : n_train + n_val]
        test_slice = items[n_train + n_val :]

        train_all.extend(train_slice)
        val_all.extend(val_slice)
        test_all.extend(test_slice)

    # Shuffle each partition
    rng.shuffle(train_all)
    rng.shuffle(val_all)
    rng.shuffle(test_all)

    # Verification: STRICT zero document leakage check
    train_ids = {d.doc_id for d in train_all}
    val_ids = {d.doc_id for d in val_all}
    test_ids = {d.doc_id for d in test_all}

    leak_train_val = train_ids & val_ids
    leak_train_test = train_ids & test_ids
    leak_val_test = val_ids & test_ids

    if leak_train_val:
        raise ValueError(f"CRITICAL: Document leakage between train and val: {leak_train_val}")
    if leak_train_test:
        raise ValueError(f"CRITICAL: Document leakage between train and test: {leak_train_test}")
    if leak_val_test:
        raise ValueError(f"CRITICAL: Document leakage between val and test: {leak_val_test}")

    return train_all, val_all, test_all


def compute_dataset_stats(docs: list[LabeledDocument]) -> dict[str, Any]:
    """Compute detailed summary statistics for a document split."""
    total_docs = len(docs)
    total_nodes = sum(len(d.nodes) for d in docs)
    langs = Counter(d.language for d in docs)

    tag_counts: Counter[str] = Counter()
    block_type_counts: Counter[str] = Counter()
    for d in docs:
        for tag in d.bio_tags:
            tag_counts[tag] += 1
            if tag.startswith("B-"):
                block_type_counts[tag[2:]] += 1

    return {
        "num_documents": total_docs,
        "total_nodes": total_nodes,
        "languages": dict(langs),
        "tag_counts": dict(tag_counts),
        "block_types": dict(block_type_counts),
    }


def write_split_jsonl(
    output_dir: Path,
    train: list[LabeledDocument],
    val: list[LabeledDocument],
    test: list[LabeledDocument],
) -> dict[str, Any]:
    """Save train, val, and test splits into output_dir with stats."""
    output_dir.mkdir(parents=True, exist_ok=True)

    def write_jsonl(path: Path, items: list[LabeledDocument]) -> None:
        with open(path, "w", encoding="utf-8") as f:
            for item in items:
                f.write(item.model_dump_json() + "\n")

    train_path = output_dir / "train.jsonl"
    val_path = output_dir / "val.jsonl"
    test_path = output_dir / "test.jsonl"

    write_jsonl(train_path, train)
    write_jsonl(val_path, val)
    write_jsonl(test_path, test)

    stats = {
        "train": compute_dataset_stats(train),
        "val": compute_dataset_stats(val),
        "test": compute_dataset_stats(test),
        "total_documents": len(train) + len(val) + len(test),
        "total_nodes": sum(len(d.nodes) for d in train + val + test),
    }

    stats_path = output_dir / "split_stats.json"
    with open(stats_path, "w", encoding="utf-8") as f:
        json.dump(stats, f, indent=2, ensure_ascii=False)

    return stats


def load_jsonl(path: Path) -> list[LabeledDocument]:
    """Load a list of LabeledDocument from a JSONL file."""
    docs: list[LabeledDocument] = []
    with open(path, "r", encoding="utf-8") as f:
        for line_no, line in enumerate(f, 1):
            line = line.strip()
            if not line:
                continue
            try:
                data = json.loads(line)
                docs.append(LabeledDocument.model_validate(data))
            except Exception as e:
                print(f"Warning: Failed to parse line {line_no} in {path}: {e}", file=sys.stderr)
    return docs


def verify_split_directory(data_dir: Path) -> dict[str, Any]:
    """Verify zero document leakage and integrity of an existing data directory."""
    train_path = data_dir / "train.jsonl"
    val_path = data_dir / "val.jsonl"
    test_path = data_dir / "test.jsonl"

    if not train_path.exists() or not val_path.exists() or not test_path.exists():
        raise FileNotFoundError(f"Missing one or more split files in {data_dir}")

    train = load_jsonl(train_path)
    val = load_jsonl(val_path)
    test = load_jsonl(test_path)

    train_ids = {d.doc_id for d in train}
    val_ids = {d.doc_id for d in val}
    test_ids = {d.doc_id for d in test}

    leak_train_val = train_ids & val_ids
    leak_train_test = train_ids & test_ids
    leak_val_test = val_ids & test_ids

    if leak_train_val or leak_train_test or leak_val_test:
        raise ValueError(
            f"LEAKAGE DETECTED! train&val: {len(leak_train_val)}, "
            f"train&test: {len(leak_train_test)}, val&test: {len(leak_val_test)}"
        )

    return {
        "status": "PASS",
        "document_leakage": 0,
        "train": compute_dataset_stats(train),
        "val": compute_dataset_stats(val),
        "test": compute_dataset_stats(test),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description="Mathia Document-Level Splitter")
    parser.add_argument("--input", "-i", type=Path, help="Input JSONL file containing LabeledDocument entries")
    parser.add_argument("--output-dir", "-o", type=Path, default=Path("data"), help="Directory to save train/val/test.jsonl")
    parser.add_argument("--train-ratio", type=float, default=0.80, help="Train ratio (default: 0.80)")
    parser.add_argument("--val-ratio", type=float, default=0.10, help="Val ratio (default: 0.10)")
    parser.add_argument("--test-ratio", type=float, default=0.10, help="Test ratio (default: 0.10)")
    parser.add_argument("--seed", type=int, default=42, help="Random seed")
    parser.add_argument("--verify-dir", type=Path, default=None, help="Verify existing train/val/test.jsonl in directory")

    args = parser.parse_args()

    if args.verify_dir:
        print(f"Verifying splits in {args.verify_dir}...")
        results = verify_split_directory(args.verify_dir)
        print("Verification SUCCESS: Zero document leakage!")
        print(json.dumps(results, indent=2))
        return

    if not args.input:
        parser.error("--input is required unless --verify-dir is specified.")

    print(f"Loading documents from {args.input}...")
    docs = load_jsonl(args.input)
    print(f"Loaded {len(docs)} documents.")

    train, val, test = split_documents(
        docs,
        train_ratio=args.train_ratio,
        val_ratio=args.val_ratio,
        test_ratio=args.test_ratio,
        seed=args.seed,
    )

    print(f"Split results: train={len(train)}, val={len(val)}, test={len(test)}")
    stats = write_split_jsonl(args.output_dir, train, val, test)
    print(f"Saved splits to {args.output_dir}/ (train.jsonl, val.jsonl, test.jsonl)")
    print(f"Train: {stats['train']['num_documents']} docs ({stats['train']['languages']})")
    print(f"Val:   {stats['val']['num_documents']} docs ({stats['val']['languages']})")
    print(f"Test:  {stats['test']['num_documents']} docs ({stats['test']['languages']})")


if __name__ == "__main__":
    main()
