import random
import unittest
from mathia_ml.dataset_gen import (
    BLOCK_TYPES,
    DocumentSegment,
    align_tags_to_nodes,
    generate_synthetic_document_segments,
    get_domains,
    inject_noise_and_strip,
    parse_tex_environments,
    rewrite_opener,
    split_atomic_chunks,
)
from mathia_ml.schema import DumpNode, LabeledDocument, NodeFeatures


class TestDatasetGen(unittest.TestCase):
    def test_parse_tex_environments(self):
        tex = r"""
\section{Intro}
Some text here.

\begin{theorem}[Cauchy]
Let $f$ be continuous.
\end{theorem}

\begin{proof}
Immediate from definition. \blacksquare
\end{proof}
"""
        segments = parse_tex_environments(tex, language="en")
        self.assertEqual(len(segments), 3)
        self.assertFalse(segments[0].is_env)
        self.assertTrue(segments[1].is_env)
        self.assertEqual(segments[1].block_type, "theorem")
        self.assertEqual(segments[1].name, "Cauchy")
        self.assertTrue(segments[2].is_env)
        self.assertEqual(segments[2].block_type, "proof")

    def test_parse_tex_environments_with_align_and_itemize_inside_theorem(self):
        tex = r"""
\begin{theorem}[Spectral]
Let $T$ be self-adjoint. Then:
\begin{align}
T v &= \lambda v \\
\lambda &\in \mathbb{R}
\end{align}
and the properties:
\begin{itemize}
\item First property
\item Second property
\end{itemize}
\end{theorem}
"""
        segments = parse_tex_environments(tex, language="en")
        self.assertEqual(len(segments), 1)
        self.assertTrue(segments[0].is_env)
        self.assertEqual(segments[0].block_type, "theorem")
        self.assertEqual(segments[0].name, "Spectral")
        combined_text = "\n\n".join(segments[0].paragraphs)
        self.assertIn(r"\begin{align}", combined_text)
        self.assertIn(r"\begin{itemize}", combined_text)
        # Verify unknown environments are not converted to theorem blocks
        self.assertNotIn("align", [s.block_type for s in segments if s.is_env and s.block_type != "theorem"])

    def test_align_tags_to_nodes_drops_mismatch(self):
        nodes = [
            DumpNode(index=0, kind="paragraph", flat_text="A", char_len=1),
            DumpNode(index=1, kind="paragraph", flat_text="B", char_len=1),
        ]
        # Length mismatch: 2 nodes vs 3 expected tags
        expected_tags = ["B-THEOREM", "I-THEOREM", "O"]
        result = align_tags_to_nodes(nodes, expected_tags, [])
        # Must return None on length mismatch (no truncation or padding with "O")
        self.assertIsNone(result)

        # Matching length
        matching_tags = ["B-THEOREM", "I-THEOREM"]
        self.assertEqual(align_tags_to_nodes(nodes, matching_tags, []), matching_tags)

    def test_synthetic_generation_covers_all_12_block_types(self):
        rng = random.Random(42)
        covered_types = set()

        for split in ["train", "val"]:
            for lang in ["en", "fa"]:
                for i in range(150):
                    _, segments = generate_synthetic_document_segments(
                        f"test_{split}_{lang}_{i}", lang, rng, split=split
                    )
                    for seg in segments:
                        if seg.is_env and seg.block_type:
                            covered_types.add(seg.block_type)

        for btype in BLOCK_TYPES:
            self.assertIn(btype, covered_types, f"BlockType '{btype}' was not generated!")

    def test_synthetic_templates_disjoint_between_train_and_val_test(self):
        def collect_template_texts(domains):
            texts = set()
            for d in domains:
                for h in d.get("headings", []):
                    texts.add(h)
                for intro in d.get("intros", []):
                    texts.add(intro)
                for key in ["theorems", "lemmas", "propositions", "corollaries", "claims"]:
                    for item in d.get(key, []):
                        texts.add(item["statement"])
                        if "proof" in item:
                            texts.add(item["proof"])
                for defn in d.get("definitions", []):
                    texts.add(defn["statement"])
                for ex in d.get("examples", []):
                    texts.add(ex)
                for rem in d.get("remarks", []):
                    texts.add(rem)
                for ex in d.get("exercises", []):
                    texts.add(ex["statement"])
                    if "solution" in ex:
                        texts.add(ex["solution"])
                for notat in d.get("notations", []):
                    texts.add(notat)
            return texts

        # English train vs eval templates
        en_train_texts = collect_template_texts(get_domains("en", split="train"))
        en_eval_texts = collect_template_texts(get_domains("en", split="val"))
        overlap_en = en_train_texts & en_eval_texts
        self.assertEqual(overlap_en, set(), f"Found overlapping English templates: {overlap_en}")

        # Persian train vs eval templates
        fa_train_texts = collect_template_texts(get_domains("fa", split="train"))
        fa_eval_texts = collect_template_texts(get_domains("fa", split="val"))
        overlap_fa = fa_train_texts & fa_eval_texts
        self.assertEqual(overlap_fa, set(), f"Found overlapping Persian templates: {overlap_fa}")

    def test_split_atomic_chunks(self):
        text = "Let $f$ be continuous:\n\n$$\\int f dx = 0$$\n\nwhere $c$ is constant."
        chunks = split_atomic_chunks(text)
        self.assertEqual(len(chunks), 3)
        self.assertEqual(chunks[0], "Let $f$ be continuous:")
        self.assertEqual(chunks[1], "$$\\int f dx = 0$$")
        self.assertEqual(chunks[2], "where $c$ is constant.")

    def test_rewrite_opener_persian_and_english(self):
        rng = random.Random(42)
        opener_en = rewrite_opener("theorem", "1.2", "Cauchy", "en", rng, drop_bold_or_colon=False)
        self.assertIn("Theorem", opener_en)
        self.assertIn("1.2", opener_en)
        self.assertIn("Cauchy", opener_en)
        self.assertTrue(opener_en.startswith("**"))

        opener_fa = rewrite_opener("theorem", "۱.۲", "کوشی", "fa", rng, drop_bold_or_colon=False)
        self.assertIn("قضیه", opener_fa)
        self.assertIn("۱.۲", opener_fa)
        self.assertIn("کوشی", opener_fa)
        self.assertTrue(opener_fa.startswith("**"))

    def test_inject_noise_and_strip(self):
        rng = random.Random(42)
        segments = [
            DocumentSegment(
                block_type=None,
                name=None,
                number=None,
                paragraphs=["# Title", "Intro text."],
                is_env=False,
            ),
            DocumentSegment(
                block_type="theorem",
                name="Euler",
                number="1.1",
                paragraphs=["Let $G$ be a graph. Then $V - E + F = 2$."],
                is_env=True,
            ),
            DocumentSegment(
                block_type="proof",
                name=None,
                number=None,
                paragraphs=["By induction on the number of faces."],
                is_env=True,
            ),
        ]

        md, tags = inject_noise_and_strip(segments, rng)
        self.assertEqual(tags[0], "O")
        self.assertEqual(tags[1], "O")
        self.assertTrue(any(t.startswith("B-THEOREM") for t in tags))
        self.assertTrue(any(t.startswith("B-PROOF") for t in tags))


if __name__ == "__main__":
    unittest.main()

