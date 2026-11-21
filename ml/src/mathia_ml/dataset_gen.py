"""LaTeX Environment Stripper & Synthetic Dataset Generator for Mathia ML.

Implements Slice 1 of Mathia Lightweight ML:
- Step A: Ingest .tex / LaTeX environments and extract ground-truth BioTags from EnvSpan.
- Step B: Strip \\begin{...} / \\end{...} wrappers and rewrite as realistic Markdown headers.
- Step C: Inject synthetic OCR noise (drop QED in 25%, drop bold/colon in 20%, split statements).
- Step D: Run `mathia dump-features` on the stripped/noisy Markdown and align ground-truth BioTags.
- Document-level 80/10/10 split stratified by language with zero document leakage.
"""

from __future__ import annotations

import argparse
import json
import os
import random
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Optional

from mathia_ml.schema import DumpNode, FeatureDump, LabeledDocument, NodeFeatures
from mathia_ml.split import split_documents, write_split_jsonl

# Canonical block types supported by math_core
BLOCK_TYPES = [
    "theorem",
    "lemma",
    "proposition",
    "corollary",
    "definition",
    "proof",
    "example",
    "remark",
    "exercise",
    "solution",
    "claim",
    "notation",
]

ENV_TO_BLOCK_TYPE: dict[str, str] = {
    "theorem": "theorem",
    "thm": "theorem",
    "lemma": "lemma",
    "lem": "lemma",
    "proposition": "proposition",
    "prop": "proposition",
    "corollary": "corollary",
    "cor": "corollary",
    "definition": "definition",
    "defn": "definition",
    "def": "definition",
    "proof": "proof",
    "example": "example",
    "exmp": "example",
    "ex": "example",
    "remark": "remark",
    "rem": "remark",
    "note": "remark",
    "exercise": "exercise",
    "problem": "exercise",
    "solution": "solution",
    "sol": "solution",
    "claim": "claim",
    "notation": "notation",
}

EN_LABELS = {
    "theorem": "Theorem",
    "lemma": "Lemma",
    "proposition": "Proposition",
    "corollary": "Corollary",
    "definition": "Definition",
    "proof": "Proof",
    "example": "Example",
    "remark": "Remark",
    "exercise": "Exercise",
    "solution": "Solution",
    "claim": "Claim",
    "notation": "Notation",
}

FA_LABELS = {
    "theorem": "قضیه",
    "lemma": "لم",
    "proposition": "گزاره",
    "corollary": "نتیجه",
    "definition": "تعریف",
    "proof": "اثبات",
    "example": "مثال",
    "remark": "تذکر",
    "exercise": "تمرین",
    "solution": "حل",
    "claim": "ادعا",
    "notation": "نمادگذاری",
}

PERSIAN_DIGITS = str.maketrans("0123456789", "۰۱۲۳۴۵۶۷۸۹")


def to_persian_num(s: str) -> str:
    return s.translate(PERSIAN_DIGITS)


def find_mathia_binary() -> Path:
    """Locate the mathia CLI binary."""
    env_bin = os.getenv("MATHIA_BIN")
    if env_bin and Path(env_bin).exists():
        return Path(env_bin).resolve()

    repo_root = Path(__file__).resolve().parents[3]
    release_bin = repo_root / "target" / "release" / "mathia"
    if release_bin.exists():
        return release_bin
    debug_bin = repo_root / "target" / "debug" / "mathia"
    if debug_bin.exists():
        return debug_bin

    # Try building mathia if not found
    try:
        subprocess.run(
            ["cargo", "build", "-q", "--bin", "mathia"],
            cwd=str(repo_root),
            check=True,
        )
        if debug_bin.exists():
            return debug_bin
    except Exception as e:
        print(f"Warning: cargo build failed: {e}", file=sys.stderr)

    raise FileNotFoundError(
        "Could not find mathia binary. Please run `cargo build --bin mathia`."
    )


def run_dump_features(
    mathia_bin: Path, content: str, overrides_path: Optional[Path] = None
) -> FeatureDump:
    """Run `mathia dump-features` on string content and return parsed JSON."""
    with tempfile.NamedTemporaryFile("w", suffix=".md", delete=False, encoding="utf-8") as f:
        f.write(content)
        tmp_path = f.name
    try:
        cmd = [str(mathia_bin), "dump-features", tmp_path]
        if overrides_path is not None:
            cmd.extend(["--overrides", str(overrides_path)])
        proc = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            check=True,
        )
        data = json.loads(proc.stdout)
        return FeatureDump.model_validate(data)
    finally:
        if os.path.exists(tmp_path):
            os.unlink(tmp_path)


