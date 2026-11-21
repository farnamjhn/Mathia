import tempfile
import unittest
from pathlib import Path

from mathia_ml.schema import DumpNode, LabeledDocument, NodeFeatures
from mathia_ml.split import (
    compute_dataset_stats,
    load_jsonl,
    split_documents,
    verify_split_directory,
    write_split_jsonl,
)


def make_dummy_doc(doc_id: str, lang: str, num_nodes: int = 3) -> LabeledDocument:
    nodes = [
        DumpNode(
            index=i,
            kind="paragraph",
            flat_text=f"Node {i} text for {doc_id}",
            char_len=20,
        )
        for i in range(num_nodes)
    ]
    features = [NodeFeatures() for _ in range(num_nodes)]
    bio_tags = ["B-THEOREM"] + ["I-THEOREM"] * (num_nodes - 1)
    return LabeledDocument(
        doc_id=doc_id,
        language=lang,
        title=f"Doc {doc_id}",
        nodes=nodes,
        features=features,
        bio_tags=bio_tags,
    )


class TestSplit(unittest.TestCase):
    def test_split_documents_proportions_and_zero_leakage(self):
        docs = []
        for i in range(50):
            docs.append(make_dummy_doc(f"en_{i:03d}", "en"))
        for i in range(50):
            docs.append(make_dummy_doc(f"fa_{i:03d}", "fa"))

        train, val, test = split_documents(docs, train_ratio=0.80, val_ratio=0.10, test_ratio=0.10, seed=42)

        # Proportions: 80 / 10 / 10
        self.assertEqual(len(train), 80)
        self.assertEqual(len(val), 10)
        self.assertEqual(len(test), 10)

        # Stratified by language: each should have half English, half Persian
        train_en = [d for d in train if d.language == "en"]
        train_fa = [d for d in train if d.language == "fa"]
        self.assertEqual(len(train_en), 40)
        self.assertEqual(len(train_fa), 40)

        val_en = [d for d in val if d.language == "en"]
        val_fa = [d for d in val if d.language == "fa"]
        self.assertEqual(len(val_en), 5)
        self.assertEqual(len(val_fa), 5)

        test_en = [d for d in test if d.language == "en"]
        test_fa = [d for d in test if d.language == "fa"]
        self.assertEqual(len(test_en), 5)
        self.assertEqual(len(test_fa), 5)

        # ZERO document leakage
        train_ids = {d.doc_id for d in train}
        val_ids = {d.doc_id for d in val}
        test_ids = {d.doc_id for d in test}

        self.assertEqual(len(train_ids & val_ids), 0)
        self.assertEqual(len(train_ids & test_ids), 0)
        self.assertEqual(len(val_ids & test_ids), 0)

    def test_deterministic_seed(self):
        docs = [make_dummy_doc(f"doc_{i}", "en" if i % 2 == 0 else "fa") for i in range(30)]
        train1, val1, test1 = split_documents(docs, seed=123)
        train2, val2, test2 = split_documents(docs, seed=123)

        self.assertEqual([d.doc_id for d in train1], [d.doc_id for d in train2])
        self.assertEqual([d.doc_id for d in val1], [d.doc_id for d in val2])
        self.assertEqual([d.doc_id for d in test1], [d.doc_id for d in test2])

    def test_invalid_ratios_raise_error(self):
        docs = [make_dummy_doc("d1", "en")]
        with self.assertRaises(ValueError):
            split_documents(docs, train_ratio=0.7, val_ratio=0.1, test_ratio=0.1)

    def test_write_and_verify_split_directory(self):
        docs = [make_dummy_doc(f"d_{i}", "en" if i % 2 == 0 else "fa") for i in range(20)]
        train, val, test = split_documents(docs, seed=42)

        with tempfile.TemporaryDirectory() as tmp_dir:
            out_dir = Path(tmp_dir)
            stats = write_split_jsonl(out_dir, train, val, test)

            self.assertTrue((out_dir / "train.jsonl").exists())
            self.assertTrue((out_dir / "val.jsonl").exists())
            self.assertTrue((out_dir / "test.jsonl").exists())
            self.assertTrue((out_dir / "split_stats.json").exists())

            loaded_train = load_jsonl(out_dir / "train.jsonl")
            self.assertEqual(len(loaded_train), len(train))

            verify_result = verify_split_directory(out_dir)
            self.assertEqual(verify_result["status"], "PASS")
            self.assertEqual(verify_result["document_leakage"], 0)


if __name__ == "__main__":
    unittest.main()
