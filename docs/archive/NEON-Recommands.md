# Mathia Lightweight ML Block Detection Plan

## Core Philosophy
Upgrade Mathia’s block detection from decoupled heuristic scoring + greedy scoping to **joint BIO Sequence Labeling** without adding any runtime bloat to the Tauri desktop app or WASM build.

- **Zero heavy runtime dependencies:** No ONNX Runtime, no PyTorch, no Python sidecar, no bundled local LLMs.
- **100% Pure Rust at inference:** A Linear-Chain Conditional Random Field (CRF) decoded via Viterbi in ~100 lines of Rust, using ~50 KB of embedded JSON weights (`< 2 ms` per document).
- **Python (`ml/`) is strictly dev-time only:** Managed via `uv` for dataset generation, training, evaluation, and exporting weights. Never shipped to end users.
- **Deterministic guarantees preserved:** LaTeX `EnvSpan` blocks (`score = 1.0`) and manual `ScopeOverrides` always take precedence.

---

## Target Directory Layout

```text
Mathia/
├── crates/
│   ├── math-core/
│   │   └── src/
│   │       ├── normalize.rs      # [MOD] Add OCR page-artifact stripper
│   │       ├── tagger.rs         # [NEW] BioTag, SequenceTagger trait, HeuristicTagger
│   │       ├── crf.rs            # [NEW] Pure-Rust Viterbi decoder + embedded weights
│   │       ├── classifier.rs     # [MOD] Delegate Stages 3–4 to SequenceTagger; stable anchors
│   │       └── crf_weights.json  # [NEW] Exported ~50 KB CRF weights (included via include_str!)
│   └── math-cli/                 # [MOD] Add `dump-features` subcommand
└── ml/                           # [NEW] Dev-only Python workspace (`uv`), NOT bundled
    ├── pyproject.toml
    ├── data/                     # Gitignored JSONL datasets
    └── src/mathia_ml/
        ├── dataset_gen.py        # LaTeX env stripping + synthetic OCR noise
        ├── export_feedback.py    # ScopeOverrides / math-judge -> JSONL
        ├── features.py           # Sliding-window [i-1, i, i+1] & delta features
        ├── train_crf.py          # Train sklearn-crfsuite -> export crf_weights.json
        └── evaluate.py           # Exact-span F1 (Heuristic vs. CRF)
```

# Slice 0: Architecture Prep, OCR Cleanup & Stable Anchors (Rust)

Goal: Abstract Stages 3 & 4 behind a SequenceTagger trait, eliminate training-serving skew by dumping Rust’s exact NodeFeatures, and fix two high-impact structural issues before introducing ML.

Tasks

1. OCR Page-Artifact Stripper (crates/math-core/src/normalize.rs):
  ⚬ Strip isolated page numbers (lines containing only ASCII or Persian digits, e.g., 12, ۱۲, - 4 -) and repeating running headers/footers before Markdown parsing so they don't split multi-paragraph blocks.
2. Stable Anchors for ScopeOverrides (crates/math-core/src/classifier.rs):
  ⚬ Replace fragile node_index keys with content-based anchors: (hash(normalized_first_64_chars), occurrence_index).
  ⚬ Keep node_index as a fallback only when no anchor matches, so editing text above a block no longer corrupts manual UI overrides.
3. Define BIO Tagging Trait (crates/math-core/src/tagger.rs):
  ⚬ Add enum BioTag { Outside, Begin(BlockType), Inside(BlockType) }.
  ⚬ Add struct TaggedNode { tag: BioTag, confidence: f32 }.
  ⚬ Define trait SequenceTagger { fn tag(&self, nodes: &[Node], features: &[NodeFeatures]) -> Vec<TaggedNode>; }.
  ⚬ Wrap existing scoring + statement_end / argument_end / implicit_proof_score in HeuristicTagger.
  ⚬ Add tags_to_blocks in classifier.rs preserving EnvSpan, link_arguments, crossref::link_references, and ScopeOverrides.
4. Feature Dumper CLI (mathia dump-features <input.md>):
  ⚬ Run normalize_markdown -> parse_markdown -> Lexicon::extract.
  ⚬ Output JSON with nodes (index, kind, flat_text, char_len, env_span) and full serde::Serialize NodeFeatures per node.

Acceptance Criteria

⚬ All existing math-core unit and snapshot tests pass with HeuristicTagger.
⚬ mathia dump-features sample.md outputs valid JSON matching classifier.rs node indices.

# Slice 1: Offline Python Data Flywheel (ml/)

Goal: Generate thousands of labeled bilingual (English + Persian) (nodes, features, bio_tags) sequences without manual annotation.

Tasks

1. Initialize Dev Workspace:
  ⚬ Run uv init ml (python >= 3.11, < 3.13).
  ⚬ Add minimal dependencies: sklearn-crfsuite>=0.5.0, scikit-learn, seqeval, numpy, pydantic.