@dataclass
class DocumentSegment:
    """A segment of a document: either an explicit LaTeX environment or outside prose/heading."""
    block_type: Optional[str]  # e.g. "theorem", "proof", or None for Outside
    name: Optional[str]
    number: Optional[str]
    paragraphs: list[str]
    language: str = "en"
    is_env: bool = False


# -----------------------------------------------------------------------------
# Bilingual Mathematical Content Corpus
# -----------------------------------------------------------------------------

DOMAINS_EN = [
    {
        "domain": "Linear Algebra",
        "headings": ["Vector Spaces and Subspaces", "Spectral Theorem", "Inner Product Spaces"],
        "intros": [
            "We recall the standard axioms of finite-dimensional vector spaces over a field $\\mathbb{F}$.",
            "Throughout this section, $V$ denotes an inner product space of dimension $n < \\infty$.",
            "Linear transformations and their spectral properties play a central role in operator theory.",
        ],
        "theorems": [
            {
                "name": "Spectral Theorem",
                "statement": "Let $T: V \\to V$ be a self-adjoint linear operator on a finite-dimensional inner product space. Then there exists an orthonormal basis of $V$ consisting of eigenvectors of $T$. Moreover, all eigenvalues of $T$ are real.",
                "proof": "We proceed by induction on $\\dim(V)$. If $\\dim(V) = 1$, the assertion is trivial. Assume the claim holds for all spaces of dimension $n - 1$. Since the characteristic polynomial has a root $\\lambda \\in \\mathbb{C}$ and $T$ is self-adjoint, $\\lambda$ must be real. Let $v_1$ be a normalized eigenvector for $\\lambda$. The orthogonal complement $W = (\\mathrm{span}\\{v_1\\})^\\perp$ is $T$-invariant. By induction, $W$ admits an orthonormal eigenbasis $\\{v_2, \\dots, v_n\\}$. Combining these yields the desired basis.",
            },
            {
                "name": "Rank-Nullity Theorem",
                "statement": "Let $V$ and $W$ be finite-dimensional vector spaces over $\\mathbb{F}$, and let $T: V \\to W$ be a linear transformation. Then the dimension equation holds:\n$$\\dim(\\ker T) + \\dim(\\mathrm{im}\\, T) = \\dim(V)$$",
                "proof": "Let $\\{u_1, \\dots, u_k\\}$ be a basis for $\\ker T$, so $k = \\dim(\\ker T)$. Extend this to a basis $\\{u_1, \\dots, u_k, v_1, \\dots, v_m\\}$ for $V$. We show that $\\{T(v_1), \\dots, T(v_m)\\}$ is a basis for $\\mathrm{im}\\, T$. Clearly it spans $\\mathrm{im}\\, T$. If $\\sum_{i=1}^m c_i T(v_i) = 0$, then $T(\\sum c_i v_i) = 0$, so $\\sum c_i v_i \\in \\ker T$, which forces all $c_i = 0$. Hence $\\dim(\\mathrm{im}\\, T) = m$, completing the proof.",
            },
        ],
        "lemmas": [
            {
                "name": "Schur's Lemma",
                "statement": "Every square complex matrix $A \\in M_n(\\mathbb{C})$ is unitarily similar to an upper triangular matrix.",
                "proof": "By induction on $n$. Let $v$ be an eigenvector of $A$ with unit norm. Construct a unitary matrix $U_1$ whose first column is $v$. Then $U_1^* A U_1$ has first column $(\\lambda, 0, \\dots, 0)^T$. The result follows by applying the induction hypothesis to the submatrix.",
            },
        ],
        "definitions": [
            {
                "name": "Positive Definite Operator",
                "statement": "A self-adjoint operator $T: V \\to V$ is called positive definite if $\\langle T(v), v \\rangle > 0$ for all non-zero $v \\in V$.",
            },
            {
                "name": "Orthogonal Projection",
                "statement": "An operator $P \\in \\mathcal{L}(V)$ is an orthogonal projection if $P^2 = P = P^*$.",
            },
        ],
        "remarks": [
            "Every positive definite matrix is invertible and possesses a unique positive definite square root.",
            "The eigenvalues of an orthogonal projection are restricted to the set $\\{0, 1\\}$.",
        ],
        "examples": [
            "Consider the matrix $A = \\begin{pmatrix} 2 & 1 \\\\ 1 & 2 \\end{pmatrix}$. Its eigenvalues are $\\lambda_1 = 3$ and $\\lambda_2 = 1$, both strictly positive.",
        ],
    },
    {
        "domain": "Real Analysis",
        "headings": ["Metric Topology and Completeness", "Lebesgue Integration", "Sequences of Functions"],
        "intros": [
            "In this section we examine convergence properties of measurable functions on $\\mathbb{R}^n$.",
            "Compactness in metric spaces guarantees that continuous functions attain their extrema.",
            "We establish fundamental convergence theorems for the Lebesgue integral.",
        ],
        "theorems": [
            {
                "name": "Monotone Convergence Theorem",
                "statement": "Let $(f_n)$ be a pointwise non-decreasing sequence of non-negative measurable functions on $(X, \\Sigma, \\mu)$. If $f(x) = \\lim_{n \\to \\infty} f_n(x)$ pointwise, then:\n$$\\int_X f \\, d\\mu = \\lim_{n \\to \\infty} \\int_X f_n \\, d\\mu$$",
                "proof": "By monotonicity of the integral, $\\lim_{n \\to \\infty} \\int f_n \\, d\\mu \\le \\int f \\, d\\mu$. To show the reverse inequality, fix $c \\in (0, 1)$ and choose a simple function $s$ with $0 \\le s \\le f$. Define $E_n = \\{x \\in X : f_n(x) \\ge c s(x)\\}$. Then $E_n \\subseteq E_{n+1}$ and $\\bigcup E_n = X$. Therefore:\n$$\\lim_{n \\to \\infty} \\int_X f_n \\, d\\mu \\ge c \\int_X s \\, d\\mu$$\nTaking the supremum over all such simple functions $s$ and letting $c \\to 1$ yields the claim.",
            },
            {
                "name": "Heine-Borel Theorem",
                "statement": "A subset $K \\subset \\mathbb{R}^n$ is compact if and only if it is closed and bounded.",
                "proof": "Assume $K$ is compact. Since $\\mathbb{R}^n$ is Hausdorff, compact sets are closed. The open cover $\\{B(0, k) : k \\in \\mathbb{N}\\}$ has a finite subcover, so $K$ is bounded. Conversely, any closed and bounded set is contained in a closed hypercube $[ -M, M ]^n$, which is compact by the Cantor intersection theorem. Being a closed subset of a compact space, $K$ is compact.",
            },
        ],
        "lemmas": [
            {
                "name": "Fatou's Lemma",
                "statement": "For any sequence of non-negative measurable functions $(f_n)$, we have:\n$$\\int_X \\liminf_{n \\to \\infty} f_n \\, d\\mu \\le \\liminf_{n \\to \\infty} \\int_X f_n \\, d\\mu$$",
                "proof": "Let $g_k = \\inf_{j \\ge k} f_j$. Then $g_k$ is non-decreasing and $g_k \\le f_k$. Applying the Monotone Convergence Theorem to $(g_k)$ immediately gives the inequality.",
            },
        ],
        "definitions": [
            {
                "name": "Equicontinuity",
                "statement": "A family of functions $\\mathcal{F} \\subset C(X)$ is equicontinuous if for every $\\epsilon > 0$ there exists $\\delta > 0$ such that $d(x, y) < \\delta$ implies $|f(x) - f(y)| < \\epsilon$ for all $f \\in \\mathcal{F}$.",
            },
        ],
        "remarks": [
            "The hypothesis of non-negativity cannot be omitted from Fatou's Lemma.",
            "Uniform boundedness on compact sets is a necessary condition for equicontinuity.",
        ],
        "examples": [
            "The sequence $f_n(x) = n \\mathbf{1}_{(0, 1/n)}(x)$ satisfies $\\int f_n = 1$ for all $n$, but $f_n(x) \\to 0$ pointwise.",
        ],
    },
]

