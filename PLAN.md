# Mathia — plan

Mathia compiles OCR-transcribed Markdown (Persian/English mathematics with LaTeX) into
one self-contained, interactive HTML study guide.

## Where things stand

Done (Task 1 — the foundation, all in `crates/math-core`, `cargo test` green):

| Piece | File | Notes |
|---|---|---|
| Workspace | `Cargo.toml` | members `crates/math-core`, `src-tauri`; shared dependency versions |
| Domain model | `src/model.rs` | `Document`, `SemanticBlock`, `BlockType`, `TocEntry`, `Node`, `Direction` |
| Config | `src/config.rs` | `ScoringConfig`: threshold, weights, keyword tables, cue lists, scope limits — all data, serde-ready |
| Parser | `src/parser.rs` | `pulldown-cmark` with `ENABLE_MATH`; one `Node` per top-level Markdown element |
| Features | `src/features.rs` | lead keyword + label, mentions, math density, logic symbols, QED ending, cues |
| Classifier | `src/classifier.rs` | scoring → openers → scoping; `ScopeOverrides`, `BlockJudge`, `classify_with_judge` |
| HTML | `src/html.rs`, `templates/document.html`, `assets/` | `askama` template, callouts, sidebar TOC, `data-block-id` hooks, micro-runtime |
| CLI stand-in | `src-tauri/src/main.rs` | `mathia in.md [-o out.html] [--katex-dir DIR]`; no Tauri dependency yet |

Decisions taken (change here first if they are revisited):

- **Math rendering:** full KaTeX inlined into every output (`MathAssets::Inline`), fonts as
  base64 WOFF2. `MathAssets::Cdn` exists for development only.
- **Block scope:** an opener absorbs following nodes until a terminator (QED marker, next
  opener, heading, size cap). Scope is an explicit node range (`SemanticBlock::span`).
- **Block types:** Theorem, Lemma, Proposition, Corollary, Definition, Proof, Example, Remark
  (+ structural Heading, Text).
- **`src-tauri`:** placeholder binary until Phase 3.
- **Libraries:** `pulldown-cmark` (byte offsets and native `$`/`$$` events) and `askama`
  (templates checked at compile time, no runtime template loading).
- **`math-core` rules:** no Tauri, no network, no LLM, no randomness. Same input and config
  always give the same `Document`.

How classification works, in one paragraph: each node gets a feature vector; the score for a
type is a weighted sum (`lead 0.45 + label 0.20 + emphasis 0.15`, or `mention 0.10`, plus
type-specific terms: QED/logic/cue for proofs, definitional cue/math for definitions,
math/logic/cue for statements). A paragraph, heading or block quote whose best score reaches
`threshold` (0.5) opens a block. A bare keyword (0.45) is deliberately not enough: it needs a
label, emphasis, or supporting features.

## Phase 1 — finish the deterministic compiler

1. [x] **Vendor KaTeX.** Added `scripts/vendor-katex.sh` that fetches pinned KaTeX v0.16.22 into
   `crates/math-core/assets/katex/` (git-ignored) with SHA-256 verification and inlines WOFF2 fonts
   into `katex.prepared.css`. Added `bundled-katex` cargo feature with `KatexAssets::bundled()`,
   `MathAssets::bundled()`, and automatic fallback to vendored assets in `mathia` CLI.
   Tests verify that inline/bundled output contains zero external HTTP/HTTPS network URLs.
2. **OCR input normalisation** (new `src/normalize.rs`, run before the parser):
   - `\( … \)` → `$ … $` and `\[ … \]` → `$$ … $$` (Mathpix-style delimiters);
   - `\begin{equation|align|gather}` environments outside `$$` wrapped as display math;
   - unbalanced `$`, stray `\\`, and broken bold markers (`* *قضیه* *`);
   - Persian/Arabic digit and letter variants are already folded for matching (`text.rs`);
     decide whether to also normalise them in the output.
3. **LaTeX theorem environments.** Treat `\begin{theorem} … \end{theorem}` (and `proof`,
   `definition`, `lemma`, …) as explicit, score-1.0 blocks with exact scope.
4. **Classifier coverage.**
   - Known weak spot: an unpunctuated reference at the start of a paragraph
     ("قضیه ۲ نتیجه می‌دهد که…") scores as a labelled theorem. Add a feature for a verb/cue
     directly after the label, or lower `label` when no punctuation follows the number.
   - Solutions (`حل`, `Solution`) attached to examples; exercises (`تمرین`); claims; notation.
   - Proofs with no keyword but a strong QED/logic profile directly after a statement
     ("previous block is a statement" as a feature).
   - Link each proof to the statement it proves (`proves: Option<BlockId>`), by adjacency and
     by "Proof of Theorem 2" references; expose it in HTML as `data-proves`.
   - Optional static decision tree over the same `Features` as an alternative to the linear
     scorer, selected in `ScoringConfig`.
