import json
import tempfile
import unittest
from pathlib import Path

from mathia_ml.dataset_gen import find_mathia_binary
from mathia_ml.export_feedback import (
    detect_language,
    export_feedback_directory,
    export_feedback_from_markdown_and_overrides,
    export_feedback_telemetry,
    load_overrides_dict,
    merge_and_resplit,
    reconstruct_overrides_from_log,
)
from mathia_ml.split import load_jsonl


class TestExportFeedback(unittest.TestCase):
    def test_detect_language(self):
        self.assertEqual(detect_language("This is a theorem about compact manifolds."), "en")
        self.assertEqual(detect_language("قضیه ۱. فرض کنید $G$ یک گروه متناهی باشد."), "fa")
        self.assertEqual(detect_language("اثبات. با استقرا روی تعداد عناصر."), "fa")
        self.assertEqual(detect_language("$$\\int_0^1 f(x) dx = 0$$"), "en")

    def test_load_overrides_dict_formats(self):
        # 1. Bare ScopeOverrides
        bare = {
            "delta": {"1": 1},
            "suppressed": ["3"],
            "retype": {"0": "lemma"},
            "end": {"2": 5},
        }
        ov, meta = load_overrides_dict(bare)
        self.assertEqual(ov["delta"], {"1": 1})
        self.assertEqual(ov["retype"], {"0": "lemma"})

        # 2. Replay / judge log
        replay = {
            "protocol": 1,
            "model_version": "gemini-1.5-pro",
            "overrides": bare,
            "log": [{"round": 1, "verdict": "accept", "applied": True}],
        }
        ov2, meta2 = load_overrides_dict(replay)
        self.assertEqual(ov2["delta"], {"1": 1})
        self.assertEqual(meta2["model_version"], "gemini-1.5-pro")

        # 3. Log of verdicts
        log_only = {
            "log": [
                {
                    "round": 1,
                    "span": {"start": 0, "end": 2},
                    "verdict": "incomplete",
                    "applied": True,
                },
                {
                    "round": 2,
                    "span": {"start": 4, "end": 5},
                    "verdict": "reject",
                    "applied": True,
                },
            ]
        }
        ov3, meta3 = load_overrides_dict(log_only)
        self.assertEqual(ov3["delta"], {"0": 1})
        self.assertIn("4", ov3["suppressed"])

    def test_reconstruct_overrides_from_log(self):
        log = [
            {"span": {"start": 0, "end": 2}, "verdict": "incomplete", "applied": True},
            {"span": {"start": 0, "end": 3}, "verdict": "incomplete", "applied": True},
            {"span": {"start": 5, "end": 6}, "verdict": "overreach", "applied": True},
            {"span": {"start": 8, "end": 9}, "verdict": "reject", "applied": True},
            {"span": {"start": 2, "end": 4}, "verdict": {"retype": "proposition"}, "applied": True},
            {"span": {"start": 1, "end": 3}, "verdict": {"end_at": 7}, "applied": True},
            {"span": {"start": 10, "end": 12}, "verdict": "accept", "applied": True},
            {"span": {"start": 15, "end": 16}, "verdict": "incomplete", "applied": False},  # not applied
        ]
        ov = reconstruct_overrides_from_log(log)
        self.assertEqual(ov["delta"]["0"], 2)
        self.assertEqual(ov["delta"]["5"], -1)
        self.assertNotIn("15", ov["delta"])
        self.assertIn("8", ov["suppressed"])
        self.assertEqual(ov["retype"]["2"], "proposition")
        self.assertEqual(ov["end"]["1"], 7)

    def test_export_feedback_with_mathia_cli_and_overrides(self):
        mathia_bin = find_mathia_binary()
        md = "# Group Theory\n\n**Theorem 1.** Let $G$ be a finite group.\n\nThen $|G| < \\infty$.\n\n$$\\prod_{g \\in G} g$$\n\nProof.\nImmediate from definition. $\\blacksquare$\n"
        overrides = {
            "retype": {"1": "lemma"},
        }

        doc = export_feedback_from_markdown_and_overrides(
            markdown_content=md,
            overrides_data=overrides,
            doc_id="feedback_test_01",
            mathia_bin=mathia_bin,
        )

        self.assertEqual(doc.doc_id, "feedback_test_01")
        self.assertEqual(doc.language, "en")
        self.assertEqual(doc.metadata["source"], "user_correction")
        self.assertGreater(len(doc.nodes), 3)
        self.assertEqual(len(doc.nodes), len(doc.bio_tags))
        self.assertIn("B-LEMMA", doc.bio_tags)
        self.assertIn("B-PROOF", doc.bio_tags)

    def test_export_feedback_directory_and_telemetry(self):
        mathia_bin = find_mathia_binary()

        with tempfile.TemporaryDirectory() as tmp_dir:
            tmp_path = Path(tmp_dir)

            # Create document and overrides file pair
            md_file = tmp_path / "algebra.md"
            md_file.write_text("# Linear Algebra\n\n**Lemma 1.** Let $V$ be a vector space.\n", encoding="utf-8")
            ov_file = tmp_path / "algebra.overrides.json"
            ov_file.write_text(json.dumps({"retype": {"1": "proposition"}}), encoding="utf-8")

            docs_from_dir = export_feedback_directory(tmp_path, mathia_bin=mathia_bin)
            self.assertEqual(len(docs_from_dir), 1)
            self.assertEqual(docs_from_dir[0].doc_id, "feedback_algebra")
            self.assertEqual(docs_from_dir[0].metadata["source"], "user_correction")
            self.assertIn("B-PROPOSITION", docs_from_dir[0].bio_tags)

            # Create telemetry JSONL file
            telemetry_file = tmp_path / "telemetry.jsonl"
            tel_record = {
                "doc_id": "tel_001",
                "markdown": "# Persian Math\n\nقضیه ۱. فرض کنید $X$ یک فضای متری باشد.\n",
                "overrides": {"retype": {"1": "lemma"}},
                "language": "fa",
            }
            telemetry_file.write_text(json.dumps(tel_record) + "\n", encoding="utf-8")

            docs_from_tel = export_feedback_telemetry(telemetry_file, mathia_bin=mathia_bin)
            self.assertEqual(len(docs_from_tel), 1)
            self.assertEqual(docs_from_tel[0].doc_id, "tel_001")
            self.assertEqual(docs_from_tel[0].language, "fa")
            self.assertEqual(docs_from_tel[0].metadata["source"], "user_correction")
            self.assertIn("B-LEMMA", docs_from_tel[0].bio_tags)

            # Test merge and resplit
            data_dir = tmp_path / "data"
            data_dir.mkdir()
            (data_dir / "train.jsonl").write_text("", encoding="utf-8")
            (data_dir / "val.jsonl").write_text("", encoding="utf-8")
            (data_dir / "test.jsonl").write_text("", encoding="utf-8")

            all_new = docs_from_dir + docs_from_tel
            stats = merge_and_resplit(data_dir, all_new, seed=42)
            self.assertEqual(stats["total_documents"], 2)


if __name__ == "__main__":
    unittest.main()