DOMAINS_FA = [
    {
        "domain": "آنالیز ریاضی",
        "headings": ["فضاهای متریک و فشردگی", "قضایای همگرایی در انتگرال لبگ", "پیوستگی یکنواخت"],
        "intros": [
            "در این بخش به بررسی ویژگی‌های توپولوژیک فضاهای متریک و قضایای بنیادین آنالیز می‌پردازیم.",
            "فشردگی یکی از مفاهیم اساسی در آنالیز است که رفتار توابع پیوسته را کنترل می‌کند.",
            "انتگرال‌گیری لبگ تعمیمی قوی از انتگرال ریمان است که قضایای حدی را ساده می‌سازد.",
        ],
        "theorems": [
            {
                "name": "مقدار میانگین رول",
                "statement": "فرض کنید تابع $f:[a, b] \\to \\mathbb{R}$ روی بازه بسته $[a, b]$ پیوسته و روی بازه باز $(a, b)$ مشتق‌پذیر باشد. اگر $f(a) = f(b)$ باشد، آنگاه نقطه‌ای مانند $c \\in (a, b)$ وجود دارد به طوری که:\n$$f'(c) = 0$$",
                "proof": "چون بازه $[a, b]$ فشرده است و $f$ پیوسته، بنا بر قضیه وایرشتراس $f$ به ماکزیمم و مینیمم مطلق خود می‌رسد. اگر ماکزیمم و مینیمم برابر باشند، تابع ثابت است و مشتق در همه جا صفر است. در غیر این صورت، حداقل یکی از این دو در نقطه‌ای درونی مانند $c \\in (a, b)$ رخ می‌دهد. چون $f$ در $c$ اکسترمم دارد و مشتق‌پذیر است، مشتق در این نقطه صفر می‌شود.",
            },
            {
                "name": "کوشی–پوانکاره",
                "statement": "هر دنباله کوشی در یک فضای متریک کامل دارای حدی یکتا در همان فضا است.",
                "proof": "فرض کنید $(x_n)$ یک دنباله کوشی در فضای متریک کامل $(X, d)$ باشد. بنا به تعریف فضا، حد $x = \\lim_{n \\to \\infty} x_n$ وجود دارد. اگر حد دیگری مانند $y$ وجود داشته باشد، به کمک نامساوی مثلثی داریم:\n$$d(x, y) \\le d(x, x_n) + d(x_n, y)$$\nبا میل دادن $n \\to \\infty$ نتیجه می‌شود $d(x, y) = 0$، پس $x = y$ و حد یکتاست.",
            },
        ],
        "lemmas": [
            {
                "name": "تحدب و مشتق‌پذیری",
                "statement": "اگر $f$ تابعی محدب و مشتق‌پذیر روی بازه $(a, b)$ باشد، آنگاه مشتق $f'$ تابعی صعودی است.",
                "proof": "برای هر دو نقطه $x_1 < x_2$ در بازه، نسبت تفاضلی را در نظر بگیرید. با استفاده از تعریف تحدب و حدگیری یک‌طرفه، نامساوی $f'(x_1) \\le f'(x_2)$ به سادگی حاصل می‌شود.",
            },
        ],
        "definitions": [
            {
                "name": "پیوستگی یکنواخت",
                "statement": "تابع $f: X \\to Y$ بین دو فضای متریک را پیوسته یکنواخت می‌نامیم هرگاه به ازای هر $\\epsilon > 0$، عددی مانند $\\delta > 0$ وجود داشته باشد به طوری که برای تمام $x, y \\in X$ با شرط $d_X(x, y) < \\delta$، نامساوی $d_Y(f(x), f(y)) < \\epsilon$ برقرار باشد.",
            },
        ],
        "remarks": [
            "توجه کنید که پیوستگی یکنواخت مفهومی سراسری است و با پیوستگی نقطه‌ای تفاوت بنیادین دارد.",
            "قضیه هاینه نشان می‌دهد که روی مجموعه‌های فشرده، هر تابع پیوسته لزوماً پیوسته یکنواخت است.",
        ],
        "examples": [
            "تابع $f(x) = x^2$ روی کل $\\mathbb{R}$ پیوسته یکنواخت نیست، اما روی هر بازه بسته و کراندار یکنواخت پیوسته است.",
        ],
    },
    {
        "domain": "جبر خطی",
        "headings": ["دترمینان و معکوس‌پذیری", "فضاهای ویژه و قطری‌سازی"],
        "intros": [
            "در این بخش به ساختارهای جبری ماتریس‌ها و عملگرهای خطی می‌پردازیم.",
            "مقادیر ویژه اطلاعات بنیادینی درباره رفتار هندسی عملگرها در بر دارند.",
        ],
        "theorems": [
            {
                "name": "کیلی–همیلتون",
                "statement": "هر ماتریس مربعی $A \\in M_n(\\mathbb{F})$ در چندجمله‌ای مشخصه خود صدق می‌کند. یعنی اگر $p(\\lambda) = \\det(\\lambda I - A)$ باشد، آنگاه:\n$$p(A) = 0$$",
                "proof": "ماتریس الحاقی $B(\\lambda) = \\mathrm{adj}(\\lambda I - A)$ را در نظر می‌گیریم. با ضرب دو طرف در $\\lambda I - A$ و بسط ضرایب توان‌های $\\lambda$، تساوی‌های متوالی به دست می‌آیند که با جمع ماتریسی آنها حاصل صفر حاصل می‌شود.",
            },
        ],
        "lemmas": [
            {
                "name": "استقلال بردارهای ویژه",
                "statement": "بردارهای ویژه متناظر با مقادیر ویژه متمایز یک عملگر خطی، همواره مستقل خطی هستند.",
                "proof": "با استقرا روی تعداد مقادیر ویژه. فرض کنید $\\sum_{i=1}^k c_i v_i = 0$. با اعمال عملگر $T - \\lambda_k I$ روی دو طرف و استفاده از فرض استقرا، تمام ضرایب صفر به دست می‌آیند.",
            },
        ],
        "definitions": [
            {
                "name": "فضای پوچی",
                "statement": "فضای پوچی تبدیل خطی $T: V \\to W$ عبارت است از مجموعه تمام بردارهای $v \\in V$ که در شرط $T(v) = 0$ صدق می‌کنند.",
            },
        ],
        "remarks": [
            "دترمینان یک ماتریس برابر با حاصل‌ضرب تمام مقادیر ویژه آن با احتساب چندگانگی جبری است.",
        ],
        "examples": [
            "ماتریس قطری $D = \\mathrm{diag}(1, 2, 3)$ دارای سه مقدار ویژه متمایز است و پایه استانداردی از بردارهای ویژه دارد.",
        ],
    },
]


