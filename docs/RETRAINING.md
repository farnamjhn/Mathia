# Retraining the CRF from User Corrections and Real Notes

This document describes the operational loop that turns real block-size
corrections and real labeled notes into a new embedded CRF model. It is the
practical companion to [`REAL_NOTES_LABELING.md`](REAL_NOTES_LABELING.md) and
the training sections of [`ml/README.md`](../ml/README.md).

The ML workspace is dev-only. End users never need Python; the only artifact
that ships is `crates/math-core/src/crf_weights.json`, embedded at compile time
via `include_str!`.

---

## 1. What Feeds the Model

The model mixes two sources of real signal with the synthetic baseline:

| Source | Metadata `source` | What it teaches |
|---|---|---|
| Desktop `ScopeOverrides` telemetry | `user_correction` | Real block sizes (`end`, `delta`), retypes, suppressions |
| LLM-labeled real notes | `real_notes` | Lexical and structural boundaries on real formatting |
| Synthetic corpus (`dataset_gen.py`) | absent | Baseline coverage, bilingual openers, OCR noise |

`train_crf.py` oversamples `user_correction` and `real_notes` documents
(`--correction-weight`, default `3`) so real corrections are not drowned out by
the synthetic corpus.

### Telemetry is anonymized

`create_override_telemetry` strips prose from `flat_text` (keeping only a
trailing colon) and preserves `kind`, `char_len`, `env_span`, `NodeFeatures`,
and the corrected BIO tags. Consequences:

- Structural and length features (`char_len_bucket`, `dist_from_lead`,
  `prev_ends_continuation_punct`) learn from telemetry.
- Lexical features (`starts_lowercase`, `starts_math`) cannot learn from
  telemetry; use real labeled notes for those.
- Entries are deduplicated by `doc_id`, so re-applying the same correction does
  not append duplicate rows.

---

## 2. Prerequisites

```bash
cd ml
uv sync
```

You also need a built `mathia` binary for `prepare_unlabeled.py` /
`export_feedback.py` (they run `mathia dump-features` under the hood):

```bash
cargo build -p mathia
```

---

## 3. Collect Corrections in the Desktop App

Telemetry is opt-in and disabled by default (`ml_telemetry = false`).

1. Dump a config template:
   ```bash
   cargo run -p mathia -- --dump-config > scoring.toml
   ```
2. Set:
   ```toml
   ml_telemetry = true
   ```
3. Load that config in the app, open a document, and apply corrections in the
   inspector (`end`, `delta`, `retype`, `suppressed`).

Telemetry is written to the OS app-data directory, e.g. on macOS:

```text
~/Library/Application Support/dev.mathia.desktop/telemetry.jsonl
```

When the **development** binary runs from the repository root, it also appends
to `ml/data/feedback.jsonl`. Packaged apps do not write into the repository;
copy the app-data `telemetry.jsonl` manually if you run a bundle.

---

## 4. Collect Real Labeled Notes

```bash
cd ml

# 1. Extract AST nodes + features
uv run python -m mathia_ml.prepare_unlabeled ../sample-data \
    --output data/unlabeled.jsonl \
    --llm-input data/unlabeled_for_llm.jsonl

# 2. Label unlabeled_for_llm.jsonl with prompts/label_bio_tags.md
#    and save the JSONL reply as data/llm_labels.jsonl

# 3. Validate and merge labels back onto full documents
uv run python -m mathia_ml.import_labeled \
    --unlabeled data/unlabeled.jsonl \
    --labels data/llm_labels.jsonl \
    --output data/real_labeled.jsonl
```

`import_labeled` enforces tag vocabulary, length alignment, legal BIO
transitions, and the "no `B-*` on non-openable nodes" rule. Pass
`--skip-invalid` to drop bad documents instead of aborting.

---

## 5. Ingest Feedback into the Dataset

`export_feedback.py` normalizes both telemetry and raw
`(markdown, overrides.json)` pairs into `LabeledDocument` JSONL with
`metadata["source"] = "user_correction"`.

