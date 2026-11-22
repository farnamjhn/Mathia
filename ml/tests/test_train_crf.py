"""Unit tests for CRF Training and Weights Exporter (Slice 2, Task 2)."""

from __future__ import annotations

import json
from pathlib import Path
import tempfile
import unittest

from mathia_ml.train_crf import (
    DEFAULT_OUTPUT_WEIGHTS,
    export_crf_weights_json,
    format_crf_weights,
    train_crf_model,
    tune_hyperparameters,
    verify_viterbi_parity,
    viterbi_decode,
)


class TestTrainCRF(unittest.TestCase):
    """Test suite for CRF training, tuning, and weight export."""

    def setUp(self) -> None:
        self.X_train = [
            [
                {"bias": 1.0, "0:node_kind=heading": 1.0, "0:is_first_node=true": 1.0},
                {
                    "bias": 1.0,
                    "-1:node_kind=heading": 1.0,
                    "0:node_kind=paragraph": 1.0,
                    "0:lead=theorem": 1.0,
                    "0:labelled=true": 1.0,
                    "+1:node_kind=paragraph": 1.0,
                },
                {
                    "bias": 1.0,
                    "-1:lead=theorem": 1.0,
                    "0:node_kind=paragraph": 1.0,
                    "0:is_last_node=true": 1.0,
                },
            ],
            [
                {
                    "bias": 1.0,
                    "0:node_kind=paragraph": 1.0,
                    "0:lead=proof": 1.0,
                    "0:is_first_node=true": 1.0,
                    "+1:node_kind=paragraph": 1.0,
                },
                {
                    "bias": 1.0,
                    "-1:lead=proof": 1.0,
                    "0:node_kind=paragraph": 1.0,
                    "0:qed=true": 1.0,
                    "0:is_last_node=true": 1.0,
                },
            ],
        ]
        self.y_train = [
            ["O", "B-THEOREM", "I-THEOREM"],
            ["B-PROOF", "I-PROOF"],
        ]

    def test_train_and_format_weights(self) -> None:
        crf = train_crf_model(self.X_train, self.y_train, c1=0.1, c2=0.1, max_iterations=20)
        weights = format_crf_weights(crf, precision=4)

        self.assertIn("labels", weights)
        self.assertIn("transitions", weights)
        self.assertIn("state_features", weights)

        labels = weights["labels"]
        K = len(labels)
        self.assertGreater(K, 0)
        self.assertEqual(labels[0], "O")  # "O" is positioned at index 0

        # Transitions must be K x K
        transitions = weights["transitions"]
        self.assertEqual(len(transitions), K)
        for row in transitions:
            self.assertEqual(len(row), K)
            for val in row:
                self.assertIsInstance(val, float)

        # State features must map feature string to { label_idx_string: float }
        state_features = weights["state_features"]
        self.assertGreater(len(state_features), 0)
        for feat_key, label_map in state_features.items():
            self.assertIsInstance(feat_key, str)
            self.assertIsInstance(label_map, dict)
            for l_idx_str, w in label_map.items():
                l_idx = int(l_idx_str)
                self.assertTrue(0 <= l_idx < K)
                self.assertIsInstance(w, float)

    def test_viterbi_decode_and_parity(self) -> None:
        crf = train_crf_model(self.X_train, self.y_train, c1=0.1, c2=0.1, max_iterations=20)
        weights = format_crf_weights(crf, precision=5)

        parity, matches, total = verify_viterbi_parity(crf, weights, self.X_train)
        self.assertEqual(parity, 1.0)
        self.assertEqual(matches, total)
        self.assertEqual(total, 5)

    def test_viterbi_empty_sequence(self) -> None:
        weights = {
            "labels": ["O", "B-THEOREM"],
            "transitions": [[0.0, 0.0], [0.0, 0.0]],
            "state_features": {},
        }
        pred = viterbi_decode([], weights)
        self.assertEqual(pred, [])

    def test_tune_hyperparameters(self) -> None:
        best_params, best_score, trials = tune_hyperparameters(
            self.X_train,
            self.y_train,
            self.X_train,  # Use train as val for toy test
            self.y_train,
            c1_candidates=[0.1, 0.2],
            c2_candidates=[0.1, 0.2],
            max_iterations=15,
        )
        self.assertEqual(len(trials), 4)
        self.assertIn(best_params[0], [0.1, 0.2])
        self.assertIn(best_params[1], [0.1, 0.2])
        self.assertGreaterEqual(best_score, 0.0)

    def test_exported_crf_weights_json_file(self) -> None:
        weights_path = Path(DEFAULT_OUTPUT_WEIGHTS)
        if not weights_path.exists():
            weights_path = Path("..") / DEFAULT_OUTPUT_WEIGHTS
        self.assertTrue(
            weights_path.exists(),
            f"Expected weights file at {weights_path} does not exist",
        )

        with open(weights_path, "r", encoding="utf-8") as f:
            data = json.load(f)

        self.assertIn("labels", data)
        self.assertIn("transitions", data)
        self.assertIn("state_features", data)

        labels = data["labels"]
        self.assertIn("O", labels)
        self.assertIn("B-THEOREM", labels)
        self.assertIn("I-THEOREM", labels)
        self.assertIn("B-PROOF", labels)

        # File size must be under 100 KB
        file_size_kb = weights_path.stat().st_size / 1024
        self.assertLess(file_size_kb, 100.0, f"Weights file too large: {file_size_kb:.2f} KB")

    def test_export_crf_weights_json_helper(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            out_path = Path(tmpdir) / "test_weights.json"
            sample_weights = {
                "labels": ["O", "B-THEOREM"],
                "transitions": [[0.1, -0.2], [0.3, -0.4]],
                "state_features": {"0:lead=theorem": {"1": 2.5}},
            }
            res_path = export_crf_weights_json(sample_weights, out_path, compact=True)
            self.assertTrue(res_path.exists())

            with open(res_path, "r", encoding="utf-8") as f:
                loaded = json.load(f)
            self.assertEqual(loaded["labels"], ["O", "B-THEOREM"])
            self.assertEqual(loaded["state_features"]["0:lead=theorem"]["1"], 2.5)


if __name__ == "__main__":
    unittest.main()