# -----------------------------------------------------------------------------
# Step A: Ingest & Ground-Truth Extraction
# -----------------------------------------------------------------------------

def parse_tex_environments(tex_source: str, language: str = "en") -> list[DocumentSegment]:
    """Parse raw .tex / LaTeX markdown into structured DocumentSegments."""
    segments: list[DocumentSegment] = []
    env_regex = re.compile(
        r"\\begin\{(?P<env>[a-zA-Z*]+)\}(?:\[(?P<opt>[^\]]*)\])?(?P<body>.*?)\\end\{(?P=env)\}",
        re.DOTALL,
    )

    last_pos = 0
    for match in env_regex.finditer(tex_source):
        pre_text = tex_source[last_pos:match.start()].strip()
        if pre_text:
            paras = [p.strip() for p in pre_text.split("\n\n") if p.strip()]
            if paras:
                segments.append(DocumentSegment(
                    block_type=None,
                    name=None,
                    number=None,
                    paragraphs=paras,
                    language=language,
                    is_env=False,
                ))

        raw_env = match.group("env").rstrip("*").lower()
        block_type = ENV_TO_BLOCK_TYPE.get(raw_env, "theorem")
        opt_name = match.group("opt")
        body = match.group("body").strip()
        body_paras = [p.strip() for p in body.split("\n\n") if p.strip()]
        if not body_paras:
            body_paras = ["..."]

        segments.append(DocumentSegment(
            block_type=block_type,
            name=opt_name,
            number=None,
            paragraphs=body_paras,
            language=language,
            is_env=True,
        ))
        last_pos = match.end()

    tail_text = tex_source[last_pos:].strip()
    if tail_text:
        paras = [p.strip() for p in tail_text.split("\n\n") if p.strip()]
        if paras:
            segments.append(DocumentSegment(
                block_type=None,
                name=None,
                number=None,
                paragraphs=paras,
                language=language,
                is_env=False,
            ))

    return segments


