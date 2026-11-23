"""Validate LLM-produced BIO tags and merge them onto unlabeled documents.

The labeler returns only ``{"doc_id", "bio_tags"}`` records (or full
``LabeledDocument`` records). This module parses that output forgivingly,
enforces every structural invariant, and writes ``LabeledDocument`` JSONL that
``split.py`` and ``train_crf.py`` can consume directly.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections import Counter
from pathlib import Path
from typing import Any, Optional

from mathia_ml.dataset_gen import BLOCK_TYPES
from mathia_ml.schema import LabeledDocument, UnlabeledDocument

TAG_VOCAB = {"O"} | {
    f"{prefix}-{block_type.upper()}"
    for prefix in ("B", "I")
    for block_type in BLOCK_TYPES
}

# Node kinds that can never open a semantic block.
NON_OPENABLE_KINDS = {"display_math", "table", "code", "rule", "list", "other"}

FENCE_RE = re.compile(r"```[a-zA-Z0-9_-]*[ \t]*\n(.*?)```", re.DOTALL)


class LabelValidationError(ValueError):
    """Raised when LLM labels violate the BIO contract."""

    def __init__(self, errors: list[str]):
        self.errors = errors
        super().__init__("\n".join(errors))


def _strip_code_fences(text: str) -> str:
    text = text.lstrip("\ufeff")
    blocks = FENCE_RE.findall(text)
    if blocks:
        return "\n".join(block.strip() for block in blocks)
    return text


def parse_labels(text: str) -> list[dict[str, Any]]:
    """Parse JSONL, a JSON array, or fenced JSON/JSONL into label records."""
    text = _strip_code_fences(text).strip()
    if not text:
        return []

    try:
        data = json.loads(text)
    except json.JSONDecodeError:
        records: list[dict[str, Any]] = []
        for line_number, line in enumerate(text.splitlines(), start=1):
            line = line.strip().rstrip(",")
            if not line or line in ("[", "]"):
                continue
            try:
                record = json.loads(line)
            except json.JSONDecodeError as error:
                raise LabelValidationError(
                    [f"labels line {line_number}: invalid JSON ({error.msg})"]
                ) from error
            if not isinstance(record, dict):
                raise LabelValidationError(
                    [f"labels line {line_number}: expected a JSON object"]
                )
            records.append(record)
        return records

    if isinstance(data, list):
        records = data
    elif isinstance(data, dict):
        records = [data]
    else:
        raise LabelValidationError(["labels: expected a JSON object or array"])
    if not all(isinstance(record, dict) for record in records):
        raise LabelValidationError(["labels: every record must be a JSON object"])
    return records


def load_unlabeled(path: Path) -> list[UnlabeledDocument]:
    documents: list[UnlabeledDocument] = []
    with path.open(encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, start=1):
            line = line.strip()
            if not line:
                continue
            try:
                documents.append(UnlabeledDocument.model_validate_json(line))
            except Exception as error:
                raise LabelValidationError(
                    [f"{path}:{line_number}: invalid UnlabeledDocument ({error})"]
                ) from error
    return documents


def validate_document(
    doc: UnlabeledDocument, bio_tags: Any
) -> list[str]:
    """Return every invariant violation for one document's tags."""
    errors: list[str] = []
    doc_id = doc.doc_id
    if not isinstance(bio_tags, list):
        return [f"{doc_id}: bio_tags must be a list, got {type(bio_tags).__name__}"]
    if len(bio_tags) != len(doc.nodes):
        errors.append(
            f"{doc_id}: length mismatch: {len(bio_tags)} tags for {len(doc.nodes)} nodes"
        )

    previous: Optional[str] = None
    for index, tag in enumerate(bio_tags):
        node_index = doc.nodes[index].index if index < len(doc.nodes) else index
        if not isinstance(tag, str) or tag not in TAG_VOCAB:
            errors.append(f"{doc_id}: node.index={node_index}: unknown tag {tag!r}")
            previous = tag if isinstance(tag, str) else None
            continue

        if tag.startswith("I-"):
            block_type = tag[2:]
            allowed = {f"B-{block_type}", f"I-{block_type}"}
            if previous not in allowed:
                errors.append(
                    f"{doc_id}: node.index={node_index}: illegal transition "
                    f"{previous!r} -> {tag!r}"
                )

        if tag.startswith("B-") and index < len(doc.nodes):
            kind = doc.nodes[index].kind
            if kind in NON_OPENABLE_KINDS:
                errors.append(
                    f"{doc_id}: node.index={node_index}: {tag} cannot open a block "
                    f"(kind={kind!r})"
                )
        previous = tag
    return errors


