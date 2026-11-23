import json
from pathlib import Path

import pytest

from mathia_ml.import_labeled import (
    LabelValidationError,
    import_labeled,
    merge_labels,
    parse_labels,
)
from mathia_ml.prepare_unlabeled import prepare_unlabeled
from mathia_ml.schema import DumpNode, LabeledDocument, NodeFeatures, UnlabeledDocument


MD_SAMPLE = """# Rolle's Theorem

**Theorem 1.** Let $f$ be continuous on $[a,b]$. Then

$$f'(c) = 0$$

**Proof.** Immediate from compactness.
"""

HTML_SAMPLE = """<html><body>
<h1>Groups</h1>
<p><strong>Theorem 1.</strong> Let $G$ be a finite group.</p>
<p>$$|G| = \\sum_{g \\in G} 1$$</p>
<p><strong>Proof.</strong> Count the elements.</p>
</body></html>
"""

FA_SAMPLE = """# قضیه رول

**قضیه ۳.۲.** فرض کنید $f$ روی $[a,b]$ پیوسته باشد. آنگاه

$$f'(c) = 0$$
"""


def read_jsonl(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]


def test_prepare_unlabeled_markdown_and_html(tmp_path: Path):
    notes = tmp_path / "notes"
    notes.mkdir()
    (notes / "rolle.md").write_text(MD_SAMPLE, encoding="utf-8")
    (notes / "groups.html").write_text(HTML_SAMPLE, encoding="utf-8")

    output = tmp_path / "data" / "unlabeled.jsonl"
    llm_input = tmp_path / "data" / "unlabeled_for_llm.jsonl"
    docs = prepare_unlabeled(notes, output, llm_input=llm_input)

    assert len(docs) == 2
    records = read_jsonl(output)
    assert len(records) == 2
    for record in records:
        assert record["bio_tags"] is None
        assert len(record["nodes"]) == len(record["features"])
        assert record["metadata"]["source"] == "real_notes"

    by_format = {record["metadata"]["source_format"]: record for record in records}
    assert set(by_format) == {"md", "html"}
    assert by_format["md"]["doc_id"].startswith("rolle_")
    assert by_format["html"]["doc_id"].startswith("groups_")
    assert by_format["md"]["title"] == "Rolle's Theorem"
    assert by_format["html"]["title"] == "Groups"
    assert any(node["kind"] == "display_math" for node in by_format["html"]["nodes"])

    compact = read_jsonl(llm_input)
    assert len(compact) == 2
    for record in compact:
        assert set(record) == {"doc_id", "language", "title", "nodes"}
        for node in record["nodes"]:
            assert set(node) == {"index", "kind", "flat_text", "env_span"}


def test_prepare_unlabeled_default_llm_path_and_language(tmp_path: Path):
    source = tmp_path / "rolle_fa.md"
    source.write_text(FA_SAMPLE, encoding="utf-8")
    output = tmp_path / "out" / "unlabeled.jsonl"

    docs = prepare_unlabeled(source, output)

    assert docs[0].language == "fa"
    assert output.exists()
    assert (output.parent / "unlabeled.for_llm.jsonl").exists()


def make_unlabeled(doc_id: str = "doc_1") -> UnlabeledDocument:
    nodes = [
        DumpNode(index=0, kind="paragraph", flat_text="Theorem 1. Let x.", char_len=16),
        DumpNode(index=1, kind="paragraph", flat_text="Then y.", char_len=7),
        DumpNode(index=2, kind="display_math", flat_text="x = y", char_len=5),
        DumpNode(index=3, kind="paragraph", flat_text="Done.", char_len=5),
    ]
    return UnlabeledDocument(
        doc_id=doc_id,
        language="en",
        title="Doc",
        nodes=nodes,
        features=[NodeFeatures() for _ in nodes],
        metadata={"source": "real_notes", "source_format": "md"},
    )


def test_import_labeled_happy_path_with_fenced_jsonl(tmp_path: Path):
    unlabeled = make_unlabeled()
    unlabeled_path = tmp_path / "unlabeled.jsonl"
    unlabeled_path.write_text(unlabeled.model_dump_json() + "\n", encoding="utf-8")

    labels_path = tmp_path / "llm_labels.jsonl"
    labels_path.write_text(
        "```jsonl\n"
        + json.dumps(
            {"doc_id": "doc_1", "bio_tags": ["B-THEOREM", "I-THEOREM", "I-THEOREM", "O"]}
        )
        + "\n```\n",
        encoding="utf-8",
    )
    output = tmp_path / "real_labeled.jsonl"

    docs = import_labeled(unlabeled_path, labels_path, output)

    assert len(docs) == 1
    merged = read_jsonl(output)[0]
    assert merged["bio_tags"] == ["B-THEOREM", "I-THEOREM", "I-THEOREM", "O"]
    assert len(merged["nodes"]) == len(merged["features"]) == 4
    assert merged["metadata"]["source"] == "real_notes"
    LabeledDocument.model_validate(merged)


def test_parse_labels_accepts_json_array_and_full_documents():
    records = parse_labels(
        json.dumps(
            [
                {"doc_id": "a", "bio_tags": ["O"]},
                {"doc_id": "b", "bio_tags": ["B-PROOF"]},
            ]
        )
    )
    assert [record["doc_id"] for record in records] == ["a", "b"]


@pytest.mark.parametrize(
    "bio_tags, expected",
    [
        (["B-THEOREM", "I-THEOREM"], "length mismatch"),
        (["B-THEOREM", "I-LEMMA", "O", "O"], "illegal transition"),
        (["B-THEOREM", "I-UNKNOWN", "O", "O"], "unknown tag"),
        (["B-THEOREM", "I-THEOREM", "B-THEOREM", "O"], "cannot open a block"),
    ],
)
def test_import_labeled_rejects_invalid_tags(tmp_path: Path, bio_tags, expected):
    unlabeled = make_unlabeled()
    unlabeled_path = tmp_path / "unlabeled.jsonl"
    unlabeled_path.write_text(unlabeled.model_dump_json() + "\n", encoding="utf-8")
    labels_path = tmp_path / "llm_labels.jsonl"
    labels_path.write_text(
        json.dumps({"doc_id": "doc_1", "bio_tags": bio_tags}) + "\n", encoding="utf-8"
    )

    with pytest.raises(LabelValidationError) as error:
        import_labeled(unlabeled_path, labels_path, tmp_path / "out.jsonl")

    message = "\n".join(error.value.errors)
    assert "doc_1" in message
    assert expected in message


def test_merge_labels_skip_invalid_drops_only_bad_documents():
    good = make_unlabeled("good")
    bad = make_unlabeled("bad")
    records = [
        {"doc_id": "good", "bio_tags": ["B-THEOREM", "I-THEOREM", "I-THEOREM", "O"]},
        {"doc_id": "bad", "bio_tags": ["O", "I-THEOREM", "O", "O"]},
    ]

    labeled, warnings = merge_labels([good, bad], records, skip_invalid=True)

    assert [doc.doc_id for doc in labeled] == ["good"]
    assert warnings == []