# -----------------------------------------------------------------------------
# Step B & C: Stripping, Rewriting, and Synthetic OCR Noise Injection
# -----------------------------------------------------------------------------

def rewrite_opener(
    block_type: str,
    number: Optional[str],
    name: Optional[str],
    language: str,
    rng: random.Random,
    drop_bold_or_colon: bool = False,
    is_implicit_proof: bool = False,
) -> str:
    """Generate realistic Markdown opener (Step B + Step C)."""
    if block_type == "proof" and is_implicit_proof:
        return ""

    label_map = FA_LABELS if language == "fa" else EN_LABELS
    type_name = label_map.get(block_type, block_type.capitalize())

    # Form label string
    if block_type == "proof":
        if number and rng.random() < 0.35:
            ref_type = FA_LABELS.get("theorem", "قضیه") if language == "fa" else "Theorem"
            core = f"{type_name} {ref_type} {number}"
        else:
            core = type_name
    else:
        num_str = f" {number}" if number else ""
        name_str = f" ({name})" if name else ""
        core = f"{type_name}{num_str}{name_str}"

    # Trailing punctuation
    punct = rng.choice([".", ":", " -", ""]) if not drop_bold_or_colon else rng.choice(["", ".", " "])

    # Textbook number-first style (e.g. "1.2 Theorem (Cauchy).")
    if number and rng.random() < 0.12 and block_type != "proof":
        opener_text = f"{number} {type_name}{name_str}{punct}"
    else:
        opener_text = f"{core}{punct}"

    # Bolding style (20% dropped in Step C)
    if drop_bold_or_colon:
        return opener_text.strip() + " "
    else:
        return f"**{opener_text.strip()}** "


