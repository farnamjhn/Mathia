"""Unit tests for Evaluation Harness (Slice 2, Task 4)."""

from __future__ import annotations

import json
from pathlib import Path
import tempfile
import unittest

from mathia_ml.evaluate import (
    compute_metrics,
    evaluate_dataset,
    format_report_markdown,
    linear_score,
    score_node,
    tag_heuristic,
)


class TestEvaluate(unittest.TestCase):
    """Test suite for evaluation harness, heuristic tagger, and metric reporting."""

    def setUp(self) -> None:
        self.sample_nodes = [
            {"index": 0, "kind": "heading", "flat_text": "# Title"},
            {
                "index": 1,
                "kind": "paragraph",
                "flat_text": "**Theorem 1.** For every $x \\in X$, $f(x) \\ge 0$.",
            },
            {
                "index": 2,
                "kind": "paragraph",
                "flat_text": "Proof. Clear by inspection. □",
            },
        ]
        self.sample_features = [
            {
                "lead": None,
                "mentions": [],
                "emphasized": False,
                "math_density": 0.0,
                "logic_density": 0.0,
                "qed": False,
                "definitional": False,
                "proof_cue": False,
                "statement_cue": False,
                "continuation": False,
            },
            {
                "lead": {
                    "kind": "theorem",
                    "number": "1",
                    "name": None,
                    "labelled": True,
                    "body_empty": False,
                    "reference": False,
                    "refers_to": None,
                },
                "mentions": ["theorem"],
                "emphasized": True,
                "math_density": 0.5,
                "logic_density": 0.0,
                "qed": False,
                "definitional": False,
                "proof_cue": False,
                "statement_cue": True,
                "continuation": False,
            },
            {
                "lead": {
                    "kind": "proof",
                    "number": None,
                    "name": None,
                    "labelled": True,
                    "body_empty": False,
                    "reference": False,
                    "refers_to": None,
                },
                "mentions": ["proof"],
                "emphasized": False,
                "math_density": 0.0,
                "logic_density": 0.0,
                "qed": True,
                "definitional": False,
                "proof_cue": True,
                "statement_cue": False,
                "continuation": False,
            },
        ]

    def test_linear_score_and_score_node(self) -> None:
        score_thm = linear_score(self.sample_features[1], "theorem")
        self.assertGreaterEqual(score_thm, 0.70)

        opens_thm, kind_thm, best_s = score_node(self.sample_nodes[1], self.sample_features[1])
        self.assertTrue(opens_thm)
        self.assertEqual(kind_thm, "theorem")
        self.assertGreaterEqual(best_s, 0.50)

        opens_hd, _, _ = score_node(self.sample_nodes[0], self.sample_features[0])
        self.assertFalse(opens_hd)

    def test_tag_heuristic_toy_sequence(self) -> None:
        tags = tag_heuristic(self.sample_nodes, self.sample_features)
        self.assertEqual(len(tags), 3)
        self.assertEqual(tags[0], "O")
        self.assertEqual(tags[1], "B-THEOREM")
        self.assertEqual(tags[2], "B-PROOF")

    def test_compute_metrics(self) -> None:
        y_true = [["O", "B-THEOREM", "I-THEOREM", "O"]]
        y_pred = [["O", "B-THEOREM", "I-THEOREM", "O"]]

        metrics = compute_metrics(y_true, y_pred)
        self.assertEqual(metrics["f1_score"], 1.0)
        self.assertEqual(metrics["precision"], 1.0)
        self.assertEqual(metrics["recall"], 1.0)
        self.assertIn("THEOREM", metrics["per_class"])

        # Mismatched prediction
        y_bad = [["O", "B-THEOREM", "O", "O"]]
        metrics_bad = compute_metrics(y_true, y_bad)
        self.assertLess(metrics_bad["f1_score"], 1.0)

    def test_evaluate_dataset_end_to_end(self) -> None:
        test_path = Path("data/test.jsonl")
        if not test_path.exists():
            test_path = Path("ml/data/test.jsonl")
        if not test_path.exists():
            self.skipTest(f"Test dataset not found at {test_path}")

        weights_path = Path("../crates/math-core/src/crf_weights.json")
        if not weights_path.exists():
            weights_path = Path("crates/math-core/src/crf_weights.json")

        results = evaluate_dataset(test_path=test_path, weights_path=weights_path)

        self.assertIn("overall", results)
        self.assertIn("by_language", results)
        self.assertIn("challenges", results)

        ov = results["overall"]
        self.assertIn("heuristic", ov)
        self.assertIn("crf", ov)
        self.assertGreater(ov["crf"]["f1_score"], 0.70)
        self.assertGreater(ov["heuristic"]["f1_score"], 0.70)

        # Markdown report generation
        md = format_report_markdown(results)
        self.assertIn("# Mathia Block Detection Evaluation Report", md)
        self.assertIn("Exact-Span F1", md)
        self.assertIn("Multi-Paragraph Statements", md)
        self.assertIn("QED-less Proofs", md)


if __name__ == "__main__":
    unittest.main()
