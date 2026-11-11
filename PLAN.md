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
| Normaliser | `src/normalize.rs` | OCR artefacts → clean Markdown, before parsing |
| Parser | `src/parser.rs` | `pulldown-cmark` with `ENABLE_MATH`; one `Node` per top-level Markdown element; LaTeX environments; raw HTML escaped |
| Features | `src/features.rs` | lead keyword + label, mentions, math density, logic symbols, QED ending, cues |
| Classifier | `src/classifier.rs`, `src/crossref.rs` | scoring → openers → scoping → proof links → reference links; `ScopeOverrides`, `BlockJudge`, `classify_with_judge` |
| HTML | `src/html.rs`, `templates/document.html`, `assets/` | `askama` template, callouts, sidebar TOC, `data-block-id` hooks, micro-runtime |
| CLI stand-in | `src-tauri/src/main.rs` | `mathia in.md [-o out.html] [--config F] [--explain] [--font F] [--katex-dir DIR]`; no Tauri dependency yet |

Decisions taken (change here first if they are revisited):

- **Math rendering:** full KaTeX inlined into every output (`MathAssets::Inline`), fonts as
  base64 WOFF2. `MathAssets::Cdn` exists for development only.
- **Block scope:** an opener absorbs following nodes until a terminator (QED marker, next
  opener, heading, size cap). Scope is an explicit node range (`SemanticBlock::span`).
- **Block types:** Theorem, Lemma, Proposition, Corollary, Definition, Proof, Example, Remark,
  Exercise, Solution, Claim, Notation (+ structural Heading, Text).
- **Raw HTML** in the source is always escaped, never passed through.
- **Persian digits** are kept as written in the output; they are folded only for matching.
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
label, emphasis, or supporting features. A label is punctuation, a `(name)` or a title-only
line — a bare number is not ("Theorem 2 gives…" is prose).

## Phase 1 — finish the deterministic compiler

1. [x] **Vendor KaTeX.** Added `scripts/vendor-katex.sh` that fetches pinned KaTeX v0.16.22 into
   `crates/math-core/assets/katex/` (git-ignored) with SHA-256 verification and inlines WOFF2 fonts
   into `katex.prepared.css`. Added `bundled-katex` cargo feature with `KatexAssets::bundled()`,
   `MathAssets::bundled()`, and automatic fallback to vendored assets in `mathia` CLI.
   Tests verify that inline/bundled output contains zero external HTTP/HTTPS network URLs.
2. [x] **OCR input normalisation.** `src/normalize.rs`, run by `analyze` before the parser
   (offsets in `Node::source` refer to the normalised text). Passes, all skipping code and
   existing `$` math: `\( … \)` / `\[ … \]` → dollars (only when the content looks like
   math, so `\[1\]` citations survive); bare `equation|align|alignat|gather` wrapped in `$$`;
   prose-level `\\` → hard break; spaced bold markers (`* *قضیه* *`, `** قضیه **متن`);
   unbalanced `$` / `$$`. Idempotent. Digits in the output are kept as written.
3. [x] **LaTeX theorem environments.** The parser cuts `\begin{theorem}[Name] … \end{theorem}`
   (names in `BlockType::from_env_name`) out of the source; the first body node carries an
   `EnvSpan`, and the classifier turns it into a score-1.0 block with exactly that scope
   (`EndReason::Environment`). Not nested; `\begin` must start its line.
4. **Classifier coverage.**
   - [x] Reference weak spot: a bare number is no longer a label, and a verb right after the
     label (`reference_cues`: "نتیجه می‌دهد", "implies", "is", …) demotes the lead to a mention.
   - [x] New types `Exercise`, `Solution`, `Claim`, `Notation` (keywords, labels, env names).
     A solution without a QED mark is scoped like a statement.
   - [x] Keyword-less proofs: a paragraph directly after a statement scores
     `after_statement 0.25 + qed + logic + cue`, with the QED looked for up to the next boundary.
   - [x] `SemanticBlock::proves`: by "Proof of Theorem 2" reference, else adjacency (plain
     text may sit in between); solutions link to their example/exercise. HTML: `data-proves`,
     a back-link in the block head, `Mathia.block(id).proves`.
   - [ ] Optional static decision tree over the same `Features` as an alternative to the linear
     scorer, selected in `ScoringConfig`.
5. [x] **Config I/O.** `ScoringConfig::{parse, serialize, load, save, validate}` for TOML and
   JSON, `schema_version` (currently 1; newer files are rejected), unknown fields rejected,
   partial files keep defaults. CLI: `--config FILE`, `--dump-config`.
6. [x] **Diagnostics.** `analyze_detailed` returns `Analysis { document, scores, block_of }`
   (all type scores per node); every block records `end: EndReason`. `Analysis::explain` /
   `mathia --explain` print the table.
7. [x] **Corpus tests.** `tests/corpus.rs`: snapshots in `tests/snapshots/*.snap`
   (`MATHIA_UPDATE_SNAPSHOTS=1` regenerates) and hand-labelled `tests/fixtures/*.gold` files
   giving precision/recall per type (floor 0.9, currently 1.00 on 33 blocks).
   - [ ] The four fixtures are written by hand in OCR style. Replace/extend with real OCR
     output; that is where the floor starts to mean something.
8. [x] **HTML hardening.** Raw HTML in the Markdown is shown as text; link/image URLs are
   limited to http(s), mailto, relative and `data:image/`; `</script` / `</style` escaped in
   every inlined asset; `Content-Security-Policy` meta tag with `default-src 'none'` and
   inline scripts allowed by SHA-256 hash only (`src/sha256.rs`).
9. **Typography.**
   - [x] Cross-reference links (`src/crossref.rs`): "طبق قضیه ۳.۳" → `<a class="mx-ref">`.
   - [x] Numbered equations: `equation` → `equation*` + `\tag{n}` across the document, with
     `\label` / `\eqref` / `\ref` resolved. `align` / `gather` are still numbered by KaTeX
     per formula (each restarts at 1).
   - [x] Print stylesheet, collapsible TOC sections, light/dark switch (remembered in
     `localStorage`). Not yet checked in a real browser — only syntax-checked.
   - [x] `HtmlOptions::fonts` / `mathia --font F.woff2` embed a WOFF2 text font.
   - [ ] Vendor and subset Vazirmatn (`scripts/vendor-fonts.sh`, pinned + checksummed like
     KaTeX) and make it the bundled default.

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

- One HTML per Markdown file, or multi-file books with a shared TOC?
- Which providers must the in-page AI features support first, and may requests go direct
  from the browser (CORS) or do they need the desktop app as a relay?