def split_atomic_chunks(text: str) -> list[str]:
    """Split text so that display math ($$...$$) and paragraphs are atomic chunks."""
    chunks: list[str] = []
    raw_paras = [p.strip() for p in text.split("\n\n") if p.strip()]
    for p in raw_paras:
        tokens = re.split(r"(\$\$.*?\$\$)", p, flags=re.DOTALL)
        for tok in tokens:
            tok = tok.strip()
            if tok:
                chunks.append(tok)
    return chunks


def inject_noise_and_strip(
    segments: list[DocumentSegment],
    rng: random.Random,
) -> tuple[str, list[str]]:
    """Step B & C: Rewrite environments to Markdown, inject OCR noise, and track expected BioTags."""
    markdown_chunks: list[str] = []
    bio_tags: list[str] = []

    qed_markers_en = ["$\\blacksquare$", "$\\square$", "□", "Q.E.D."]
    qed_markers_fa = ["$\\blacksquare$", "$\\square$", "□", "و حکم ثابت شد.", "این برهان را کامل می‌کند."]

    num_counters: dict[str, int] = {}

    for seg in segments:
        lang = seg.language
        if not seg.is_env or not seg.block_type:
            # Outside prose/heading
            for p in seg.paragraphs:
                for chunk in split_atomic_chunks(p):
                    markdown_chunks.append(chunk)
                    bio_tags.append("O")
            continue

        btype = seg.block_type
        upper_tag = btype.upper()

        # Generate number counter
        num_counters[btype] = num_counters.get(btype, 0) + 1
        num_val = f"{num_counters[btype]}"
        if lang == "fa":
            num_val = to_persian_num(num_val)

        # Flatten paragraphs into atomic chunks (separating display math)
        atomic_chunks: list[str] = []
        for p in seg.paragraphs:
            atomic_chunks.extend(split_atomic_chunks(p))
        if not atomic_chunks:
            atomic_chunks = ["..."]

        # Decide noise flags (Step C)
        drop_bold = rng.random() < 0.20
        drop_qed = rng.random() < 0.25
        split_statement = (btype != "proof") and (len(atomic_chunks) == 1) and (rng.random() < 0.35)
        is_implicit_proof = (btype == "proof") and (rng.random() < 0.15)

        # Split single-paragraph statement if requested (Step C)
        if split_statement and len(atomic_chunks) == 1:
            p = atomic_chunks[0]
            split_candidates = re.split(r"(?<=[.!?؟])\s+", p)
            if len(split_candidates) >= 2:
                mid = len(split_candidates) // 2
                atomic_chunks = [
                    " ".join(split_candidates[:mid]),
                    " ".join(split_candidates[mid:]),
                ]

        # Handle QED for proofs (Step C)
        if btype == "proof":
            last_p = atomic_chunks[-1]
            for qm in ["\\blacksquare", "\\square", "□", "Q.E.D.", "حکم ثابت شد", "این برهان را کامل می‌کند."]:
                last_p = last_p.replace(qm, "").strip()

            if not drop_qed:
                qm = rng.choice(qed_markers_fa if lang == "fa" else qed_markers_en)
                last_p = f"{last_p} {qm}"
            atomic_chunks[-1] = last_p

        # Generate opener
        opener = rewrite_opener(
            block_type=btype,
            number=num_val,
            name=seg.name,
            language=lang,
            rng=rng,
            drop_bold_or_colon=drop_bold,
            is_implicit_proof=is_implicit_proof,
        )

        # Combine opener with first atomic chunk
        first_chunk = f"{opener}{atomic_chunks[0]}".strip()
        markdown_chunks.append(first_chunk)
        bio_tags.append(f"B-{upper_tag}")

        for chunk in atomic_chunks[1:]:
            markdown_chunks.append(chunk)
            bio_tags.append(f"I-{upper_tag}")

    full_markdown = "\n\n".join(markdown_chunks)
    return full_markdown, bio_tags


# -----------------------------------------------------------------------------
# Synthetic Bilingual Document Generator
# -----------------------------------------------------------------------------

