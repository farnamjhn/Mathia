"""Extract unlabeled documents from real Markdown or HTML math notes.

Runs ``mathia dump-features`` on every input file and writes:

- a full ``UnlabeledDocument`` JSONL (``nodes`` + ``features``) for the
  validation/merge step, and
- a compact JSONL for an LLM labeler that keeps only the node text and
  structure (no ``features``), which is far cheaper in tokens.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any, Optional

from mathia_ml.dataset_gen import find_mathia_binary, run_dump_features
from mathia_ml.schema import DumpNode, UnlabeledDocument

SUPPORTED_SUFFIXES = {".md", ".markdown", ".mdown", ".txt", ".html", ".htm"}
HTML_SUFFIXES = {".html", ".htm"}

PERSIAN_RANGES = (
    ("\u0600", "\u06ff"),
    ("\ufb50", "\ufdff"),
    ("\ufe70", "\ufeff"),
)


def is_persian(char: str) -> bool:
    return any(start <= char <= end for start, end in PERSIAN_RANGES)


def detect_language(text: str) -> str:
    """``"fa"`` when Persian characters are a meaningful share of the text."""
    persian = sum(1 for char in text if is_persian(char))
    non_whitespace = sum(1 for char in text if not char.isspace())
    if persian >= 5 or (non_whitespace and persian / non_whitespace > 0.05):
        return "fa"
    return "en"


def looks_like_html(content: str) -> bool:
    lowered = content.lstrip("\ufeff").lstrip().lower()
    return (
        lowered.startswith("<!doctype html")
        or lowered.startswith("<html")
        or lowered.startswith("<body")
        or lowered.startswith("<main id=\"mx-main\"")
        or lowered.startswith("<main id='mx-main'")
        or ("<html" in lowered and "</html>" in lowered)
        or ("<body" in lowered and "</body>" in lowered)
        or 'class="mx-block' in lowered
        or "class='mx-block" in lowered
    )


def source_format(path: Path, content: str) -> str:
    if path.suffix.lower() in HTML_SUFFIXES or looks_like_html(content):
        return "html"
    return "md"


def make_doc_id(path: Path, content: str) -> str:
    """Deterministic ``stem_<content hash>`` identifier."""
    stem = re.sub(r"[^\w-]+", "_", path.stem, flags=re.UNICODE).strip("_") or "doc"
    digest = hashlib.sha256(content.encode("utf-8")).hexdigest()[:8]
    return f"{stem}_{digest}"


def extract_title(nodes: list[DumpNode], fallback: str) -> str:
    for node in nodes:
        if node.kind == "heading" and node.flat_text.strip():
            return node.flat_text.strip()
    return fallback


def collect_input_files(input_path: Path, recursive: bool) -> list[Path]:
    if input_path.is_file():
        if input_path.suffix.lower() not in SUPPORTED_SUFFIXES:
            raise ValueError(
                f"{input_path}: unsupported extension (expected one of "
                f"{', '.join(sorted(SUPPORTED_SUFFIXES))})"
            )
        return [input_path]
    if not input_path.is_dir():
        raise FileNotFoundError(f"{input_path}: no such file or directory")

    iterator = input_path.rglob("*") if recursive else input_path.iterdir()
    files = [
        path
        for path in iterator
        if path.is_file() and path.suffix.lower() in SUPPORTED_SUFFIXES
    ]
    files.sort(key=lambda path: str(path))
    return files


def compact_node(node: DumpNode) -> dict[str, Any]:
    return {
        "index": node.index,
        "kind": node.kind,
        "flat_text": node.flat_text,
        "env_span": node.env_span.model_dump() if node.env_span is not None else None,
    }


def compact_record(doc: UnlabeledDocument) -> dict[str, Any]:
    return {
        "doc_id": doc.doc_id,
        "language": doc.language,
        "title": doc.title,
        "nodes": [compact_node(node) for node in doc.nodes],
    }


def prepare_document(
    path: Path,
    mathia_bin: Path,
    config_path: Optional[Path] = None,
) -> UnlabeledDocument:
    content = path.read_text(encoding="utf-8")
    fmt = source_format(path, content)
    dump = run_dump_features(
        mathia_bin,
        content,
        suffix=".html" if fmt == "html" else ".md",
        config_path=config_path,
    )
    return UnlabeledDocument(
        doc_id=make_doc_id(path, content),
        language=detect_language(content),
        title=extract_title(dump.nodes, path.stem),
        nodes=dump.nodes,
        features=dump.features,
        bio_tags=None,
        metadata={
            "source": "real_notes",
            "source_file": str(path),
            "source_format": fmt,
        },
    )


def default_llm_path(output: Path) -> Path:
    return output.with_name(f"{output.stem}.for_llm{output.suffix or '.jsonl'}")


def write_jsonl(path: Path, records: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        for record in records:
            handle.write(json.dumps(record, ensure_ascii=False) + "\n")


def prepare_unlabeled(
    input_path: Path,
    output: Path,
    llm_input: Optional[Path] = None,
    config_path: Optional[Path] = None,
    recursive: bool = False,
) -> list[UnlabeledDocument]:
    files = collect_input_files(input_path, recursive)
    if not files:
        raise ValueError(f"{input_path}: no Markdown or HTML files found")

    mathia_bin = find_mathia_binary()
    documents: list[UnlabeledDocument] = []
    for path in files:
        try:
            documents.append(prepare_document(path, mathia_bin, config_path))
        except Exception as error:  # keep going: one bad note must not stop a corpus
            print(f"Skipping {path}: {error}", file=sys.stderr)

    if not documents:
        raise ValueError("no documents could be extracted")

    write_jsonl(output, [doc.model_dump() for doc in documents])
    llm_path = llm_input or default_llm_path(output)
    write_jsonl(llm_path, [compact_record(doc) for doc in documents])

    total_nodes = sum(len(doc.nodes) for doc in documents)
    print(
        f"Wrote {len(documents)} unlabeled documents ({total_nodes} nodes) to {output}"
    )
    print(f"Wrote compact LLM input to {llm_path}")
    return documents


def main(argv: Optional[list[str]] = None) -> None:
    parser = argparse.ArgumentParser(
        description="Extract unlabeled documents from real Markdown/HTML math notes."
    )
    parser.add_argument("input", type=Path, help="Input file or directory")
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("data/unlabeled.jsonl"),
        help="Full UnlabeledDocument JSONL (default: data/unlabeled.jsonl)",
    )
    parser.add_argument(
        "--llm-input",
        type=Path,
        default=None,
        help="Compact JSONL for the LLM (default: sibling *.for_llm.jsonl)",
    )
    parser.add_argument(
        "--config", type=Path, default=None, help="Scoring configuration (.toml or .json)"
    )
    parser.add_argument(
        "--recursive", action="store_true", help="Recurse into subdirectories"
    )
    args = parser.parse_args(argv)

    prepare_unlabeled(
        input_path=args.input,
        output=args.output,
        llm_input=args.llm_input,
        config_path=args.config,
        recursive=args.recursive,
    )


if __name__ == "__main__":
    main()