```bash
cd ml

# A. Anonymized telemetry JSONL
uv run python -m mathia_ml.export_feedback \
    --telemetry "$HOME/Library/Application Support/dev.mathia.desktop/telemetry.jsonl" \
    --merge-into data

# B. Raw (markdown, overrides.json) pairs in a directory
uv run python -m mathia_ml.export_feedback \
    --dir path/to/pairs \
    --merge-into data

# C. A single pair
uv run python -m mathia_ml.export_feedback \
    --markdown notes.md \
    --overrides notes.overrides.json \
    --merge-into data
```

`--merge-into` merges with the existing `train.jsonl` / `val.jsonl` /
`test.jsonl` and resplits 80/10/10 at the document level. Use `--split` instead
to write a fresh split, or `--output` to write a single JSONL without splitting.

Verify there is no document leakage after merging:

```bash
uv run python -m mathia_ml.split --verify-dir data/
```

---

## 6. Train and Export Weights

```bash
cd ml
uv run python -m mathia_ml.train_crf \
    --train-file data/train.jsonl \
    --val-file data/val.jsonl \
    --output-weights ../crates/math-core/src/crf_weights.json
```

What the trainer does:

1. Oversamples `user_correction` / `real_notes` training documents
   (`--correction-weight`, default `3`).
2. Tunes L1/L2 regularization (`c1`, `c2`) on validation exact-span F1.
3. Sweeps inference-time transition temperatures
   (`--temperatures 0.4 0.6 0.8 1.0`, default) on `val.jsonl` and reports the
   winner.
4. Exports `crates/math-core/src/crf_weights.json`.
5. Writes `ml/data/test_viterbi_expected.json` (gitignored) for the Rust↔Python
   parity test.

Useful flags:

```text
--correction-weight N   oversampling weight (default 3)
--temperatures T...     validation temperature candidates
--no-temp-sweep         skip the sweep
--no-tune               skip c1/c2 tuning
--c1 / --c2             fix regularization explicitly
```

---

## 7. Keep `TRANSITION_TEMPERATURE` in Sync

The decoder applies a damping factor to every learned transition weight. It is
defined twice and **must match**:

- `crates/math-core/src/crf.rs` → `TRANSITION_TEMPERATURE`
- `ml/src/mathia_ml/train_crf.py` → `TRANSITION_TEMPERATURE`

If the sweep picks a different value, the trainer prints a reminder:

```text
Reminder: update TRANSITION_TEMPERATURE in crates/math-core/src/crf.rs from 0.8 to 0.6 ...
```

After updating both constants, rerun `train_crf.py` once so
`test_viterbi_expected.json` is regenerated at the same temperature. Otherwise
the Rust parity test compares predictions produced at different temperatures.

The current shipped value is `0.8`.

---

## 8. Rebuild and Verify

The weights are embedded with `include_str!`, so the app must be recompiled:

```bash
cargo build -p mathia-app
```

If block spans changed, regenerate the corpus snapshots and review the diff:

```bash
MATHIA_UPDATE_SNAPSHOTS=1 cargo test -p math-core --test corpus
git diff crates/math-core/tests/snapshots
```

Then run the full checks:

```bash
cargo fmt --all --check
cargo test --workspace
cd ml && uv run pytest
```

---

## 9. What Gets Committed

| Path | Committed? | Notes |
|---|---|---|
| `crates/math-core/src/crf_weights.json` | Yes | The shipped model |
| `crates/math-core/tests/snapshots/*.snap` | Yes | Update only for intended span changes |
| `ml/data/**/*.jsonl`, `ml/data/**/*.json` | No | Gitignored local datasets and expected predictions |
| `ml/data/test_viterbi_expected.json` | No | Regenerated by `train_crf.py` |

---

## 10. Troubleshooting

- **Rust↔Python parity below 99.9%**: regenerate
  `test_viterbi_expected.json` with the current weights and temperature; a stale
  file is the most common cause.
- **A retrain opens spurious blocks on continuation paragraphs**: the CRF may
  score `B-PROOF` on "where …" paragraphs. `sanitize_argument_openers` folds
  continuation nodes back into the preceding statement; structural scoping is
  also kept as a floor in `tags_to_blocks`.
- **Telemetry file is empty**: confirm `ml_telemetry = true`, that you applied a
  non-empty `ScopeOverrides`, and (for the repo copy) that you ran the dev
  binary from the repository root.
- **Snapshots change unexpectedly**: inspect the diff before committing. Score
  changes are expected after retraining; span changes should be justified.
