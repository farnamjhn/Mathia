# Mathia Known Issues, Architectural Debt & Constraints

This document records architectural trade-offs, fragile couplings, implicit environmental assumptions, and areas of technical debt identified across the Mathia codebase. Every entry is strictly grounded in the current implementation.

---

## 1. Implicit Environmental Assumptions

### Relative Working Directory Assumption for Local ML Telemetry
- **Canonical Location:** [`src-tauri/src/main.rs`](../src-tauri/src/main.rs#L468-L471)
- **Code Reference:**
  ```rust
  let ml_data = std::path::Path::new("ml/data/feedback.jsonl");
  if ml_data.parent().map(|p| p.is_dir()).unwrap_or(false) {
      let _ = session.record_override_telemetry(&content, ml_data);
  }
  ```
- **Constraint:** When a user sets a manual override in the desktop app with `ml_telemetry = true`, Mathia attempts to write directly to the relative path `ml/data/feedback.jsonl`.
- **Impact:** This succeeds only when running the development binary from the repository root via `cargo run -p mathia-app`. When running as a packaged bundle (`.app` on macOS, `.exe` on Windows) launched from the dock or desktop, the current working directory is typically not the repository root, so this write silently no-ops. Telemetry is still saved to the OS app data directory (`data_dir(&app).join("telemetry.jsonl")`), but direct repo synchronization requires manual developer extraction.
- **Retraining:** See [`RETRAINING.md`](RETRAINING.md) for extracting the app-data telemetry, exporting it as `user_correction` documents, and retraining the embedded model.

### CRLF vs LF Line-Ending Sensitivity in Content Security Policy (CSP)
- **Canonical Location:** [`crates/math-core/src/html.rs`](../crates/math-core/src/html.rs#L474-L483)
- **Code Reference:**
  ```rust
  fn normalize_newlines(text: &str) -> String {
      if !text.contains('\r') {
          return text.to_string();
      }
      text.replace("\r\n", "\n").replace('\r', "\n")
  }
  ```
- **Constraint:** The HTML5 compiler computes SHA-256 hashes of embedded JavaScript (`runtime.js`, `katex.min.js`, `ai.js`) to generate strict CSP headers (`script-src 'sha256-...'`). Modern web browsers normalize script element bodies to LF (`\n`) before validating CSP hashes.
- **Risk:** On Windows machines checked out with Git's `core.autocrlf = true`, files on disk contain `\r\n`. Without `normalize_newlines`, hash generation yields hashes of CRLF strings while the browser checks LF strings, resulting in silent CSP rejection of all scripts.
- **Maintenance Invariant:** Any future script or stylesheet added to the inline document template **must** pass through `normalize_newlines` prior to hash computation.

---

## 2. Fragile Couplings & Sync Requirements

### Cross-Language Feature Key Invariant (Rust vs. Python)
- **Canonical Locations:**
  - Rust: [`crates/math-core/src/crf.rs`](../crates/math-core/src/crf.rs#L139-L260) (`extract_node_base_features_visitor`)
  - Python: [`ml/src/mathia_ml/features.py`](../ml/src/mathia_ml/features.py) (`extract_node_features`)
- **Constraint:** The pure-Rust Viterbi decoder uses lookup keys in `model.state_features` (e.g., `"-1:node_kind=paragraph"`, `"0:math_density=medium"`, `"0:char_len_bucket=short"`, `"0:dist_from_lead=2"`, `"+1:qed=true"`). These string keys are produced independently by Rust at inference time and by Python at training time.
- **Risk:** If a developer alters feature naming, threshold buckets, or whitespace trimming in one language without mirroring it identically in the other, model inference will silently encounter missing keys and degrade to default transitions without error or panic.
- **Guardrail:** The unit tests `crf::tests::feature_functions_parity` and `crf::tests::boundary_scope_feature_keys_exact` in `crates/math-core/src/crf.rs` (mirrored by `ml/tests/test_features.py`) enforce parity across identical test inputs.

### Compile-Time Model Embedding & Panic on Corrupt Weights
- **Canonical Location:** [`crates/math-core/src/crf.rs`](../crates/math-core/src/crf.rs#L19,L403-L410)
- **Code Reference:**
  ```rust
  pub const EMBEDDED_WEIGHTS_JSON: &str = include_str!("crf_weights.json");
  // ...
  let model = CrfModel::from_json(EMBEDDED_WEIGHTS_JSON)
      .expect("embedded crf_weights.json must be valid");
  ```
- **Constraint:** Model weights (~50 KB JSON) are statically included at compile time via `include_str!`. Parsing occurs lazily on first access inside `std::sync::LazyLock`.
- **Risk:** If a broken or malformed weights file is committed and compiled, compilation succeeds, but the application will panic on the first call to `CrfTagger::new()`.
- **Mitigation:** CI runs `cargo test --workspace`, which exercises `CrfModel::embedded_shared()` during test suite execution.

### Synchronous Stdio Subprocess in CommandJudge Without Process Timeout
- **Canonical Location:** [`crates/math-judge/src/lib.rs`](../crates/math-judge/src/lib.rs#L220-L250)
- **Constraint:** When `CommandJudge::run` executes an external decision model binary or script via `--judge CMD`, it pipes `JudgeRequest` JSON to stdin from a background thread and synchronously reads stdout lines via `BufReader::lines()`.
- **Risk:** If the external command deadlocks, hangs, or fails to terminate on EOF, Mathia will block indefinitely on `BufReader::lines()`. There is currently no configurable timeout on external process execution.
- **Status:** `[NEEDS CLARIFICATION]` on whether to integrate an OS-level timeout (e.g., using `wait_timeout` or polling) for external command judges.

---

## 3. Structural Constraints & Edge Cases

### Top-Level List Flattening in Markdown AST Parser
- **Canonical Location:** [`crates/math-core/src/parser.rs`](../crates/math-core/src/parser.rs#L128-L160)
- **Constraint:** Top-level lists are parsed into discrete `NodeKind::ListItem` nodes. This design is necessary so that a bulleted label (e.g., `* **Lemma 1.** Let G be a graph...`) can open a semantic block without swallowing subsequent list items as part of its body.
- **Edge Case:** In deeply nested Markdown lists (lists within lists or blockquotes within lists), the inner elements are flattened into the outer item's rendered HTML. While display formatting in the browser is maintained, fine-grained semantic splitting within nested sub-lists is not supported.

### Heading Level Clamping in HTML & MindMap Extraction
- **Canonical Locations:**
  - [`crates/math-core/src/html.rs`](../crates/math-core/src/html.rs#L428-L432)
  - [`crates/math-core/src/mindmap.rs`](../crates/math-core/src/mindmap.rs#L54-L71)
- **Constraint:** Heading blocks are clamped between levels 1 and 6 (`b.heading_level.unwrap_or(2).clamp(1, 6)`). When documents contain multiple `# H1` headings, or start with semantic blocks prior to any `# H1` heading, the mindmap generator synthesizes a virtual `Root` node (`id: "mx-main"`, level 0) using the document title or source filename to prevent corruption of the heading navigation hierarchy.

---

## 4. Historical Planning Artifacts

- **Canonical Locations:** `docs/archive/PLAN.md`, `docs/archive/phase2-spec.md`
- **Context:** These documents reflect historical development phases (e.g., the initial Phase 2 interactive AI specification). While valuable for historical design provenance, they reference intermediate states (such as older provider lists or proposed features that were refined).
- **Rule for Future Maintainers & AI Agents:** Always treat [`docs/ARCHITECTURE.md`](ARCHITECTURE.md) and current source code as canonical. Do not treat archived specs as active requirements.