5. **Config I/O.** Load/save `ScoringConfig` as TOML/JSON (types already derive serde), with
   validation (threshold in `[0,1]`, non-empty keywords) and a versioned schema.
6. **Diagnostics.** `analyze` returns per-node scores and the reason a block ended, so the UI
   can show "why is this a theorem" and so thresholds can be tuned on real files.
7. **Corpus tests.** Collect real OCR output into `tests/fixtures/`, store expected
   classifications as snapshot files, and track precision/recall per type.
8. **HTML hardening.** Sanitise raw HTML coming from the Markdown (or disable it), escape
   `</script` in all inlined assets (done for KaTeX), add a restrictive `Content-Security-Policy`
   meta tag compatible with the runtime.
9. **Typography.** Embed a subset Persian font (Vazirmatn) as WOFF2; cross-reference links
   ("طبق قضیه ۳.۳" → `#block-theorem-…`); numbered equations; print stylesheet polish;
   nested, collapsible TOC sections; optional theme switch.

## Phase 1b — decision models (jev, clef, laya, …)

The seam exists: `BlockJudge::judge(&SemanticBlock, &[Node]) -> Verdict`, driven by
`HeuristicClassifier::classify_with_judge`. A verdict of `Incomplete` / `Overreach` widens /
narrows the block by one node and re-runs; `Reject` drops the opener. Implementations must
live **outside** `math-core`.

1. New crate `crates/math-judge` (may depend on model runtimes; `math-core` must not).
2. Decide the judge contract per model: input (block text with LaTeX restored, neighbours,
   proposed type), output (`Verdict`, optionally a confidence and a suggested type).
3. Extend `Verdict` with `Retype(BlockType)` and extend `ScopeOverrides` to match.
4. Smarter rescoping than ±1 node: let the judge name a split point; allow widening through
   a suppressed opener (today a hard boundary); move the start of a block, not only its end.
5. Batch and cache verdicts keyed by a hash of (block text, type, model version) so
   recompiling an unchanged document is free and reproducible.
6. Policy for disagreement and for non-terminating judges (`max_rounds` exists; add logging
   of every override applied so a compile can be replayed without the model).

## Phase 2 — interactive runtime (inside the compiled HTML)

The compiled file already exposes `window.Mathia` (`blocks()`, `block(id)`,
`registerAction({id, label, run, types})`, `renderMath()`) and an empty action slot in every
semantic block (`[data-block-actions]`).

1. **API key handling:** user pastes their own key into the page; stored in `localStorage`
   only, never written into the file; clear "forget key" control; provider selection.
2. **Ask about this block:** per-block panel that sends the block text (LaTeX restored via
   `block.text`) plus optional neighbouring context; streamed answer rendered as Markdown +
   KaTeX; conversation kept per block.
3. **Explanatory SVG:** per-block "illustrate" action; the returned SVG must be sanitised
   (no scripts, no external references) before insertion; cache per block in `localStorage`.
4. Offline behaviour: all of the above degrade to hidden controls with no network.
5. Keep the runtime dependency-free vanilla JS, embedded from `assets/runtime.js`; add a
   small JS test harness (headless browser) for it.

## Phase 3 — Tauri v2 desktop app (`src-tauri`)

1. Replace the placeholder with a real Tauri v2 crate: `tauri`, `tauri-build`, `build.rs`,
   `tauri.conf.json`, capabilities limited to file dialogs and scoped fs access.
2. Commands (thin wrappers over `math-core`): `analyze(path|text) -> Document`,
   `compile(path, options) -> html`, `export(path, dest)`, `load/save_config`.
3. Frontend (choose a framework at that point): open/drag-drop Markdown, live preview of the
   compiled HTML, block inspector (type, score, features, scope), manual retype/rescope that
   writes `ScopeOverrides`, scoring-config editor, export.
4. Bundle KaTeX and fonts as Tauri resources; batch compilation of a folder; file watching.
5. Packaging, signing and auto-update for macOS/Windows/Linux; CI running
   `cargo test -p math-core` and `cargo clippy --workspace`.

## Open questions

- Should Persian digits in labels be normalised in the output, or always kept as written?
- One HTML per Markdown file, or multi-file books with a shared TOC?
- Is raw HTML inside the OCR Markdown ever wanted, or should it always be escaped?
- Which providers must the in-page AI features support first, and may requests go direct
  from the browser (CORS) or do they need the desktop app as a relay?
