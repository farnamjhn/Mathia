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
│   ├── export_feedback.py      # Telemetry & UI ScopeOverrides ingestion
│   ├── prepare_unlabeled.py    # Real Markdown/HTML notes → unlabeled JSONL + compact LLM input
│   └── import_labeled.py       # Validates LLM BIO labels and merges them into LabeledDocument JSONL
├── prompts/
│   └── label_bio_tags.md       # Ready-to-use LLM labeling prompt
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
4. Mixes in math-dense exposition (`O`) and multi-paragraph proofs so the model
   cannot learn "math-dense paragraph = proof".
5. Produces aligned `(nodes, features, bio_tags)` JSONL sequences.

```bash
uv run python -m mathia_ml.dataset_gen \
    --sources ../sample-data \
    --output-dir data/
```

### Step 2: Stratified Partitioning (`split.py`)
Partitions documents into 80% train, 10% validation, and 10% test splits. Partitioning is strictly at the **document level** to eliminate sequence leakage, and stratified by language (English / Persian).

`dataset_gen.py` already writes `train.jsonl`/`val.jsonl`/`test.jsonl`, so this
step is only needed for a single combined labeled dataset (for example
`real_labeled.jsonl` from the real-notes pipeline) or to verify an existing split.

```bash
# Partition a combined LabeledDocument JSONL
uv run python -m mathia_ml.split \
    --input data/real_labeled.jsonl \
    --output-dir data/ \
    --train-ratio 0.8 \
    --val-ratio 0.1 \
    --test-ratio 0.1

# Verify train/val/test.jsonl have zero document leakage
uv run python -m mathia_ml.split --verify-dir data/
```

### Step 3: CRF Training & Weights Export (`train_crf.py`)
Trains a Linear-Chain CRF using `sklearn-crfsuite` with L-BFGS:
- Performs grid search over L1 ($c_1$) and L2 ($c_2$) regularization penalties on `val.jsonl`.
- Selects the parameter pair maximizing exact-span F1.
- Exports compact, deterministic weights to `crates/math-core/src/crf_weights.json`:

```bash
uv run python -m mathia_ml.train_crf \
    --train-file data/train.jsonl \
    --val-file data/val.jsonl \
    --output-weights ../crates/math-core/src/crf_weights.json
```

#### Transition temperature

The synthetic corpus is small, so the learned transition prior is overconfident:
a proof self-loop can dwarf the emission evidence and Viterbi then never leaves
`I-PROOF`. `train_crf.py` therefore damps every transition weight by
`TRANSITION_TEMPERATURE = 0.4` during decoding, and `crates/math-core/src/crf.rs`
applies the same constant in both its forward and confidence Viterbi passes.
Keep the two values in sync. The raw `sklearn-crfsuite` parity check still runs
at temperature `1.0`, so it compares unmodified transition weights.

Because damped transitions let the emission model decide boundaries, explicit
openers are anchored deterministically in Rust: a labelled or numbered lead
("Theorem 2.", "برهان (Proof):", "قضیه ۳-۱") always starts its block, and its
body span follows the structural rules rather than the CRF's `I-` run.

### Step 4: Model Evaluation (`evaluate.py`)
Benchmarks the trained CRF against the baseline heuristic scorer using `seqeval` exact-span F1, precision, and recall metrics, broken down by language and BlockType:

```bash
uv run python -m mathia_ml.evaluate \
    --test-file data/test.jsonl \
    --weights-file ../crates/math-core/src/crf_weights.json
```

### Labeling Real Notes with an LLM

Real Markdown or HTML notes can be labeled with an LLM and folded into the same
CRF pipeline. The extractor runs `mathia dump-features` (HTML is cleaned and
rendered to Markdown first), so `nodes` and `features` match the CRF schema
exactly.

```bash
# 1. Extract unlabeled documents (full JSONL + compact LLM input)
uv run python -m mathia_ml.prepare_unlabeled ../sample-data \
    --output data/unlabeled.jsonl \
    --llm-input data/unlabeled_for_llm.jsonl

# Optional: pass --config path/to/scoring.toml to score with a custom config
# instead of the built-in defaults.

# 2. Give prompts/label_bio_tags.md plus data/unlabeled_for_llm.jsonl to an LLM
#    and save its JSONL reply as data/llm_labels.jsonl

# 3. Validate the labels and merge them back onto the full documents
uv run python -m mathia_ml.import_labeled \
    --unlabeled data/unlabeled.jsonl \
    --labels data/llm_labels.jsonl \
    --output data/real_labeled.jsonl

# 4. Feed the result into the normal split → train → evaluate pipeline
uv run python -m mathia_ml.split --input data/real_labeled.jsonl --output-dir data/
uv run python -m mathia_ml.train_crf --train-file data/train.jsonl --val-file data/val.jsonl
uv run python -m mathia_ml.evaluate --test-file data/test.jsonl
```

`import_labeled` enforces tag vocabulary, length alignment, legal BIO
transitions, and the "no `B-*` on non-openable nodes" rule before writing
anything; pass `--skip-invalid` to drop bad documents instead of aborting.

---

## 4. Feature Parity Invariant

To guarantee that the pure-Rust inference engine (`crates/math-core/src/crf.rs`) produces the exact same probabilities as `sklearn-crfsuite`:
- Any change to feature logic in `ml/src/mathia_ml/features.py` must be mirrored identically in `crates/math-core/src/crf.rs`.
- Run the cross-language parity test in the Rust workspace after any feature adjustment:
  ```bash
  cargo test -p math-core test_feature_functions_parity
  ```
