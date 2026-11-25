"""Feedback Exporter for Mathia ML (Task 3).

Converts saved ScopeOverrides and converged math-judge verdicts
(Replay logs, UI telemetry, judge logs) into the standard LabeledDocument JSONL schema.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, Optional, Union

from mathia_ml.dataset_gen import find_mathia_binary, run_dump_features
from mathia_ml.schema import DumpNode, FeatureDump, LabeledDocument, NodeFeatures
from mathia_ml.split import compute_dataset_stats, load_jsonl, split_documents, write_split_jsonl

# Unicode range for Persian / Arabic scripts
PERSIAN_PATTERN = re.compile(r"[\u0600-\u06FF\uFB50-\uFDFF\uFE70-\uFEFF]")


def detect_language(text: str) -> str:
    """Detect whether text is predominantly Persian ('fa') or English ('en')."""
    persian_chars = len(PERSIAN_PATTERN.findall(text))
    # If more than 5 Persian characters or > 5% of non-whitespace is Persian
    non_ws = sum(1 for c in text if not c.isspace())
    if persian_chars >= 5 or (non_ws > 0 and (persian_chars / non_ws) > 0.05):
        return "fa"
    return "en"


def parse_override_key(key: str | int, nodes_count: int) -> tuple[Optional[int], Optional[str]]:
    """Parse an OverrideKey from string or integer.

    Returns:
        (node_index, anchor_hash_str)
    """
    if isinstance(key, int):
        return (key if 0 <= key < nodes_count else None, None)

    s = str(key).strip()
    if s.isdigit():
        idx = int(s)
        return (idx if 0 <= idx < nodes_count else None, None)

    parts = s.split(":")
    if len(parts) == 1:
        if parts[0].isdigit():
            idx = int(parts[0])
            return (idx if 0 <= idx < nodes_count else None, None)
        return (None, parts[0])
    elif len(parts) == 2:
        return (None, parts[0])
    else:
        # hash:occurrence:fallback
        fb = int(parts[2]) if parts[2].isdigit() else None
        if fb is not None and 0 <= fb < nodes_count:
            return (fb, parts[0])
        return (None, parts[0])


def load_overrides_dict(data_or_path: Union[dict[str, Any], str, Path]) -> tuple[dict[str, Any], dict[str, Any]]:
    """Load ScopeOverrides dictionary and metadata from a file or dict.

    Supports:
    - Bare ScopeOverrides JSON: {"delta": ..., "suppressed": ..., "retype": ..., "end": ...}
    - Judge log / Replay JSON: {"protocol": ..., "model_version": ..., "overrides": {...}, "log": [...]}
    - Verdict list: {"log": [...]}

    Returns:
        (overrides_dict, metadata_dict)
    """
    if isinstance(data_or_path, (str, Path)):
        path = Path(data_or_path)
        with open(path, "r", encoding="utf-8") as f:
            data = json.load(f)
        source_name = path.name
    else:
        data = data_or_path
        source_name = "in_memory"

    metadata: dict[str, Any] = {"source": source_name}

    if not isinstance(data, dict):
        raise ValueError(f"Expected JSON object for overrides, got {type(data)}")

    # Check if this is a Replay / judge-log file
    if "overrides" in data and isinstance(data["overrides"], dict):
        metadata["protocol"] = data.get("protocol")
        metadata["model_version"] = data.get("model_version")
        if "log" in data:
            metadata["log_count"] = len(data["log"])
        return data["overrides"], metadata

    # Check if this is a bare log of verdicts
    if "log" in data and isinstance(data["log"], list) and "overrides" not in data:
        overrides = reconstruct_overrides_from_log(data["log"])
        metadata["reconstructed_from_log"] = True
        metadata["log_count"] = len(data["log"])
        return overrides, metadata

    # Otherwise assume bare ScopeOverrides
    overrides = {
        "delta": data.get("delta", {}),
        "suppressed": data.get("suppressed", []),
        "retype": data.get("retype", {}),
        "end": data.get("end", {}),
    }
    return overrides, metadata


def reconstruct_overrides_from_log(log_entries: list[dict[str, Any]]) -> dict[str, Any]:
    """Reconstruct ScopeOverrides from a sequence of AppliedVerdict entries."""
    delta: dict[str, int] = {}
    suppressed: list[str] = []
    retype: dict[str, str] = {}
    end_map: dict[str, int] = {}

    for entry in log_entries:
        if not entry.get("applied", True):
            continue

        span = entry.get("span", {})
        if isinstance(span, dict):
            start = span.get("start", 0)
            end = span.get("end", start + 1)
        elif isinstance(span, (list, tuple)) and len(span) == 2:
            start, end = span[0], span[1]
        else:
            start = 0
            end = 1

        opener = str(start)
        verdict = entry.get("verdict")

        if verdict == "accept":
            continue
        elif verdict == "incomplete":
            delta[opener] = delta.get(opener, 0) + 1
        elif verdict == "overreach":
            delta[opener] = delta.get(opener, 0) - 1
        elif verdict == "reject":
            if opener not in suppressed:
                suppressed.append(opener)
        elif isinstance(verdict, dict):
            if "retype" in verdict:
                retype[opener] = str(verdict["retype"]).lower()
            elif "end_at" in verdict:
                end_map[opener] = int(verdict["end_at"])
            elif "start_at" in verdict:
                new_start = str(verdict["start_at"])
                if opener not in suppressed:
                    suppressed.append(opener)
                kind = entry.get("kind", "theorem")
                retype[new_start] = str(kind).lower()
                end_map[new_start] = end
        elif isinstance(verdict, str):
            if verdict.startswith("retype:"):
                retype[opener] = verdict.split(":", 1)[1].lower()
            elif verdict.startswith("end_at:"):
                end_map[opener] = int(verdict.split(":", 1)[1])
            elif verdict.startswith("start_at:"):
                new_start = str(int(verdict.split(":", 1)[1]))
                if opener not in suppressed:
                    suppressed.append(opener)
                kind = entry.get("kind", "theorem")
                retype[new_start] = str(kind).lower()
                end_map[new_start] = end

    return {
        "delta": delta,
        "suppressed": suppressed,
        "retype": retype,
        "end": end_map,
    }


def export_feedback_from_markdown_and_overrides(
    markdown_content: str,
    overrides_data: Union[dict[str, Any], str, Path],
    doc_id: str = "feedback_doc",
    title: str = "",
    language: Optional[str] = None,
    mathia_bin: Optional[Path] = None,
) -> LabeledDocument:
    """Convert a single (markdown, overrides) pair into a verified LabeledDocument."""
    overrides_dict, metadata = load_overrides_dict(overrides_data)

    detected_lang = language or detect_language(markdown_content)

    if mathia_bin is None:
        mathia_bin = find_mathia_binary()

    if not mathia_bin.exists():
        raise FileNotFoundError(f"Mathia binary not found at {mathia_bin}")

    with tempfile.NamedTemporaryFile("w", suffix=".overrides.json", delete=False, encoding="utf-8") as f_ov:
        json.dump(overrides_dict, f_ov)
        tmp_ov = Path(f_ov.name)
    try:
        dump = run_dump_features(mathia_bin, markdown_content, overrides_path=tmp_ov)
    finally:
        if tmp_ov.exists():
            tmp_ov.unlink()

    if dump.bio_tags is None:
        raise RuntimeError(f"mathia dump-features with overrides failed to produce bio_tags for {doc_id}")

    bio_tags = dump.bio_tags

    if not title:
        # Extract title from first heading if present
        for node in dump.nodes:
            if node.kind == "heading":
                title = node.flat_text.lstrip("#").strip()
                break

    metadata["num_nodes"] = len(dump.nodes)
    # Feedback pairs are user corrections by definition. Keep the original
    # file-name/in-memory source for provenance.
    metadata.setdefault("override_source", metadata.get("source"))
    metadata["source"] = "user_correction"

    return LabeledDocument(
        doc_id=doc_id,
        language=detected_lang,
        title=title,
        nodes=dump.nodes,
        features=dump.features,
        bio_tags=bio_tags,
        metadata=metadata,
    )


def export_feedback_directory(
    dir_path: Path,
    mathia_bin: Optional[Path] = None,
) -> list[LabeledDocument]:
    """Scan directory for matching (.md, .overrides.json / .replay.json / .judge.json) pairs."""
    docs: list[LabeledDocument] = []
    dir_path = Path(dir_path)

    md_files = sorted(dir_path.glob("*.md"))
    for md_path in md_files:
        stem = md_path.stem
        # Look for overrides file
        candidates = [
            dir_path / f"{stem}.overrides.json",
            dir_path / f"{stem}.replay.json",
            dir_path / f"{stem}.judge.json",
            dir_path / f"{stem}.feedback.json",
            dir_path / f"{stem}.json",
        ]
        ov_path = next((c for c in candidates if c.exists()), None)
        if not ov_path:
            continue

        doc_id = f"feedback_{stem}"
        with open(md_path, "r", encoding="utf-8") as f:
            md_text = f.read()

        try:
            doc = export_feedback_from_markdown_and_overrides(
                markdown_content=md_text,
                overrides_data=ov_path,
                doc_id=doc_id,
                mathia_bin=mathia_bin,
            )
            docs.append(doc)
            print(f"Exported feedback: {md_path.name} + {ov_path.name} -> {len(doc.nodes)} nodes")
        except Exception as e:
            print(f"Error processing {md_path.name}: {e}", file=sys.stderr)

    return docs


def export_feedback_telemetry(
    telemetry_path: Path,
    mathia_bin: Optional[Path] = None,
) -> list[LabeledDocument]:
    """Process feedback telemetry JSONL file (as produced by UI/Tauri or judge logs)."""
    docs: list[LabeledDocument] = []
    telemetry_path = Path(telemetry_path)

    with open(telemetry_path, "r", encoding="utf-8") as f:
        for line_no, line in enumerate(f, 1):
            line = line.strip()
            if not line:
                continue
            try:
                entry = json.loads(line)
            except Exception as e:
                print(f"Warning: line {line_no} malformed JSON: {e}", file=sys.stderr)
                continue

            doc_id = entry.get("doc_id", f"telemetry_{line_no:05d}")

            # Check if entry already has full LabeledDocument schema
            if "nodes" in entry and "features" in entry and "bio_tags" in entry:
                try:
                    doc = LabeledDocument.model_validate(entry)
                    original_source = doc.metadata.get("source")
                    if original_source and original_source != "user_correction":
                        doc.metadata.setdefault("telemetry_source", original_source)
                    doc.metadata["source"] = "user_correction"
                    docs.append(doc)
                    continue
                except Exception:
                    pass

            # Otherwise extract markdown and overrides
            md_content = entry.get("markdown")
            if not md_content and "markdown_path" in entry:
                p = Path(entry["markdown_path"])
                if p.exists():
                    md_content = p.read_text(encoding="utf-8")

            if not md_content:
                print(f"Warning: line {line_no} missing markdown content, skipping.", file=sys.stderr)
                continue

            ov_data = entry.get("overrides") or entry.get("replay") or entry.get("log")
            if not ov_data and "overrides_path" in entry:
                p = Path(entry["overrides_path"])
                if p.exists():
                    ov_data = p

            if not ov_data:
                print(f"Warning: line {line_no} missing overrides/log data, skipping.", file=sys.stderr)
                continue

            try:
                doc = export_feedback_from_markdown_and_overrides(
                    markdown_content=md_content,
                    overrides_data=ov_data,
                    doc_id=doc_id,
                    title=entry.get("title", ""),
                    language=entry.get("language"),
                    mathia_bin=mathia_bin,
                )
                docs.append(doc)
            except Exception as e:
                print(f"Error processing telemetry line {line_no}: {e}", file=sys.stderr)

    return docs


def merge_and_resplit(
    existing_data_dir: Path,
    new_docs: list[LabeledDocument],
    seed: int = 42,
) -> dict[str, Any]:
    """Merge new feedback documents into an existing dataset directory and resplit 80/10/10 with zero leakage."""
    train_path = existing_data_dir / "train.jsonl"
    val_path = existing_data_dir / "val.jsonl"
    test_path = existing_data_dir / "test.jsonl"

    all_docs: dict[str, LabeledDocument] = {}
    for p in (train_path, val_path, test_path):
        if p.exists():
            for d in load_jsonl(p):
                all_docs[d.doc_id] = d

    # Add / update new feedback documents
    for d in new_docs:
        all_docs[d.doc_id] = d

    print(f"Total merged corpus: {len(all_docs)} unique documents.")
    docs_list = list(all_docs.values())

    train, val, test = split_documents(docs_list, seed=seed)
    stats = write_split_jsonl(existing_data_dir, train, val, test)
    return stats


def main() -> None:
    parser = argparse.ArgumentParser(description="Mathia Feedback Exporter (Task 3)")
    parser.add_argument("--markdown", "-m", type=Path, help="Single markdown source file")
    parser.add_argument("--overrides", "-ov", type=Path, help="Matching ScopeOverrides or Replay judge-log JSON")
    parser.add_argument("--dir", "-d", type=Path, help="Directory containing (.md, .overrides.json) pairs")
    parser.add_argument("--telemetry", "-t", type=Path, help="Telemetry JSONL file")
    parser.add_argument("--output", "-o", type=Path, help="Output path for exported JSONL")
    parser.add_argument("--split", action="store_true", help="Split exported documents into train/val/test 80/10/10")
    parser.add_argument("--output-dir", type=Path, default=Path("data"), help="Directory for split outputs")
    parser.add_argument("--merge-into", type=Path, help="Merge into existing data directory and resplit 80/10/10")
    parser.add_argument("--seed", type=int, default=42, help="Random seed for splitting")

    args = parser.parse_args()

    mathia_bin = find_mathia_binary()
    exported: list[LabeledDocument] = []

    if args.markdown:
        if not args.overrides:
            parser.error("--overrides is required when --markdown is specified.")
        print(f"Processing single pair: {args.markdown} + {args.overrides}")
        doc = export_feedback_from_markdown_and_overrides(
            markdown_content=args.markdown.read_text(encoding="utf-8"),
            overrides_data=args.overrides,
            doc_id=f"feedback_{args.markdown.stem}",
            mathia_bin=mathia_bin,
        )
        exported.append(doc)

    elif args.dir:
        print(f"Scanning directory: {args.dir}...")
        exported.extend(export_feedback_directory(args.dir, mathia_bin=mathia_bin))

    elif args.telemetry:
        print(f"Processing telemetry file: {args.telemetry}...")
        exported.extend(export_feedback_telemetry(args.telemetry, mathia_bin=mathia_bin))

    else:
        parser.error("Specify --markdown & --overrides, --dir, or --telemetry.")

    print(f"Total exported feedback documents: {len(exported)}")

    if not exported:
        print("No feedback documents to write.")
        return

    if args.merge_into:
        print(f"Merging {len(exported)} feedback documents into {args.merge_into}...")
        stats = merge_and_resplit(args.merge_into, exported, seed=args.seed)
        print("Merge and resplit complete!")
        print(json.dumps(stats, indent=2))
        return

    if args.split:
        print(f"Splitting {len(exported)} documents 80/10/10 into {args.output_dir}...")
        train, val, test = split_documents(exported, seed=args.seed)
        stats = write_split_jsonl(args.output_dir, train, val, test)
        print("Split complete!")
        print(json.dumps(stats, indent=2))
        return

    output_path = args.output or Path("data/feedback.jsonl")
    output_path.parent.mkdir(parents=True, exist_ok=True)
    with open(output_path, "w", encoding="utf-8") as f:
        for d in exported:
            f.write(d.model_dump_json() + "\n")
    print(f"Saved {len(exported)} documents to {output_path}")

    stats = compute_dataset_stats(exported)
    print(json.dumps(stats, indent=2))


if __name__ == "__main__":
    main()
