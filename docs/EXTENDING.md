# Mathia Extension Guide ("How to Extend")

This guide provides concrete, repeatable recipes for making common architectural and feature modifications to the Mathia codebase. Follow the exact file checklists and verification commands to maintain compiler determinism, test suite compliance, and cross-language parity.

---

## Table of Recipes

1. [Adding a New Semantic BlockType](#1-adding-a-new-semantic-blocktype)
2. [Adding a New Feature to the Classifier & CRF](#2-adding-a-new-feature-to-the-classifier--crf)
3. [Retraining & Deploying Updated CRF Weights](#3-retraining--deploying-updated-crf-weights)
4. [Adding a New CLI Option or Configuration Field](#4-adding-a-new-cli-option-or-configuration-field)
5. [Adding a New Tauri Desktop IPC Command](#5-adding-a-new-tauri-desktop-ipc-command)
6. [Adding a New Interactive AI Provider](#6-adding-a-new-interactive-ai-provider)

---

## 1. Adding a New Semantic BlockType

**Context:** Adding a new first-class mathematical construct (e.g., `Axiom`, `Conjecture`, or `Hypothesis`) requires updates across the domain model, bilingual lexicons, decision models, Python ML schema, and UI badges.

### Checklist

1. **Update Domain Model (`crates/math-core/src/model.rs`)**:
   - Add variant to `enum BlockType`:
     ```rust
     pub enum BlockType {
         Theorem,
         // ...
         Axiom, // <--- Add new variant
         Heading,
         Text,
     }
     ```
   - Add to `BlockType::SEMANTIC` array and update its constant size.
   - Update `is_statement(self)` (return `true` if it states a claim) or `is_argument(self)` (if it functions like a proof).
   - Update `from_env_name(name: &str)` to recognize LaTeX environments (`\begin{axiom}`).
   - Update `slug(self)` with the machine-readable name (e.g., `"axiom"`).
   - Update `label(self, dir: Direction)` with human-readable labels for both `Direction::Rtl` (Persian: e.g., `"اصل موضوع"`) and `Direction::Ltr` (English: e.g., `"Axiom"`).

2. **Update Lexicon & Feature Detection (`crates/math-core/src/features.rs`)**:
   - In `Lexicon::new(config: &ScoringConfig)`, register the Persian and English opener keywords and patterns for the new block type.

3. **Update Judge Contract & Prompts (`crates/math-judge/src/jev.rs`)**:
   - In `kind_criteria()`, add a concise, single-sentence definition for the new type so external LLM judges know when to select it:
     ```rust
     BlockType::Axiom => "A foundational statement accepted without proof as a starting point.",
     ```

4. **Update Python ML Schema & Evaluation (`ml/src/mathia_ml/`)**:
   - In `ml/src/mathia_ml/schema.py` and `ml/src/mathia_ml/evaluate.py`, add the new slug to `SEMANTIC_TYPES`.
   - In `ml/src/mathia_ml/dataset_gen.py`, add the bilingual keyword patterns for synthetic dataset generation.

5. **Retrain CRF Weights & Verify**:
   - Run the ML training pipeline (see [Recipe 3](#3-retraining--deploying-updated-crf-weights)) to export updated weights to `crates/math-core/src/crf_weights.json`.

6. **Verification**:
   ```bash
   cargo fmt --check
   cargo test --workspace
   ```

---

## 2. Adding a New Feature to the Classifier & CRF

**Context:** Extracting a new structural or lexical signal (e.g., detecting indentation, list bullet styles, or specific Persian transitional markers) for both heuristic scoring and the machine learning model.

> **CRITICAL INVARIANT:** Feature key generation in `crates/math-core/src/crf.rs` and `ml/src/mathia_ml/features.py` must maintain **100% exact string-for-string parity**. Any discrepancy causes silent training-serving skew.

### Checklist

1. **Add Feature to Core Struct (`crates/math-core/src/features.rs`)**:
   - Add the field to `struct Features`:
     ```rust
     #[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
     pub struct Features {
         // ...
         pub has_bullet_marker: bool,
     }
     ```
   - In `Lexicon::extract(&self, node: &Node) -> Features`, implement the extraction logic from AST node data.

2. **Add to Pure-Rust CRF Feature Visitor (`crates/math-core/src/crf.rs`)**:
   - In `extract_node_base_features_visitor()`, emit the feature string at offset prefix `p`:
     ```rust
     if feat.has_bullet_marker {
         buf.clear();
         let _ = write!(buf, "{p}has_bullet_marker=true");
         visit(buf);
     }
     ```

3. **Add to Python ML Schema (`ml/src/mathia_ml/schema.py`)**:
   - Add the field with matching default to Pydantic class `NodeFeatures`:
     ```python
     class NodeFeatures(BaseModel):
         # ...
         has_bullet_marker: bool = False
     ```

4. **Add to Python ML Feature Extractor (`ml/src/mathia_ml/features.py`)**:
   - In `extract_node_features()`, emit the exact same string key:
     ```python
     if _get(feat, "has_bullet_marker"):
         features[f"{p}has_bullet_marker=true"] = 1.0
     ```

5. **Verify Python-Rust Parity**:
   - Run the dedicated cross-validation test in Rust:
     ```bash
     cargo test -p math-core test_feature_functions_parity
     ```

---

## 3. Retraining & Deploying Updated CRF Weights

**Context:** Updating model weights after modifying features, adding BlockTypes, or expanding the training dataset with new textbook corpora.

### Step-by-Step Procedure

1. **Navigate to the ML Directory**:
   ```bash
   cd ml
   ```

2. **Sync the Python Environment (`uv`)**:
   ```bash
   uv sync
   ```

3. **Generate Synthetic Bilingual Datasets (Optional if data already exists)**:
   ```bash
   uv run python -m mathia_ml.dataset_gen \
       --sources ../sample-data \
       --output-dir data/
   ```

4. **Split Combined Data (or Verify an Existing Split)**:
   ```bash
   # Partition a single combined LabeledDocument JSONL (e.g. real_labeled.jsonl)
   uv run python -m mathia_ml.split \
       --input data/real_labeled.jsonl \
       --output-dir data/ \
       --train-ratio 0.8 \
       --val-ratio 0.1 \
       --test-ratio 0.1

   # Verify train/val/test.jsonl have zero document leakage
   uv run python -m mathia_ml.split --verify-dir data/
   ```
   *`dataset_gen.py` already writes `train.jsonl`/`val.jsonl`/`test.jsonl`, so
   this step is only needed for combined labeled data or split verification.*

5. **Train CRF with Hyperparameter Tuning and Export Weights**:
   ```bash
   uv run python -m mathia_ml.train_crf \
       --train-file data/train.jsonl \
       --val-file data/val.jsonl \
       --output-weights ../crates/math-core/src/crf_weights.json
   ```
   *Note: This automatically tunes L1 ($c_1$) and L2 ($c_2$) penalties via L-BFGS, optimizes for exact-span F1 on validation, and writes the compact JSON weights directly to the core Rust crate.*

6. **Benchmark Exact-Span F1**:
   ```bash
   uv run python -m mathia_ml.evaluate \
       --test-file data/test.jsonl \
       --weights-file ../crates/math-core/src/crf_weights.json
   ```

7. **Verify Rust Test Suite & Benchmarks**:
   ```bash
   cd ..
   cargo test -p math-core crf::tests
   ```

---

## 4. Adding a New CLI Option or Configuration Field

**Context:** Exposing a new compiler toggle, scope limit, or output option through `scoring.toml` and the `mathia` CLI binary.

### Checklist

1. **Update Configuration Model (`crates/math-core/src/config.rs`)**:
   - Add field to `struct ScoringConfig` (or the nested `ScopeConfig`).
   - Define sensible defaults in `impl Default for ScoringConfig`.
   - Update `ScoringConfig::validate(&self)` to enforce value ranges (e.g., scores in $[0.0, 1.0]$).

2. **Update CLI Parser (`crates/mathia-cli/src/main.rs`)**:
   - Update `const USAGE: &str` string with flag documentation.
   - In `fn run()`, parse the new flag inside the argument match loop (`--my-flag`).
   - Propagate the flag value into `Settings`.

3. **Verification**:
   ```bash
   # Test dump-config reflects new field
   cargo run -p mathia -- --dump-config
   
   # Test compilation with the flag
   cargo run -p mathia -- sample-data/0-25.md -o /tmp/test.html
   ```

---

## 5. Adding a New Tauri Desktop IPC Command

**Context:** Creating a backend command in the Tauri v2 desktop app that can be called asynchronously from the webview UI (`ui/main.js`).

### Checklist

1. **Implement Core Session Logic (`src-tauri/src/session.rs`)**:
   - Add session state management or transformation method on `Session` (e.g., `pub fn my_action(&mut self, ...) -> Result<...>`).

2. **Define the Tauri Command (`src-tauri/src/main.rs`)**:
   - Add a new command function tagged with `#[tauri::command]`:
     ```rust
     #[tauri::command]
     async fn my_new_command(
         app: AppHandle,
         state: State<'_, Shared>,
         param: String,
     ) -> Result<String, String> {
         let mut session = lock(&state);
         session.my_action(&param).map_err(|e| e.to_string())
     }
     ```
   - Register the function in `tauri::generate_handler![...]` inside `main()`:
     ```rust
     .invoke_handler(tauri::generate_handler![
         pick_markdown,
         // ...
         my_new_command, // <--- Add here
     ])
     ```

3. **Check Permissions & Capabilities (`src-tauri/capabilities/default.json`)**:
   - If the command introduces new core plugins or custom protocol access, ensure it is permitted under permissions. Default internal commands are auto-permitted.

4. **Call from UI (`ui/main.js`)**:
   - Invoke the command via the Tauri global IPC bridge:
     ```javascript
     const result = await window.__TAURI__.core.invoke('my_new_command', {
         param: 'value'
     });
     ```

5. **Verification**:
   ```bash
   cargo test -p mathia-app
   ```

---

## 6. Adding a New Interactive AI Provider

**Context:** Integrating an additional cloud LLM provider (e.g., Mistral, Cohere, or a custom internal endpoint) into the compiled document's interactive client-side runtime.

### Checklist

1. **Add Provider Enum (`crates/math-core/src/html.rs`)**:
   - Add variant to `enum AiProvider`:
     ```rust
     pub enum AiProvider {
         Anthropic,
         OpenAi,
         Gemini,
         Compatible,
         Mistral, // <--- Add new variant
     }
     ```
   - Add to `AiProvider::ALL` array.
   - Define its machine slug in `slug(self)` (e.g., `"mistral"`).
   - Define its CSP `connect-src` whitelist origin in `origin(self)`:
     ```rust
     Self::Mistral => "https://api.mistral.ai",
     ```
   - Update `from_slug(s: &str)`.

2. **Update Runtime Client Script (`crates/math-core/assets/ai.js`)**:
   - Add default recommended model to `DEFAULT_MODELS`.
   - Add API base origin to `BUILTIN_ORIGINS`.
   - In `fetchChatCompletion(provider, model, key, messages, streamCallback)`, implement request payload construction and streaming SSE line-parsing matching the provider's API specification.

3. **Verification**:
   - Compile a document enabling the new provider:
     ```bash
     cargo run -p mathia -- sample-data/0-25.md -o /tmp/ai_test.html --ai-provider mistral
     ```
   - Run the headless Chrome test harness to verify CSP hash integrity and script loading:
     ```bash
     cd tests/js && npm test
     ```
