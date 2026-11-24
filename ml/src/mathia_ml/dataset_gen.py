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
import logging
import os
import random
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable, Optional

logger = logging.getLogger(__name__)

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
    mathia_bin: Path,
    content: str,
    overrides_path: Optional[Path] = None,
    suffix: str = ".md",
    config_path: Optional[Path] = None,
) -> FeatureDump:
    """Run `mathia dump-features` on string content and return parsed JSON."""
    with tempfile.NamedTemporaryFile("w", suffix=suffix, delete=False, encoding="utf-8") as f:
        f.write(content)
        tmp_path = f.name
    try:
        cmd = [str(mathia_bin), "dump-features", tmp_path]
        if config_path is not None:
            cmd.extend(["--config", str(config_path)])
        if overrides_path is not None:
            cmd.extend(["--overrides", str(overrides_path)])
        proc = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
        )
        if proc.returncode != 0:
            detail = proc.stderr.strip() or proc.stdout.strip() or "no output"
            raise RuntimeError(
                f"mathia dump-features failed (exit {proc.returncode}): {detail}"
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

DOMAINS_EN_TRAIN = [
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
        "propositions": [
            {
                "name": "Invariance of Eigenspaces",
                "statement": "Let $T: V \\to V$ be a linear operator and let $E_\\lambda$ be the eigenspace of $\\lambda$. If $S$ commutes with $T$, then $E_\\lambda$ is $S$-invariant.",
                "proof": "For any $v \\in E_\\lambda$, $T(S(v)) = S(T(v)) = S(\\lambda v) = \\lambda S(v)$, which shows $S(v) \\in E_\\lambda$.",
            },
        ],
        "corollaries": [
            {
                "name": "Simultaneous Diagonalizability",
                "statement": "Any commuting family of diagonalizable normal operators on $V$ admits a common orthonormal eigenbasis.",
                "proof": "Follows by induction on dimension by decomposing $V$ into the eigenspaces of the first operator.",
            },
        ],
        "exercises": [
            {
                "name": "Nilpotent Operator Image",
                "statement": "Let $T: V \\to V$ satisfy $T^2 = 0$. Prove that $\\mathrm{im}(T) \\subseteq \\ker(T)$.",
                "solution": "If $w \\in \\mathrm{im}(T)$, then $w = T(v)$. Thus $T(w) = T^2(v) = 0$, so $w \\in \\ker(T)$.",
            },
        ],
        "claims": [
            {
                "name": "Non-negative Spectrum",
                "statement": "Every eigenvalue of $T^* T$ is a non-negative real number.",
                "proof": "If $T^* T v = \\lambda v$ with $v \\neq 0$, then $\\lambda \\|v\\|^2 = \\langle T^* T v, v \\rangle = \\|Tv\\|^2 \\ge 0$, so $\\lambda \\ge 0$.",
            },
        ],
        "notations": [
            "We denote by $\\mathcal{L}(V, W)$ the space of linear transformations from $V$ to $W$.",
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
        "propositions": [
            {
                "name": "Limit of Continuous Functions",
                "statement": "If a sequence of continuous functions $(f_n)$ converges uniformly to $f$ on $X$, then $f$ is continuous.",
                "proof": "For any $\\epsilon > 0$, choose $N$ such that $|f_N(x) - f(x)| < \\epsilon/3$. By continuity of $f_N$, pick $\\delta > 0$. The triangle inequality completes the proof.",
            },
        ],
        "corollaries": [
            {
                "name": "Bounded Dominated Convergence",
                "statement": "Let $|f_n(x)| \\le M$ on a set of finite measure $E$ with $f_n \\to f$ a.e. Then $\\int_E f_n \\to \\int_E f$.",
                "proof": "Special case of the Lebesgue Dominated Convergence Theorem with constant dominating function $g(x) = M$.",
            },
        ],
        "exercises": [
            {
                "name": "Separability of Compact Space",
                "statement": "Prove that every compact metric space $(X, d)$ is separable.",
                "solution": "For each $n \\in \\mathbb{N}$, cover $X$ by open balls of radius $1/n$. By compactness, select a finite subcover. The union of all centers is countable and dense.",
            },
        ],
        "claims": [
            {
                "name": "Closure Inclusion",
                "statement": "In any metric space $(X, d)$, the closure $\\overline{B(x, r)}$ is contained in the closed ball $\\bar{B}(x, r)$.",
                "proof": "If $y \\in \\overline{B(x, r)}$, there is a sequence $y_k \\in B(x, r)$ with $y_k \\to y$. Since $d(x, y_k) < r$, continuity yields $d(x, y) \\le r$.",
            },
        ],
        "notations": [
            "Let $\\mathcal{B}(X)$ denote the Borel $\\sigma$-algebra generated by the open subsets of $X$.",
        ],
    },
]

DOMAINS_FA_TRAIN = [
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
        "propositions": [
            {
                "name": "فشردگی و کرانداری",
                "statement": "هر زیرمجموعه فشرده از یک فضای متریک، بسته و کراندار است.",
                "proof": "با استفاده از پوشش باز متشکل از گوی‌های به شعاع ۱ حول یک نقطه ثابت و استفاده از زیرپوشش متناهی، کرانداری حاصل می‌شود.",
            },
        ],
        "corollaries": [
            {
                "name": "پیوستگی یکنواخت نگاشت‌های فشرده",
                "statement": "هر تابع پیوسته از یک فضای متریک فشرده به فضای متریک دلخواه، پیوسته یکنواخت است.",
                "proof": "قضیه کانتور با پوشاندن دامنه با گوی‌های به شعاع نصف دلتا این نتیجه را بلافاصله نتیجه می‌دهد.",
            },
        ],
        "exercises": [
            {
                "name": "تفکیک‌پذیری فضا",
                "statement": "ثابت کنید هر فضای متریک فشرده دارای زیرمجموعه‌ای شمارا و چگال است.",
                "solution": "به ازای هر عدد طبیعی $n$، پوششی از گوی‌های به شعاع $1/n$ انتخاب کرده و زیرپوشش متناهی می‌گیریم. اجتماع مراکز این گوی‌ها مجموعه‌ای شمارا و چگال است.",
            },
        ],
        "claims": [
            {
                "name": "بستار گوی باز",
                "statement": "در هر فضای متریک $(X, d)$، بستار گوی باز $B(x, r)$ زیرمجموعه گوی بسته است.",
                "proof": "اگر دنباله‌ای در گوی باز به نقطه $y$ همگرا شود، به دلیل پیوستگی تابع فاصله، $d(x, y) \\le r$ برقرار خواهد بود.",
            },
        ],
        "notations": [
            "مجموعه اعداد حقیقی را با $\\mathbb{R}$ و اندازه لبگ را با $m$ نشان می‌دهیم.",
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
        "propositions": [
            {
                "name": "ماتریس‌های متقارن حقیقی",
                "statement": "تمام مقادیر ویژه یک ماتریس متقارن با درایه‌های حقیقی، اعدادی حقیقی هستند.",
                "proof": "با ضرب رابطه $A v = \\lambda v$ در مزدوج ترانهاده $v^*$ و استفاده از تقارن $A = A^T$ به سادگی $\\lambda = \\bar{\\lambda}$ نتیجه می‌شود.",
            },
        ],
        "corollaries": [
            {
                "name": "قطری‌پذیری با مقادیر متمایز",
                "statement": "هر ماتریس $n \\times n$ با $n$ مقدار ویژه متمایز، قطری‌پذیر است.",
                "proof": "چون بردارهای ویژه متناظر با مقادیر ویژه متمایز مستقل خطی‌اند، پایه‌ای برای فضا تشکیل می‌دهند.",
            },
        ],
        "exercises": [
            {
                "name": "محاسبه چندجمله‌ای مشخصه",
                "statement": "چندجمله‌ای مشخصه ماتریس پادمتقارن مرتبه فرد را محاسبه کرده و نشان دهید دترمینان آن صفر است.",
                "solution": "داریم $\\det(A) = \\det(A^T) = \\det(-A) = (-1)^n \\det(A)$. چون $n$ فرد است، $\\det(A) = -\\det(A)$ و در نتیجه $\\det(A) = 0$.",
            },
        ],
        "claims": [
            {
                "name": "ارتباط رتبه و تصویر",
                "statement": "رتبه هر ماتریس برابر با بعد فضای ستونی آن است.",
                "proof": "تصویر تبدیل خطی متناظر دقیقاً توسط ستون‌های ماتریس تولید می‌شود.",
            },
        ],
        "notations": [
            "فضای تمام ماتریس‌های $n \\times n$ روی میدان $F$ را با $M_n(F)$ نمایش می‌دهیم.",
        ],
    },
]

DOMAINS_EN_EVAL = [
    {
        "domain": "Abstract Algebra",
        "headings": ["Group Homomorphisms and Normal Subgroups", "Ideals and Quotient Rings"],
        "intros": [
            "We investigate quotient structures and isomorphism theorems in abstract algebra.",
            "Galois theory and the study of rings provide foundational algebraic tools.",
        ],
        "theorems": [
            {
                "name": "First Isomorphism Theorem",
                "statement": "Let $\\phi: G \\to H$ be a group homomorphism. Then $\\ker(\\phi) \\triangleleft G$ and $G/\\ker(\\phi) \\cong \\mathrm{im}(\\phi)$.",
                "proof": "Define $\\bar{\\phi}(g \\ker \\phi) = \\phi(g)$. Well-definedness and injectivity follow directly from coset properties.",
            },
            {
                "name": "Lagrange's Theorem",
                "statement": "For any finite group $G$ and subgroup $H \\le G$, the order of $H$ divides the order of $G$.",
                "proof": "The left cosets of $H$ partition $G$, and each coset has cardinality $|H|$. Thus $|G| = [G : H] |H|$.",
            },
        ],
        "lemmas": [
            {
                "name": "Gauss's Lemma",
                "statement": "The product of two primitive polynomials over a unique factorization domain is primitive.",
                "proof": "Reducing modulo any prime element gives a product of non-zero elements in an integral domain, which cannot vanish.",
            },
        ],
        "propositions": [
            {
                "name": "Maximal Ideals Characterization",
                "statement": "Let $R$ be a commutative ring with identity. An ideal $M$ is maximal if and only if $R/M$ is a field.",
                "proof": "The ideals of $R/M$ correspond to ideals of $R$ containing $M$. Thus $M$ is maximal iff $R/M$ has only trivial ideals.",
            },
        ],
        "corollaries": [
            {
                "name": "Element Order Divisibility",
                "statement": "For every element $g$ in a finite group $G$, the order of $g$ divides $|G|$.",
                "proof": "Apply Lagrange's theorem to the cyclic subgroup generated by $g$.",
            },
        ],
        "definitions": [
            {
                "name": "Normal Subgroup",
                "statement": "A subgroup $N \\le G$ is normal if $g n g^{-1} \\in N$ for all $g \\in G$ and $n \\in N$.",
            },
            {
                "name": "Principal Ideal Domain",
                "statement": "An integral domain $R$ is a principal ideal domain if every ideal in $R$ is principal.",
            },
        ],
        "remarks": [
            "Every subgroup of index 2 in a group is necessarily normal.",
            "A field has no proper non-trivial ideals.",
        ],
        "examples": [
            "The quaternion group $Q_8$ is non-abelian, but every subgroup of $Q_8$ is normal.",
        ],
        "exercises": [
            {
                "name": "Cyclic Homomorphisms",
                "statement": "Find the number of group homomorphisms from $\\mathbb{Z}/12\\mathbb{Z}$ to $\\mathbb{Z}/4\\mathbb{Z}$.",
                "solution": "A homomorphism is determined by the image of 1, which must satisfy $12 \\phi(1) = 0$. Since all elements satisfy this, there are 4 homomorphisms.",
            },
        ],
        "claims": [
            {
                "name": "Center Normality",
                "statement": "The center $Z(G)$ of any group $G$ is a normal subgroup.",
                "proof": "For any $z \\in Z(G)$ and $g \\in G$, $g z g^{-1} = z g g^{-1} = z \\in Z(G)$.",
            },
        ],
        "notations": [
            "For a subgroup $H \\le G$, $[G : H]$ denotes the index of $H$ in $G$.",
        ],
    },
]

DOMAINS_FA_EVAL = [
    {
        "domain": "جبر مجرد و توپولوژی",
        "headings": ["هم‌ریختی‌های گروهی و زیرگروه‌های نرمال", "فضاهای توپولوژیک و همبندی"],
        "intros": [
            "در این بخش به ساختارهای جبری گروه‌ها، حلقه‌ها و ویژگی‌های فضاهای توپولوژیک می‌پردازیم.",
            "بررسی هم‌ریختی‌ها درک عمیقی از تقارن‌های ریاضیاتی به دست می‌دهد.",
        ],
        "theorems": [
            {
                "name": "قضیه اول ایزومورفیسم",
                "statement": "اگر $\\phi: G \\to H$ یک هم‌ریختی گروهی باشد، آنگاه هسته $\\ker(\\phi)$ زیرگروهی نرمال از $G$ است و $G/\\ker(\\phi) \\cong \\mathrm{im}(\\phi)$.",
                "proof": "نگاشت القایی روی هم‌دسته‌ها خوش‌تعریف، یک‌به‌یک و پوشا است و ساختار گروهی را حفظ می‌کند.",
            },
            {
                "name": "قضیه لاگرانژ",
                "statement": "مرتبه هر زیرگروه از یک گروه متناهی، مرتبه گروه را عاد می‌کند.",
                "proof": "هم‌دسته‌های چپ یک افراز روی فضا تشکیل می‌دهند که هر یک دارای کاردینالیتی برابر با زیرگروه هستند.",
            },
        ],
        "lemmas": [
            {
                "name": "لم زاسن‌هاوس",
                "statement": "اگر $A$ و $B$ زیرگروه‌هایی از $G$ باشند، ساختارهای خارج‌قسمتی متناظر با آن‌ها ایزومورف هستند.",
                "proof": "بررسی نگاشت‌های کانونیکال و استفاده از قضایای ایزومورفیسم اثبات را کامل می‌کند.",
            },
        ],
        "propositions": [
            {
                "name": "ایده‌آل‌های ماکزیمال",
                "statement": "در یک حلقه جابه‌جایی یک‌دار، ایده‌آل $M$ ماکزیمال است اگر و تنها اگر حلقه خارج‌قسمتی $R/M$ یک میدان باشد.",
                "proof": "ایده‌آل‌های حلقه خارج‌قسمتی متناظر با ایده‌آل‌های دربرگیرنده هستند و میدان‌ها تنها ایده‌آل‌های بدیهی دارند.",
            },
        ],
        "corollaries": [
            {
                "name": "مرتبه عناصر در گروه متناهی",
                "statement": "مرتبه هر عنصر در یک گروه متناهی مرتبه کل گروه را عاد می‌کند.",
                "proof": "این نتیجه مستقیماً از اعمال قضیه لاگرانژ به زیرگروه دوری تولیدشده حاصل می‌شود.",
            },
        ],
        "definitions": [
            {
                "name": "زیرگروه نرمال",
                "statement": "زیرگروه $N$ از $G$ را نرمال می‌نامیم هرگاه به ازای هر $g \\in G$ داشته باشیم $g N g^{-1} = N$.",
            },
            {
                "name": "فضای هاوسدورف",
                "statement": "فضای توپولوژیک $X$ را هاوسدورف می‌نامیم هرگاه هر دو نقطه متمایز را بتوان با مجموعه‌های باز مجزا جدا کرد.",
            },
        ],
        "remarks": [
            "هر زیرگروه با اندیس ۲ در یک گروه لزوماً نرمال است.",
            "هر فضای فشرده در یک فضای هاوسدورف بسته است.",
        ],
        "examples": [
            "گروه تقارنی $S_3$ کوچک‌ترین گروه غیرآبلی است و زیرگروه متناوب $A_3$ در آن نرمال است.",
        ],
        "exercises": [
            {
                "name": "هم‌ریختی‌های دوری",
                "statement": "تعداد هم‌ریختی‌های گروهی از $\\mathbb{Z}_6$ به $\\mathbb{Z}_{12}$ را بیابید.",
                "solution": "تصویر مولد باید در معادله $6x = 0$ در $\\mathbb{Z}_{12}$ صدق کند که دقیقاً ۶ پاسخ دارد.",
            },
        ],
        "claims": [
            {
                "name": "نرمال بودن مرکز گروه",
                "statement": "مرکز هر گروه $G$ همواره زیرگروهی نرمال و آبلی است.",
                "proof": "چون عناصر مرکز با تمام عناصر گروه جابه‌جا می‌شوند، تزویج آن‌ها تغییری ایجاد نمی‌کند.",
            },
        ],
        "notations": [
            "نماد $H \\triangleleft G$ نشان‌دهنده نرمال بودن زیرگروه $H$ در $G$ است.",
        ],
    },
]

DOMAINS_EN = DOMAINS_EN_TRAIN
DOMAINS_FA = DOMAINS_FA_TRAIN


def get_domains(language: str, split: str = "train") -> list[dict[str, Any]]:
    """Return template domains, ensuring strict disjointness between train and eval splits."""
    if language == "fa":
        return DOMAINS_FA_TRAIN if split == "train" else DOMAINS_FA_EVAL
    else:
        return DOMAINS_EN_TRAIN if split == "train" else DOMAINS_EN_EVAL


# -----------------------------------------------------------------------------
# Step A: Ingest & Ground-Truth Extraction
# -----------------------------------------------------------------------------

def parse_tex_environments(tex_source: str, language: str = "en") -> list[DocumentSegment]:
    """Parse raw .tex / LaTeX markdown into structured DocumentSegments."""
    segments: list[DocumentSegment] = []
    env_pattern = "|".join(re.escape(k) for k in sorted(ENV_TO_BLOCK_TYPE.keys(), key=len, reverse=True))
    env_regex = re.compile(
        rf"\\begin\{{(?P<env>(?:{env_pattern})\*?)\}}(?:\[(?P<opt>[^\]]*)\])?(?P<body>.*?)\\end\{{(?P=env)\}}",
        re.DOTALL | re.IGNORECASE,
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
        if raw_env not in ENV_TO_BLOCK_TYPE:
            continue
        block_type = ENV_TO_BLOCK_TYPE[raw_env]
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
        # Implicit proofs (no "برهان"/"Proof" opener) are the hard case on real
        # notes. Keep a healthy share so the CRF learns that a math-dense
        # paragraph right after a statement is usually a proof.
        is_implicit_proof = (btype == "proof") and (rng.random() < 0.45)

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

# Math-dense exposition that is *not* a proof. Real compiled notes are full of
# prose paragraphs saturated with inline math that still belong to the narrative
# (label O). Without these the CRF learns "math-dense paragraph == proof" and
# runs away on real documents.
EXPOSITION_EN = [
    "This observation shows that the structure of $V$ is preserved under every linear map, so for all $v \\in V$ we have $T(v) = 0$. It therefore follows that $\\dim V = n$; the argument is purely structural and does not establish the claim.",
    "Suppose $T$ is self-adjoint. Then $V$ decomposes into eigenspaces, and this decomposition is used throughout the chapter. Hence the notation $V = \\bigoplus_\\lambda E_\\lambda$ is convenient, although no proof is given here.",
    "We write $A = (a_{ij})_{m \\times n}$ and note that the entries $a_{ij} \\in \\mathbb{F}$ determine the map completely. In particular $A x = 0$ whenever $x \\in \\ker A$, which motivates the next definition.",
    "For $\\lambda \\in \\mathbb{F}$ the set $E_\\lambda = \\{v \\in V \\mid T(v) = \\lambda v\\}$ is a subspace. This is a useful bookkeeping device: it lets us compare $\\dim E_\\lambda$ with $\\dim V$ without assuming anything about the spectrum.",
    "The reader should keep in mind that $\\langle u, v \\rangle$ is linear in the first slot and conjugate-linear in the second. Consequently $\\|u + v\\|^2 = \\|u\\|^2 + 2\\,\\mathrm{Re}\\langle u, v\\rangle + \\|v\\|^2$, a formula we will reuse repeatedly.",
    "Throughout, $\\mathcal{L}(V, W)$ denotes the space of linear maps and $\\mathrm{im}\\,T \\subseteq W$ its image. Thus $\\dim \\mathrm{im}\\,T \\le \\dim W$, and equality holds exactly when $T$ is surjective.",
]

EXPOSITION_FA = [
    "این نتیجه نشان می‌دهد که ساختار $V$ تحت هر نگاشت خطی پایدار می‌ماند، پس برای هر $v \\in V$ رابطه $T(v) = 0$ برقرار است. بنابراین $\\dim V = n$؛ این استدلال صرفاً ساختاری است و حکم را اثبات نمی‌کند.",
    "فرض کنید $T$ خودالحاقی باشد. در این صورت $V$ به زیرفضاهای ویژه تجزیه می‌شود و از این تجزیه در سراسر فصل استفاده می‌کنیم. از این رو نماد $V = \\bigoplus_\\lambda E_\\lambda$ مناسب است، هرچند در اینجا برهانی ارائه نمی‌شود.",
    "می‌نویسیم $A = (a_{ij})_{m \\times n}$ و توجه می‌کنیم که درایه‌های $a_{ij} \\in \\mathbb{F}$ نگاشت را کاملاً مشخص می‌کنند. به‌ویژه $A x = 0$ هرگاه $x \\in \\ker A$، و همین انگیزه تعریف بعدی است.",
    "برای $\\lambda \\in \\mathbb{F}$ مجموعه $E_\\lambda = \\{v \\in V \\mid T(v) = \\lambda v\\}$ یک زیرفضاست. این یک ابزار ثبت‌کننده سودمند است که امکان مقایسه $\\dim E_\\lambda$ با $\\dim V$ را بدون فرضی درباره طیف فراهم می‌کند.",
    "باید در نظر داشت که $\\langle u, v \\rangle$ در مؤلفه اول خطی و در مؤلفه دوم مزدوج-خطی است. در نتیجه $\\|u + v\\|^2 = \\|u\\|^2 + 2\\,\\mathrm{Re}\\langle u, v\\rangle + \\|v\\|^2$، فرمولی که بارها به کار می‌بریم.",
    "در سراسر متن $\\mathcal{L}(V, W)$ فضای نگاشت‌های خطی و $\\mathrm{im}\\,T \\subseteq W$ تصویر آن است. بنابراین $\\dim \\mathrm{im}\\,T \\le \\dim W$، و تساوی دقیقاً هنگامی برقرار است که $T$ پوشا باشد.",
]


def split_proof_paragraphs(text: str, rng: random.Random) -> list[str]:
    """Split a single-paragraph proof into 2-4 paragraphs at sentence boundaries.

    Multi-paragraph proofs are the norm in real notes. Training only on
    one-paragraph proofs leaves the CRF with no evidence for when an ``I-PROOF``
    run should end, which produces unbounded proof spans at inference time.
    """
    sentences = [s for s in re.split(r"(?<=[.!?؟])\s+", text.strip()) if s]
    if len(sentences) < 2:
        return [text]
    parts = min(rng.randint(2, 4), len(sentences))
    groups: list[list[str]] = [[] for _ in range(parts)]
    for idx, sentence in enumerate(sentences):
        groups[idx % parts].append(sentence)
    return [" ".join(group) for group in groups if group]


def generate_synthetic_document_segments(
    doc_id: str,
    language: str,
    rng: random.Random,
    split: str = "train",
) -> tuple[str, list[DocumentSegment]]:
    """Generate a realistic bilingual math document with explicit LaTeX environments."""
    domains = get_domains(language, split)
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

    exposition_pool = EXPOSITION_FA if language == "fa" else EXPOSITION_EN

    def add_exposition() -> None:
        """Insert math-dense narrative prose (label O) between blocks."""
        segments.append(DocumentSegment(
            block_type=None,
            name=None,
            number=None,
            paragraphs=[rng.choice(exposition_pool)],
            language=language,
            is_env=False,
        ))

    # Generate 3-7 semantic environment blocks covering all 12 BlockTypes
    num_blocks = rng.randint(3, 7)
    for _ in range(num_blocks):
        choice = rng.random()
        if choice < 0.20:
            # Theorem (+ optional proof in 75%)
            thm_data = rng.choice(domain["theorems"])
            segments.append(DocumentSegment(
                block_type="theorem",
                name=thm_data.get("name") if rng.random() < 0.7 else None,
                number=None,
                paragraphs=[thm_data["statement"]],
                language=language,
                is_env=True,
            ))
            if "proof" in thm_data and rng.random() < 0.55:
                segments.append(DocumentSegment(
                    block_type="proof",
                    name=None,
                    number=None,
                    paragraphs=split_proof_paragraphs(thm_data["proof"], rng),
                    language=language,
                    is_env=True,
                ))
            else:
                add_exposition()
        elif choice < 0.32:
            # Proposition (+ optional proof in 70%)
            prop_data = rng.choice(domain["propositions"])
            segments.append(DocumentSegment(
                block_type="proposition",
                name=prop_data.get("name") if rng.random() < 0.6 else None,
                number=None,
                paragraphs=[prop_data["statement"]],
                language=language,
                is_env=True,
            ))
            if "proof" in prop_data and rng.random() < 0.50:
                segments.append(DocumentSegment(
                    block_type="proof",
                    name=None,
                    number=None,
                    paragraphs=split_proof_paragraphs(prop_data["proof"], rng),
                    language=language,
                    is_env=True,
                ))
            else:
                add_exposition()
        elif choice < 0.44:
            # Lemma (+ optional proof in 70%)
            lem_data = rng.choice(domain["lemmas"])
            segments.append(DocumentSegment(
                block_type="lemma",
                name=lem_data.get("name") if rng.random() < 0.6 else None,
                number=None,
                paragraphs=[lem_data["statement"]],
                language=language,
                is_env=True,
            ))
            if "proof" in lem_data and rng.random() < 0.50:
                segments.append(DocumentSegment(
                    block_type="proof",
                    name=None,
                    number=None,
                    paragraphs=split_proof_paragraphs(lem_data["proof"], rng),
                    language=language,
                    is_env=True,
                ))
            else:
                add_exposition()
        elif choice < 0.54:
            # Corollary (+ optional proof in 60%)
            cor_data = rng.choice(domain["corollaries"])
            segments.append(DocumentSegment(
                block_type="corollary",
                name=cor_data.get("name") if rng.random() < 0.5 else None,
                number=None,
                paragraphs=[cor_data["statement"]],
                language=language,
                is_env=True,
            ))
            if "proof" in cor_data and rng.random() < 0.45:
                segments.append(DocumentSegment(
                    block_type="proof",
                    name=None,
                    number=None,
                    paragraphs=split_proof_paragraphs(cor_data["proof"], rng),
                    language=language,
                    is_env=True,
                ))
            else:
                add_exposition()
        elif choice < 0.64:
            # Definition
            defn_data = rng.choice(domain["definitions"])
            segments.append(DocumentSegment(
                block_type="definition",
                name=defn_data.get("name") if rng.random() < 0.6 else None,
                number=None,
                paragraphs=[defn_data["statement"]],
                language=language,
                is_env=True,
            ))
        elif choice < 0.74:
            # Exercise (+ optional solution in 65%)
            ex_data = rng.choice(domain["exercises"])
            segments.append(DocumentSegment(
                block_type="exercise",
                name=ex_data.get("name") if rng.random() < 0.4 else None,
                number=None,
                paragraphs=[ex_data["statement"]],
                language=language,
                is_env=True,
            ))
            if "solution" in ex_data and rng.random() < 0.50:
                segments.append(DocumentSegment(
                    block_type="solution",
                    name=None,
                    number=None,
                    paragraphs=split_proof_paragraphs(ex_data["solution"], rng),
                    language=language,
                    is_env=True,
                ))
            else:
                add_exposition()
        elif choice < 0.82:
            # Claim (+ optional proof in 65%)
            claim_data = rng.choice(domain["claims"])
            segments.append(DocumentSegment(
                block_type="claim",
                name=claim_data.get("name") if rng.random() < 0.4 else None,
                number=None,
                paragraphs=[claim_data["statement"]],
                language=language,
                is_env=True,
            ))
            if "proof" in claim_data and rng.random() < 0.45:
                segments.append(DocumentSegment(
                    block_type="proof",
                    name=None,
                    number=None,
                    paragraphs=split_proof_paragraphs(claim_data["proof"], rng),
                    language=language,
                    is_env=True,
                ))
            else:
                add_exposition()
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
        elif choice < 0.95:
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
        else:
            # Notation
            notat = rng.choice(domain["notations"])
            segments.append(DocumentSegment(
                block_type="notation",
                name=None,
                number=None,
                paragraphs=[notat],
                language=language,
                is_env=True,
            ))

        # Often continue with math-dense narrative prose. This teaches the CRF
        # that a proof (or any statement) is usually followed by exposition, so
        # an ``I-PROOF`` run must be able to end.
        if rng.random() < 0.55:
            add_exposition()

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
    on_misaligned: Optional[Callable[[], None]] = None,
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
    aligned_tags = align_tags_to_nodes(nodes, expected_tags, segments)
    if aligned_tags is None:
        if on_misaligned is not None:
            on_misaligned()
        print(
            f"Skipping misaligned document {doc_id}: {len(nodes)} nodes vs {len(expected_tags)} tags",
            file=sys.stderr,
        )
        return None

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
) -> Optional[list[str]]:
    """Strict alignment between AST nodes and expected tags.

    Returns None on length mismatch (does NOT truncate or pad with 'O').
    """
    if len(nodes) != len(expected_tags):
        return None
    return expected_tags


def generate_dataset(
    output_dir: Path,
    num_docs: int = 500,
    sources_dir: Optional[Path] = None,
    seed: int = 42,
) -> dict[str, Any]:
    """Generate complete dataset and write train.jsonl, val.jsonl, test.jsonl."""
    rng = random.Random(seed)
    mathia_bin = find_mathia_binary()
    output_dir.mkdir(parents=True, exist_ok=True)

    skipped_misaligned = 0

    def record_misaligned() -> None:
        nonlocal skipped_misaligned
        skipped_misaligned += 1

    source_docs: list[LabeledDocument] = []

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
                        on_misaligned=record_misaligned,
                    )
                    if doc:
                        source_docs.append(doc)
            except Exception as e:
                print(f"Skipping {tex_file}: {e}", file=sys.stderr)

    src_train, src_val, src_test = (
        split_documents(source_docs, 0.80, 0.10, 0.10, seed=seed)
        if source_docs
        else ([], [], [])
    )

    total_target = max(len(source_docs), num_docs)
    target_train = int(round(total_target * 0.80))
    target_val = int(round(total_target * 0.10))
    target_test = total_target - target_train - target_val

    rem_train = max(0, target_train - len(src_train))
    rem_val = max(0, target_val - len(src_val))
    rem_test = max(0, target_test - len(src_test))

    print(
        f"Generating synthetic documents: {rem_train} train, {rem_val} val, {rem_test} test (disjoint templates)..."
    )

    def generate_split_docs(count: int, split_name: str) -> list[LabeledDocument]:
        en_docs: list[LabeledDocument] = []
        fa_docs: list[LabeledDocument] = []
        en_target = count // 2
        fa_target = count - en_target

        attempt = 0
        while len(en_docs) < en_target and attempt < en_target * 6:
            doc_id = f"synth_en_{split_name}_{len(en_docs):04d}"
            title, segments = generate_synthetic_document_segments(
                doc_id, "en", rng, split=split_name
            )
            doc = process_segments_into_labeled_document(
                doc_id, title, "en", segments, mathia_bin, rng, on_misaligned=record_misaligned
            )
            if doc:
                en_docs.append(doc)
            attempt += 1

        attempt = 0
        while len(fa_docs) < fa_target and attempt < fa_target * 6:
            doc_id = f"synth_fa_{split_name}_{len(fa_docs):04d}"
            title, segments = generate_synthetic_document_segments(
                doc_id, "fa", rng, split=split_name
            )
            doc = process_segments_into_labeled_document(
                doc_id, title, "fa", segments, mathia_bin, rng, on_misaligned=record_misaligned
            )
            if doc:
                fa_docs.append(doc)
            attempt += 1

        return en_docs + fa_docs

    train_set = src_train + generate_split_docs(rem_train, "train")
    val_set = src_val + generate_split_docs(rem_val, "val")
    test_set = src_test + generate_split_docs(rem_test, "test")

    stats = write_split_jsonl(output_dir, train_set, val_set, test_set)

    logger.info("skipped_misaligned: %d", skipped_misaligned)
    print(f"Skipped misaligned documents: {skipped_misaligned}")

    print(f"Saved {len(train_set)} documents to {output_dir / 'train.jsonl'}")
    print(f"Saved {len(val_set)} documents to {output_dir / 'val.jsonl'}")
    print(f"Saved {len(test_set)} documents to {output_dir / 'test.jsonl'}")

    return {
        "train": len(train_set),
        "val": len(val_set),
        "test": len(test_set),
        "total_en": sum(1 for d in (train_set + val_set + test_set) if d.language == "en"),
        "total_fa": sum(1 for d in (train_set + val_set + test_set) if d.language == "fa"),
        "skipped_misaligned": skipped_misaligned,
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