2. LaTeX Environment Stripper (ml/src/mathia_ml/dataset_gen.py):
  ⚬ Ingest .tex sources containing explicit environments: Theorem (قضیه), Lemma (لم), Proposition (گزاره), Corollary (نتیجه), Definition (تعریف), Proof (اثبات), Example (مثال), Remark (تذکر).
  ⚬ Step A: Run mathia dump-features on the original file to extract ground-truth BioTags from EnvSpan.
  ⚬ Step B: Strip \begin{...} / \end{...} wrappers and rewrite them as realistic Markdown headers (**Theorem 1.2.**, قضیه ۳.۲:, or keyword-less implicit proofs).
  ⚬ Step C: Inject synthetic OCR noise:
    ⚬ Drop QED symbols (\blacksquare, □) in 25% of proofs.
    ⚬ Drop bolding or trailing colons on 20% of openers.
    ⚬ Split single-paragraph statements into multi-paragraph blocks.
  ⚬ Step D: Run mathia dump-features on the stripped/noisy Markdown and align ground-truth BioTags.
3. Feedback Exporter (ml/src/mathia_ml/export_feedback.py):
  ⚬ Convert saved ScopeOverrides and converged math-judge verdicts into the same JSONL schema.
4. Document-Level Split:
  ⚬ Split 80/10/10 (train.jsonl, val.jsonl, test.jsonl) strictly by document, stratified by language (English / Persian).

Acceptance Criteria

⚬ uv run python -m mathia_ml.dataset_gen produces clean train.jsonl, val.jsonl, test.jsonl with zero document leakage.

Slice 2: Linear-Chain CRF & Pure-Rust Viterbi Decoder

Goal: Jointly learn opener detection and block scoping transitions, then run inference natively in pure Rust.

Tasks

1. Shared Feature Window Specification (ml/src/mathia_ml/features.py):
  ⚬ For each node i, generate string feature keys from [i-1, i, i+1]:
    ⚬ All NodeFeatures signals (lead_kind, labelled, emphasized, body_empty, qed, math_density bucketed, logic_density bucketed, cues).
    ⚬ node_kind (Paragraph, DisplayMath, List, Heading, Rule, etc.).
    ⚬ Relational / delta features: math_density_delta (up/down/flat), after_display_math, prev_ends_with_colon, is_first_node, is_last_node.
2. Train & Export (ml/src/mathia_ml/train_crf.py):
  ⚬ Train sklearn_crfsuite.CRF(algorithm="lbfgs") tuning c1/c2 regularization on val.jsonl.
  ⚬ Export crates/math-core/src/crf_weights.json:
    {
      "labels": ["O", "B-THEOREM", "I-THEOREM", "..."],
      "transitions": [[0.12, -0.45, ...]],
      "state_features": {
        "0:lead=theorem": { "1": 2.41 },
        "-1:ends_colon=true": { "2": 0.88 }
      }
    }
    
3. Pure-Rust Viterbi Decoder (crates/math-core/src/crf.rs):
  ⚬ Replicate the exact [i-1, i, i+1] feature key generator in Rust.
  ⚬ Load crf_weights.json via include_str!("crf_weights.json") (zero external file I/O required at runtime).
  ⚬ Implement CrfTagger using standard Viterbi dynamic programming over transitions + state_features.
  ⚬ Compute per-node confidence via the margin between the best and second-best Viterbi scores.
4. Evaluation Harness (ml/src/mathia_ml/evaluate.py):
  ⚬ Benchmark HeuristicTagger vs. CrfTagger on test.jsonl using seqeval exact-span F1, broken down by language (English / Persian) and BlockType.

Acceptance Criteria

⚬ Rust CrfTagger matches Python sklearn-crfsuite predictions with > 99.9% parity on test.jsonl.
⚬ Exact-span F1 outperforms HeuristicTagger on multi-paragraph statements and QED-less proofs.
⚬ Binary size increase < 100 KB; inference time < 2 ms for a 500-node document.

# Slice 3: Confidence-Gated math-judge Escalation

Goal: Use your existing optional math-judge layer only on genuinely ambiguous spans, keeping normal compilation instant and free.

Tasks

1. Confidence Gating (crates/math-core/src/classifier.rs):
  ⚬ Run EnvSpan bypass first, then CrfTagger across the document.
  ⚬ If a BlockJudge is configured, only send windows [i-3 .. i+3] to math-judge when:
    ⚬ CrfTagger confidence margin falls below config.ml_confidence_threshold, OR
    ⚬ An Outside paragraph with high logic_density immediately follows a Theorem (قضیه) or Lemma (لم).
2. Local Override Telemetry (Opt-In):
  ⚬ When the user applies a ScopeOverride in the Tauri UI, append the anonymized (nodes, features, corrected_bio_tags) entry to a local JSONL file so periodic train_crf.py runs continuously improve crf_weights.json.

Acceptance Criteria

⚬ Clean documents never trigger math-judge (0 ms overhead, 0 API cost).
⚬ ScopeOverrides always win over both CrfTagger and math-judge.

Agent Execution Order

1. Slice 0: Refactor classifier.rs -> SequenceTagger, add normalize.rs artifact stripper, stable anchors, and dump-features CLI. Verify all Rust tests pass.
2. Slice 1: Set up ml/ with uv, generate train/val/test.jsonl, and manually inspect 20 Persian/English samples to verify label alignment.
3. Slice 2: Train CRF, export crf_weights.json, implement crf.rs, verify > 99.9% Python-to-Rust parity, and record baseline vs. CRF F1 scores.
4. Slice 3: Wire confidence-gated math-judge escalation and local override logging.