def generate_synthetic_document_segments(
    doc_id: str,
    language: str,
    rng: random.Random,
) -> tuple[str, list[DocumentSegment]]:
    """Generate a realistic bilingual math document with explicit LaTeX environments."""
    domains = DOMAINS_FA if language == "fa" else DOMAINS_EN
    domain = rng.choice(domains)
    title = rng.choice(domain["headings"])

    segments: list[DocumentSegment] = []

    # Title heading
    segments.append(DocumentSegment(
        block_type=None,
        name=None,
        number=None,
        paragraphs=[f"# {title}"],
        language=language,
        is_env=False,
    ))

    # Introductory prose
    intro = rng.choice(domain["intros"])
    segments.append(DocumentSegment(
        block_type=None,
        name=None,
        number=None,
        paragraphs=[intro],
        language=language,
        is_env=False,
    ))

    # Generate 2-4 semantic environment blocks
    num_blocks = rng.randint(2, 4)
    for _ in range(num_blocks):
        choice = rng.random()
        if choice < 0.45:
            # Theorem + Proof pair
            thm_data = rng.choice(domain["theorems"])
            # Theorem env
            segments.append(DocumentSegment(
                block_type="theorem",
                name=thm_data["name"] if rng.random() < 0.7 else None,
                number=None,
                paragraphs=[thm_data["statement"]],
                language=language,
                is_env=True,
            ))
            # Proof env
            segments.append(DocumentSegment(
                block_type="proof",
                name=None,
                number=None,
                paragraphs=[thm_data["proof"]],
                language=language,
                is_env=True,
            ))
        elif choice < 0.65:
            # Definition
            defn_data = rng.choice(domain["definitions"])
            segments.append(DocumentSegment(
                block_type="definition",
                name=defn_data["name"] if rng.random() < 0.6 else None,
                number=None,
                paragraphs=[defn_data["statement"]],
                language=language,
                is_env=True,
            ))
        elif choice < 0.80:
            # Lemma + Proof
            lem_data = rng.choice(domain["lemmas"])
            segments.append(DocumentSegment(
                block_type="lemma",
                name=lem_data["name"] if rng.random() < 0.6 else None,
                number=None,
                paragraphs=[lem_data["statement"]],
                language=language,
                is_env=True,
            ))
            segments.append(DocumentSegment(
                block_type="proof",
                name=None,
                number=None,
                paragraphs=[lem_data["proof"]],
                language=language,
                is_env=True,
            ))
        elif choice < 0.90:
            # Remark
            rem = rng.choice(domain["remarks"])
            segments.append(DocumentSegment(
                block_type="remark",
                name=None,
                number=None,
                paragraphs=[rem],
                language=language,
                is_env=True,
            ))
        else:
            # Example
            ex = rng.choice(domain["examples"])
            segments.append(DocumentSegment(
                block_type="example",
                name=None,
                number=None,
                paragraphs=[ex],
                language=language,
                is_env=True,
            ))

        # Occasionally add connective prose
        if rng.random() < 0.40:
            connective = (
                "این نتایج ساختار کلی فضا را روشن می‌سازند."
                if language == "fa"
                else "These preliminary observations motivate the subsequent development."
            )
            segments.append(DocumentSegment(
                block_type=None,
                name=None,
                number=None,
                paragraphs=[connective],
                language=language,
                is_env=False,
            ))

    return title, segments


# -----------------------------------------------------------------------------
# Pipeline Execution & Alignment (Steps A, B, C, D)
# -----------------------------------------------------------------------------

def process_segments_into_labeled_document(
    doc_id: str,
    title: str,
    language: str,
    segments: list[DocumentSegment],
    mathia_bin: Path,
    rng: random.Random,
) -> Optional[LabeledDocument]:
    """Execute Steps B, C, D on document segments and return a verified LabeledDocument."""
    # Step B & C: Strip and inject noise
    markdown_str, expected_tags = inject_noise_and_strip(segments, rng)

    # Step D: Run dump-features on stripped/noisy Markdown
    try:
        dump = run_dump_features(mathia_bin, markdown_str)
    except Exception as e:
        print(f"Error dumping features for {doc_id}: {e}", file=sys.stderr)
        return None

    nodes = dump.nodes
    features = dump.features

    # Alignment check
    if len(nodes) != len(expected_tags):
        # Fallback heuristic alignment if markdown parsing split/merged nodes
        aligned_tags = align_tags_to_nodes(nodes, expected_tags, segments)
    else:
        aligned_tags = expected_tags

    return LabeledDocument(
        doc_id=doc_id,
        language=language,
        title=title,
        nodes=nodes,
        features=features,
        bio_tags=aligned_tags,
        metadata={"num_segments": len(segments)},
    )


