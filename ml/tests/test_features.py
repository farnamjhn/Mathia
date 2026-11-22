"""Unit tests for Mathia ML Feature Extraction (Slice 2, Task 1)."""

from __future__ import annotations

import json
from pathlib import Path
import unittest

import sklearn_crfsuite

from mathia_ml.features import (
    FEATURE_SPEC_VERSION,
    WINDOW_OFFSETS,
    bucket_logic_density,
    bucket_math_density,
    extract_dataset,
    extract_document_features,
    extract_node_base_features,
    extract_node_features,
    extract_sequence_features,
    get_math_density_delta,
    node_ends_with_colon,
    offset_prefix,
    summarize_features,
)
from mathia_ml.schema import (
    DumpNode,
    KeywordHit,
    LabeledDocument,
    NodeFeatures,
    Reference,
)


class TestFeatureExtraction(unittest.TestCase):
    """Test suite for sliding window feature extraction."""

    def test_offset_prefix(self) -> None:
        self.assertEqual(offset_prefix(-1), "-1:")
        self.assertEqual(offset_prefix(0), "0:")
        self.assertEqual(offset_prefix(1), "+1:")
        self.assertEqual(offset_prefix(2), "+2:")
        self.assertEqual(offset_prefix(-2), "-2:")

    def test_bucket_math_density(self) -> None:
        self.assertEqual(bucket_math_density(0.0), "zero")
        self.assertEqual(bucket_math_density(0.001), "zero")
        self.assertEqual(bucket_math_density(0.05), "low")
        self.assertEqual(bucket_math_density(0.30), "low")
        self.assertEqual(bucket_math_density(0.31), "medium")
        self.assertEqual(bucket_math_density(0.70), "medium")
        self.assertEqual(bucket_math_density(0.71), "high")
        self.assertEqual(bucket_math_density(1.0), "high")

    def test_bucket_logic_density(self) -> None:
        self.assertEqual(bucket_logic_density(0.0), "zero")
        self.assertEqual(bucket_logic_density(0.001), "zero")
        self.assertEqual(bucket_logic_density(0.10), "low")
        self.assertEqual(bucket_logic_density(0.35), "low")
        self.assertEqual(bucket_logic_density(0.36), "medium")
        self.assertEqual(bucket_logic_density(0.70), "medium")
        self.assertEqual(bucket_logic_density(0.71), "high")
        self.assertEqual(bucket_logic_density(1.0), "high")

    def test_get_math_density_delta(self) -> None:
        self.assertEqual(get_math_density_delta(0.5, None), "none")
        self.assertEqual(get_math_density_delta(0.6, 0.4), "up")
        self.assertEqual(get_math_density_delta(0.3, 0.5), "down")
        self.assertEqual(get_math_density_delta(0.52, 0.50), "flat")
        self.assertEqual(get_math_density_delta(0.50, 0.52), "flat")

    def test_node_ends_with_colon(self) -> None:
        self.assertTrue(node_ends_with_colon("Theorem 1:"))
        self.assertTrue(node_ends_with_colon("Theorem 1:   \n"))
        self.assertTrue(node_ends_with_colon("قضیه ۱.۲: "))
        self.assertTrue(node_ends_with_colon("Proof："))  # Full-width colon
        self.assertFalse(node_ends_with_colon("Theorem 1."))
        self.assertFalse(node_ends_with_colon("Just text"))
        self.assertFalse(node_ends_with_colon(""))

    def test_single_node_boundaries(self) -> None:
        node = DumpNode(index=0, kind="paragraph", flat_text="A single line.", char_len=14)
        feat = NodeFeatures(math_density=0.0)

        feats = extract_node_features([node], [feat], 0)
        self.assertIn("bias", feats)
        self.assertEqual(feats["bias"], 1.0)
        self.assertIn("0:is_first_node=true", feats)
        self.assertIn("0:is_last_node=true", feats)
        self.assertIn("0:math_density_delta=none", feats)
        self.assertIn("0:node_kind=paragraph", feats)

        # No -1 or +1 window features should exist
        for key in feats:
            self.assertFalse(key.startswith("-1:"), f"Unexpected key: {key}")
            self.assertFalse(key.startswith("+1:"), f"Unexpected key: {key}")

    def test_three_node_window(self) -> None:
        nodes = [
            DumpNode(index=0, kind="heading", flat_text="# Introduction", char_len=14),
            DumpNode(index=1, kind="paragraph", flat_text="Theorem 1.1:", char_len=12),
            DumpNode(index=2, kind="display_math", flat_text="x^2 + y^2 = z^2", char_len=15),
        ]
        feats = [
            NodeFeatures(math_density=0.0),
            NodeFeatures(
                lead=KeywordHit(kind="theorem", number="1.1", labelled=True),
                math_density=0.2,
                emphasized=True,
            ),
            NodeFeatures(math_density=1.0),
        ]

        seq_feats = extract_sequence_features(nodes, feats)
        self.assertEqual(len(seq_feats), 3)

        # Node 0 (First node)
        self.assertIn("0:is_first_node=true", seq_feats[0])
        self.assertNotIn("0:is_last_node=true", seq_feats[0])
        self.assertIn("0:node_kind=heading", seq_feats[0])
        self.assertIn("+1:node_kind=paragraph", seq_feats[0])
        self.assertIn("+1:lead=theorem", seq_feats[0])
        self.assertIn("+1:ends_colon=true", seq_feats[0])
        for key in seq_feats[0]:
            self.assertFalse(key.startswith("-1:"))

        # Node 1 (Middle node)
        self.assertNotIn("0:is_first_node=true", seq_feats[1])
        self.assertNotIn("0:is_last_node=true", seq_feats[1])
        self.assertIn("-1:node_kind=heading", seq_feats[1])
        self.assertIn("0:node_kind=paragraph", seq_feats[1])
        self.assertIn("0:lead=theorem", seq_feats[1])
        self.assertNotIn("0:lead_kind=theorem", seq_feats[1])
        self.assertIn("0:labelled=true", seq_feats[1])
        self.assertIn("0:lead_has_number=true", seq_feats[1])
        self.assertIn("0:emphasized=true", seq_feats[1])
        self.assertIn("0:ends_colon=true", seq_feats[1])
        self.assertIn("+1:node_kind=display_math", seq_feats[1])
        self.assertIn("+1:math_density=high", seq_feats[1])

        # Node 2 (Last node)
        self.assertNotIn("0:is_first_node=true", seq_feats[2])
        self.assertIn("0:is_last_node=true", seq_feats[2])
        self.assertIn("0:node_kind=display_math", seq_feats[2])
        self.assertIn("-1:node_kind=paragraph", seq_feats[2])
        self.assertIn("-1:ends_colon=true", seq_feats[2])
        self.assertNotIn("0:prev_ends_with_colon=true", seq_feats[2])
        self.assertIn("0:math_density_delta=up", seq_feats[2])
        for key in seq_feats[2]:
            self.assertFalse(key.startswith("+1:"))

    def test_all_node_features_signals(self) -> None:
        node = DumpNode(index=0, kind="paragraph", flat_text="Proof of Theorem 1.", char_len=19)
        feat = NodeFeatures(
            lead=KeywordHit(
                kind="proof",
                number=None,
                name="Cauchy",
                labelled=True,
                body_empty=True,
                reference=True,
                refers_to=Reference(kind="theorem", number="1"),
            ),
            mentions=["lemma", "proposition"],
            emphasized=True,
            math_density=0.85,
            logic_count=4,
            logic_density=1.0,
            qed=True,
            definitional=True,
            proof_cue=True,
            statement_cue=True,
            continuation=True,
        )

        extracted = extract_node_base_features(node, feat, 0)
        self.assertIn("0:node_kind=paragraph", extracted)
        self.assertIn("0:has_lead=true", extracted)
        self.assertIn("0:lead=proof", extracted)
        self.assertNotIn("0:lead_kind=proof", extracted)
        self.assertIn("0:labelled=true", extracted)
        self.assertNotIn("0:lead_labelled=true", extracted)
        self.assertIn("0:body_empty=true", extracted)
        self.assertIn("0:lead_reference=true", extracted)
        self.assertIn("0:lead_has_name=true", extracted)
        self.assertIn("0:lead_refers_to=theorem", extracted)
        self.assertIn("0:emphasized=true", extracted)
        self.assertIn("0:math_density=high", extracted)
        self.assertIn("0:logic_density=high", extracted)
        self.assertIn("0:qed=true", extracted)
        self.assertIn("0:definitional=true", extracted)
        self.assertIn("0:proof_cue=true", extracted)
        self.assertIn("0:statement_cue=true", extracted)
        self.assertIn("0:continuation=true", extracted)
        self.assertIn("0:mention=lemma", extracted)
        self.assertIn("0:mention=proposition", extracted)

    def test_relational_after_display_math(self) -> None:
        nodes = [
            DumpNode(index=0, kind="display_math", flat_text="\\int f dx", char_len=9),
            DumpNode(index=1, kind="paragraph", flat_text="where f is bounded.", char_len=19),
        ]
        feats = [
            NodeFeatures(math_density=1.0),
            NodeFeatures(math_density=0.1, continuation=True),
        ]

        seq_feats = extract_sequence_features(nodes, feats)
        self.assertIn("-1:node_kind=display_math", seq_feats[1])
        self.assertNotIn("0:after_display_math=true", seq_feats[1])
        self.assertIn("0:continuation=true", seq_feats[1])
        self.assertIn("0:math_density_delta=down", seq_feats[1])

    def test_extract_dataset_and_summarize(self) -> None:
        doc = LabeledDocument(
            doc_id="doc_1",
            language="en",
            nodes=[
                DumpNode(index=0, kind="paragraph", flat_text="Theorem 1:", char_len=10),
                DumpNode(index=1, kind="paragraph", flat_text="Statement text.", char_len=15),
            ],
            features=[
                NodeFeatures(lead=KeywordHit(kind="theorem", labelled=True)),
                NodeFeatures(math_density=0.0),
            ],
            bio_tags=["B-THEOREM", "I-THEOREM"],
        )

        X, y = extract_dataset([doc])
        self.assertEqual(len(X), 1)
        self.assertEqual(len(y), 1)
        self.assertEqual(len(X[0]), 2)
        self.assertEqual(y[0], ["B-THEOREM", "I-THEOREM"])

        summary = summarize_features(X)
        self.assertEqual(summary["total_documents"], 1)
        self.assertEqual(summary["total_nodes"], 2)
        self.assertGreater(summary["unique_feature_keys"], 5)
        self.assertGreater(summary["avg_features_per_node"], 5.0)

    def test_real_dataset_compatibility(self) -> None:
        train_path = Path("ml/data/train.jsonl")
        if not train_path.exists():
            train_path = Path("data/train.jsonl")
        if not train_path.exists():
            self.skipTest("train.jsonl does not exist")

        docs = []
        with open(train_path, "r", encoding="utf-8") as f:
            for idx, line in enumerate(f):
                if idx >= 10:  # Sample first 10 documents
                    break
                docs.append(json.loads(line))

        X, y = extract_dataset(docs)
        self.assertEqual(len(X), len(docs))
        self.assertEqual(len(y), len(docs))

        # Check all features are 1.0 and keys are well-formed strings
        for seq in X:
            for node_feats in seq:
                self.assertIn("bias", node_feats)
                for k, v in node_feats.items():
                    self.assertIsInstance(k, str)
                    self.assertEqual(v, 1.0)
                    self.assertTrue(
                        k == "bias" or ":" in k,
                        f"Feature key missing colon prefix: {k}",
                    )

    def test_crfsuite_training_integration(self) -> None:
        """Verify sklearn_crfsuite can train and predict on extracted features."""
        train_path = Path("ml/data/train.jsonl")
        if not train_path.exists():
            train_path = Path("data/train.jsonl")
        if not train_path.exists():
            self.skipTest("train.jsonl does not exist")

        docs = []
        with open(train_path, "r", encoding="utf-8") as f:
            for idx, line in enumerate(f):
                if idx >= 20:  # Use 20 docs for rapid integration check
                    break
                docs.append(json.loads(line))

        X, y = extract_dataset(docs)
        crf = sklearn_crfsuite.CRF(
            algorithm="lbfgs",
            c1=0.1,
            c2=0.1,
            max_iterations=15,
            all_possible_transitions=True,
        )
        crf.fit(X, y)

        preds = crf.predict(X[:3])
        self.assertEqual(len(preds), 3)
        self.assertEqual(len(preds[0]), len(y[0]))
        self.assertGreater(len(crf.state_features_), 10)
        self.assertGreater(len(crf.transition_features_), 1)


if __name__ == "__main__":
    unittest.main()