def merge_labels(
    unlabeled: list[UnlabeledDocument],
    label_records: list[dict[str, Any]],
    skip_invalid: bool = False,
) -> tuple[list[LabeledDocument], list[str]]:
    """Merge BIO tags onto unlabeled documents, validating every invariant.

    Returns the labeled documents and a list of warning messages. Raises
    :class:`LabelValidationError` when validation fails and ``skip_invalid`` is
    false.
    """
    by_id = {doc.doc_id: doc for doc in unlabeled}
    seen: set[str] = set()
    errors: list[str] = []
    labeled: list[LabeledDocument] = []

    for record in label_records:
        doc_id = record.get("doc_id")
        if not isinstance(doc_id, str) or not doc_id:
            errors.append("labels: record without a doc_id")
            continue
        if doc_id in seen:
            errors.append(f"{doc_id}: duplicate label record")
            continue
        seen.add(doc_id)

        doc = by_id.get(doc_id)
        if doc is None:
            errors.append(f"{doc_id}: doc_id not found in --unlabeled")
            continue

        bio_tags = record.get("bio_tags")
        doc_errors = validate_document(doc, bio_tags)
        if doc_errors:
            errors.extend(doc_errors)
            continue

        labeled.append(
            LabeledDocument(
                doc_id=doc.doc_id,
                language=doc.language,
                title=doc.title,
                nodes=doc.nodes,
                features=doc.features,
                bio_tags=list(bio_tags),
                metadata=doc.metadata,
            )
        )

    if errors:
        if not skip_invalid:
            raise LabelValidationError(errors)
        for message in errors:
            print(f"Skipping invalid label: {message}", file=sys.stderr)

    warnings = [
        f"{doc.doc_id}: unlabeled document has no labels"
        for doc in unlabeled
        if doc.doc_id not in seen
    ]
    return labeled, warnings


def span_counts(documents: list[LabeledDocument]) -> Counter[str]:
    counts: Counter[str] = Counter()
    for doc in documents:
        for tag in doc.bio_tags:
            if tag.startswith("B-"):
                counts[tag[2:].lower()] += 1
    return counts


def write_labeled_jsonl(path: Path, documents: list[LabeledDocument]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        for doc in documents:
            handle.write(json.dumps(doc.model_dump(), ensure_ascii=False) + "\n")


def import_labeled(
    unlabeled_path: Path,
    labels_path: Path,
    output: Path,
    skip_invalid: bool = False,
) -> list[LabeledDocument]:
    unlabeled = load_unlabeled(unlabeled_path)
    label_records = parse_labels(labels_path.read_text(encoding="utf-8"))
    labeled, warnings = merge_labels(unlabeled, label_records, skip_invalid=skip_invalid)
    for warning in warnings:
        print(f"warning: {warning}", file=sys.stderr)

    write_labeled_jsonl(output, labeled)

    counts = span_counts(labeled)
    total_nodes = sum(len(doc.nodes) for doc in labeled)
    print(f"Imported {len(labeled)} documents ({total_nodes} nodes) to {output}")
    for block_type in BLOCK_TYPES:
        if counts.get(block_type):
            print(f"  {block_type}: {counts[block_type]}")
    return labeled


def main(argv: Optional[list[str]] = None) -> None:
    parser = argparse.ArgumentParser(
        description="Validate LLM BIO labels and merge them onto unlabeled documents."
    )
    parser.add_argument("--unlabeled", type=Path, required=True)
    parser.add_argument("--labels", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--skip-invalid",
        action="store_true",
        help="Drop invalid documents instead of aborting",
    )
    args = parser.parse_args(argv)

    try:
        import_labeled(
            unlabeled_path=args.unlabeled,
            labels_path=args.labels,
            output=args.output,
            skip_invalid=args.skip_invalid,
        )
    except LabelValidationError as error:
        for message in error.errors:
            print(f"error: {message}", file=sys.stderr)
        raise SystemExit(1) from error


if __name__ == "__main__":
    main()
