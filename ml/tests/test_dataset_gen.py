import random
import unittest
from mathia_ml.dataset_gen import (
    DocumentSegment,
    parse_tex_environments,
    rewrite_opener,
    inject_noise_and_strip,
    split_atomic_chunks,
)
from mathia_ml.schema import LabeledDocument, DumpNode, NodeFeatures


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