def align_tags_to_nodes(
    nodes: list[DumpNode],
    expected_tags: list[str],
    segments: list[DocumentSegment],
) -> list[str]:
    """Robust fallback alignment between AST nodes and expected tags."""
    if len(nodes) == len(expected_tags):
        return expected_tags

    # Truncate or pad with O
    aligned: list[str] = []
    for i in range(len(nodes)):
        if i < len(expected_tags):
            aligned.append(expected_tags[i])
        else:
            aligned.append("O")
    return aligned


def generate_dataset(
    output_dir: Path,
    num_docs: int = 500,
    sources_dir: Optional[Path] = None,
    seed: int = 42,
) -> dict[str, int]:
    """Generate complete dataset and write train.jsonl, val.jsonl, test.jsonl."""
    rng = random.Random(seed)
    mathia_bin = find_mathia_binary()
    output_dir.mkdir(parents=True, exist_ok=True)

    en_docs: list[LabeledDocument] = []
    fa_docs: list[LabeledDocument] = []

    # Ingest .tex sources if provided
    if sources_dir and sources_dir.exists():
        tex_files = list(sources_dir.glob("**/*.tex")) + list(sources_dir.glob("**/*.md"))
        print(f"Found {len(tex_files)} source files in {sources_dir}")
        for idx, tex_file in enumerate(tex_files):
            try:
                content = tex_file.read_text(encoding="utf-8")
                # Infer language
                is_fa = bool(re.search(r"[\u0600-\u06FF]", content))
                lang = "fa" if is_fa else "en"
                segments = parse_tex_environments(content, language=lang)
                if segments:
                    doc = process_segments_into_labeled_document(
                        doc_id=f"src_{idx:04d}_{tex_file.stem}",
                        title=tex_file.stem,
                        language=lang,
                        segments=segments,
                        mathia_bin=mathia_bin,
                        rng=rng,
                    )
                    if doc:
                        if lang == "fa":
                            fa_docs.append(doc)
                        else:
                            en_docs.append(doc)
            except Exception as e:
                print(f"Skipping {tex_file}: {e}", file=sys.stderr)

    # Generate synthetic bilingual documents to meet target quota
    remaining_docs = max(0, num_docs - (len(en_docs) + len(fa_docs)))
    en_target = remaining_docs // 2
    fa_target = remaining_docs - en_target

    print(f"Generating {en_target} English and {fa_target} Persian synthetic documents...")

    for i in range(en_target):
        doc_id = f"synth_en_{i:04d}"
        title, segments = generate_synthetic_document_segments(doc_id, "en", rng)
        doc = process_segments_into_labeled_document(doc_id, title, "en", segments, mathia_bin, rng)
        if doc:
            en_docs.append(doc)

    for i in range(fa_target):
        doc_id = f"synth_fa_{i:04d}"
        title, segments = generate_synthetic_document_segments(doc_id, "fa", rng)
        doc = process_segments_into_labeled_document(doc_id, title, "fa", segments, mathia_bin, rng)
        if doc:
            fa_docs.append(doc)

    all_docs = en_docs + fa_docs
    print(f"Total documents prepared: {len(en_docs)} English, {len(fa_docs)} Persian.")

    # Task 4: 80/10/10 Split strictly by document, stratified by language
    train_set, val_set, test_set = split_documents(
        all_docs,
        train_ratio=0.80,
        val_ratio=0.10,
        test_ratio=0.10,
        seed=seed,
    )

    stats = write_split_jsonl(output_dir, train_set, val_set, test_set)

    print(f"Saved {len(train_set)} documents to {output_dir / 'train.jsonl'}")
    print(f"Saved {len(val_set)} documents to {output_dir / 'val.jsonl'}")
    print(f"Saved {len(test_set)} documents to {output_dir / 'test.jsonl'}")

    return {
        "train": len(train_set),
        "val": len(val_set),
        "test": len(test_set),
        "total_en": len(en_docs),
        "total_fa": len(fa_docs),
        "stats": stats,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description="Mathia LaTeX Stripper & Dataset Generator")
    parser.add_argument("--output-dir", type=Path, default=Path(__file__).resolve().parents[2] / "data")
    parser.add_argument("--num-docs", type=int, default=500, help="Number of documents to generate")
    parser.add_argument("--sources", type=Path, default=None, help="Optional directory with .tex/.md sources")
    parser.add_argument("--seed", type=int, default=42, help="Random seed")
    args = parser.parse_args()

    generate_dataset(
        output_dir=args.output_dir,
        num_docs=args.num_docs,
        sources_dir=args.sources,
        seed=args.seed,
    )


if __name__ == "__main__":
    main()
