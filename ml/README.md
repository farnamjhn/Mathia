# Mathia ML Developer Flywheel (`mathia-ml`)

The `ml/` workspace is a dev-only Python environment managed via [`uv`](https://github.com/astral-sh/uv). It handles dataset generation, synthetic OCR perturbation, Linear-Chain Conditional Random Field (CRF) training, validation tuning, exact-span F1 evaluation, and deterministic weight export to pure Rust.

> **CRITICAL INVARIANT:** This workspace is strictly for offline development and model training. It is **never** bundled with the Mathia CLI or desktop app, and end users require neither Python nor any Python package to run Mathia.

---

## 1. Quickstart & Setup

Prerequisites: Python 3.11+ and `uv`.

```bash
cd ml

# Create virtual environment and install pinned dependencies
uv sync

# Run Python test suite
uv run pytest
```

---

## 2. Directory Structure

```text
ml/
├── pyproject.toml              # uv project definition (sklearn-crfsuite, seqeval, pydantic)
├── uv.lock                     # Lockfile for reproducible Python dependencies
├── data/                       # Local JSONL datasets (train.jsonl, val.jsonl, test.jsonl)
├── src/mathia_ml/
│   ├── __init__.py
│   ├── schema.py               # Pydantic models matching Rust AST and NodeFeatures
│   ├── dataset_gen.py          # LaTeX env stripping & synthetic OCR noise generator
│   ├── features.py             # Context window [i-1, i, i+1] and delta feature generator
│   ├── split.py                # Document-level stratified train/val/test partitioner
│   ├── train_crf.py            # L-BFGS CRF trainer, c1/c2 tuner, & crf_weights.json exporter
│   ├── evaluate.py             # seqeval exact-span F1 evaluation harness
│   └── export_feedback.py      # Telemetry & UI ScopeOverrides ingestion
└── tests/                      # Pytest suite verifying feature parity, training, and metrics
```

---

## 3. Workflow & Data Pipeline

### Step 1: Synthetic Dataset Generation (`dataset_gen.py`)
Mathematical textbooks with LaTeX theorem-like environments (`\begin{theorem}...\end{theorem}`) provide ground truth. `dataset_gen.py` transforms them into realistic OCR-transcribed Markdown:
1. Strips explicit LaTeX environment tags.
2. Formats openers as Markdown badges (e.g., `**Theorem 3.1.**`, `قضیه ۱.۲:`).
3. Injects synthetic OCR noise:
   - Drops bolding or trailing colons on 20% of openers.
   - Drops QED symbols ($\blacksquare$, □, `چ.ب.ح`) on 25% of proofs.
   - Splits single-paragraph statements across multiple paragraphs.
4. Produces aligned `(nodes, features, bio_tags)` JSONL sequences.

```bash
uv run python -m mathia_ml.dataset_gen \
    --input-dir ../sample-data \
    --output-dir data/
```

### Step 2: Stratified Partitioning (`split.py`)
Partitions documents into 80% train, 10% validation, and 10% test splits. Partitioning is strictly at the **document level** to eliminate sequence leakage, and stratified by language (English / Persian).

```bash
uv run python -m mathia_ml.split \
    --data-dir data/ \
    --train-ratio 0.8 \
    --val-ratio 0.1 \
    --test-ratio 0.1
```

### Step 3: CRF Training & Weights Export (`train_crf.py`)
Trains a Linear-Chain CRF using `sklearn-crfsuite` with L-BFGS:
- Performs grid search over L1 ($c_1$) and L2 ($c_2$) regularization penalties on `val.jsonl`.
- Selects the parameter pair maximizing exact-span F1.
- Exports compact, deterministic weights to `crates/math-core/src/crf_weights.json`:

```bash
uv run python -m mathia_ml.train_crf \
    --train-path data/train.jsonl \
    --val-path data/val.jsonl \
    --output-weights ../crates/math-core/src/crf_weights.json
```

### Step 4: Model Evaluation (`evaluate.py`)
Benchmarks the trained CRF against the baseline HeuristicTagger using `seqeval` exact-span F1, precision, and recall metrics, broken down by language and BlockType:

```bash
uv run python -m mathia_ml.evaluate \
    --test-path data/test.jsonl \
    --weights-path ../crates/math-core/src/crf_weights.json
```

---

## 4. Feature Parity Invariant

To guarantee that the pure-Rust inference engine (`crates/math-core/src/crf.rs`) produces the exact same probabilities as `sklearn-crfsuite`:
- Any change to feature logic in `ml/src/mathia_ml/features.py` must be mirrored identically in `crates/math-core/src/crf.rs`.
- Run the cross-language parity test in the Rust workspace after any feature adjustment:
  ```bash
  cargo test -p math-core test_feature_functions_parity
  ```
