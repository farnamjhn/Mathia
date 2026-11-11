# Mathia — plan

Mathia compiles OCR-transcribed Markdown (Persian/English mathematics with LaTeX) into
one self-contained, interactive HTML study guide.

## Where things stand

Done (Task 1 — the foundation, all in `crates/math-core`, `cargo test` green):

| Piece | File | Notes |
|---|---|---|
| Workspace | `Cargo.toml` | members `crates/math-core`, `crates/mathia-cli`, `src-tauri`; the app is not a default member, so plain `cargo build` / `cargo test` do not need Tauri |
| Domain model | `src/model.rs` | `Document`, `SemanticBlock`, `BlockType`, `TocEntry`, `Node`, `Direction` |
| Config | `src/config.rs` | `ScoringConfig`: threshold, weights, keyword tables, cue lists, scope limits — all data, serde-ready |
| Normaliser | `src/normalize.rs` | OCR artefacts → clean Markdown, before parsing |
| Parser | `src/parser.rs` | `pulldown-cmark` with `ENABLE_MATH`; one `Node` per top-level Markdown element; LaTeX environments; raw HTML escaped |
| Features | `src/features.rs` | lead keyword + label, mentions, math density, logic symbols, QED ending, cues |
| Classifier | `src/classifier.rs`, `src/crossref.rs` | scoring → openers → scoping → proof links → reference links; `ScopeOverrides`, `BlockJudge`, `classify_with_judge` |
| HTML | `src/html.rs`, `templates/document.html`, `assets/` | `askama` template, callouts, sidebar TOC, `data-block-id` hooks, micro-runtime |
| Decision models | `crates/math-judge` | JSON judge contract, `CommandJudge`, `CachedJudge`, `Policy`, `Replay` (Phase 1b) |
| CLI | `crates/mathia-cli/src/main.rs` | `mathia in.md\|folder/ [-o out] [--watch] [--config F] [--explain] [--font F\|--no-font] [--katex-dir DIR] [--ai…] [--judge CMD …\|--overrides F]`; no Tauri dependency |
| Desktop app | `src-tauri`, `ui/` | Tauri v2 shell, package `mathia-app` (Phase 3) |

Decisions taken (change here first if they are revisited):

- **Math rendering:** full KaTeX inlined into every output (`MathAssets::Inline`), fonts as
  base64 WOFF2. `MathAssets::Cdn` exists for development only.
- **Block scope:** an opener absorbs following nodes until a terminator (QED marker, next
  opener, heading, size cap). Scope is an explicit node range (`SemanticBlock::span`).
- **Block types:** Theorem, Lemma, Proposition, Corollary, Definition, Proof, Example, Remark,
  Exercise, Solution, Claim, Notation (+ structural Heading, Text).
- **Raw HTML** in the source is always escaped, never passed through.
- **Persian digits** are kept as written in the output; they are folded only for matching.
- **`src-tauri`:** the Tauri app (`mathia-app`). The CLI moved to `crates/mathia-cli` and
  keeps the package and binary name `mathia`, so `cargo run -p mathia` is unchanged.
- **Desktop frontend:** plain HTML/CSS/JS in `ui/`, no framework, no bundler, no npm
  (`withGlobalTauri`). Revisit if the inspector/config editor outgrow it.
- **Desktop file access:** the frontend has no fs/dialog/shell permission. Dialogs are opened
  by Rust commands; a path is read only after the user picked or dropped it
  (`Session::grant`). App commands are under the ACL (`AppManifest` in `build.rs`).
- **Preview:** the compiled HTML is served to an `<iframe>` by the `mathia-preview` URI
  scheme, so it keeps its own origin and its own CSP (a `srcdoc`/`blob:` frame would inherit
  the app's CSP and lose the hashed inline scripts).
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
   - [x] Optional static decision tree over the same `Features` (`src/tree.rs`), selected with
     `ScoringConfig::scorer` (`[scorer] model = "tree"`; without a `root` the built-in tree).
     Tests are yes/no questions relative to the candidate type, leaves hold scores; validated
     (scores and thresholds in [0, 1], depth ≤ 32). The default tree holds the same gold floor
     (1.00 on the corpus, `decision_tree_precision_and_recall_per_type`). Keyword-less proofs
     still use `weights.after_statement` with either scorer.
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
   - [x] Vendor and subset Vazirmatn (`scripts/vendor-fonts.sh`: v33.003, SHA-256 checked,
     variable font subset to Arabic script + Latin by a pinned fontTools in a throwaway venv,
     84 KB WOFF2, reproducible). Cargo feature `bundled-fonts` / `EmbeddedFont::bundled()`;
     default in the CLI (`--font` replaces it, `--no-font` drops it) and in the app.

## Phase 1b — decision models (jev, clef, laya, …)

The seam: `BlockJudge::judge(&SemanticBlock, &[Node]) -> Verdict` (plus `judge_batch`), driven
by `HeuristicClassifier::classify_with_judge[_detailed]`. Implementations live **outside**
`math-core`, in `crates/math-judge`.

1. [x] New crate `crates/math-judge` (may depend on model runtimes; `math-core` must not).
   Today it needs only `serde_json`.
2. [x] Judge contract: one for every model. `JudgeRequest` (block text with LaTeX restored,
   two nodes of context each side, proposed type, node range, `proves`, heuristic score) and
   `JudgeResponse` (verdict, optional confidence), JSON. `CommandJudge` runs any program and
   speaks JSON Lines over stdin/stdout, so a model plugs in as a wrapper script; a failing or
   silent program means `Accept`. No adapter for jev, clef or laya is written yet.
3. [x] `Verdict::Retype(BlockType)`; `ScopeOverrides::retype` (also forces an opener on a node
   that would not open one). `ScopeOverrides` and `Verdict` are serde types.
4. [x] Smarter rescoping: `Verdict::EndAt(node)` (a split point, or a wider scope) and
   `Verdict::StartAt(node)`; `ScopeOverrides::end` may run through later openers (they are
   absorbed; headings and environments still stop it). Environments keep their extent.
5. [x] `CachedJudge`: verdicts keyed by a stable 128-bit FNV-1a hash of (model version, type,
   scope + text), one batch per round for the misses only, saved as JSON. CLI:
   `--judge CMD --judge-model ID --judge-cache FILE`.
6. [x] Policy: a block is judged once per distinct (scope, type), so contradicting or blocked
   verdicts cannot loop; `max_rounds` still caps it; `Policy::min_confidence` (default 0.5,
   `--min-confidence`) turns unsure answers into `Accept`; unusable verdicts are logged with
   `applied: false`. Every verdict is logged (`AppliedVerdict`), and the final overrides
   replay the document without the model (`Replay`, `--judge-log` → `--overrides`).

## Phase 2 — interactive runtime (inside the compiled HTML)

Done. Contract: `docs/archive/phase2-spec.md`. Opt-in: nothing below is embedded unless the document
is compiled with `--ai` / `--ai-provider` / `--ai-custom` (`HtmlOptions::ai`); without them
the output is unchanged. Code: `assets/ai.js`, `assets/ai.css`; API: `window.Mathia.ai`.

1. [x] **Embedding and CSP** (`html.rs`). `AiOptions { providers, custom_providers }` adds
   the second script (allowed by hash), `connect-src` limited to the enabled provider
   origins, and `body[data-mx-ai]`. Rust still has no network or LLM code.
2. [x] **API key handling.** Settings dialog (gear button): provider, model, endpoint for
   custom providers, key. Stored in `localStorage` (`mathia-ai`) only, never written into
   the file, sent only in request headers; "Forget key" / "Forget all". Providers:
   Anthropic, OpenAI, Gemini, and any OpenAI-compatible endpoint (key optional for local
   ones), all called directly from the browser with streaming.
3. [x] **Ask about this block.** Per-block panel; first turn sends the block text (LaTeX
   restored) plus, optionally, the neighbouring blocks and the statement a proof proves;
   the streamed answer is rendered by a DOM-only Markdown + KaTeX renderer (no `innerHTML`
   anywhere); conversation kept per block in `sessionStorage`.
4. [x] **Explanatory SVG.** Per-block "Illustrate". The returned SVG goes through
   `sanitizeSvg` (element and attribute allowlists, only `#fragment` references) and is
   shown through `<img src="data:…">`, so it cannot run script or load anything even if the
   sanitiser missed something. Cached in `localStorage` per block and block-text hash;
   cached entries are sanitised again on load.
5. [x] **Offline behaviour.** `html.mx-ai-offline` hides the settings button and every AI
   action; an open panel says it is offline and disables input. Cached figures stay visible.
6. [x] **Tests.** `tests/js` (`npm test`): `playwright-core` driving the system Chrome
   against documents compiled by the real CLI, all provider traffic mocked, every test
   asserting no CSP violations and no un-mocked requests. Run in CI.

Not done / known limits:

- No request has been made against a real provider from these tests; the wire formats
  follow the spec and are only checked against mocks. Default model names are free text in
  the dialog and will age.
- Figures cannot contain rendered LaTeX (labels are plain text).
- One conversation per block; nothing is shared between blocks or exported.

## Phase 3 — Tauri v2 desktop app (`src-tauri`)

Started. Run with `cargo run -p mathia-app [-- notes.md]`; tests `cargo test -p mathia-app`.

1. [x] **Real Tauri v2 crate.** `tauri` 2.12, `tauri-build`, `tauri-plugin-dialog`,
   `build.rs`, `tauri.conf.json` (app CSP: `default-src 'self'`, frames only from
   `mathia-preview`), `capabilities/default.json` (events + Mathia's own commands, nothing
   else), placeholder icons in `icons/`.
2. [x] **Commands** (`src/main.rs`; the logic is in `src/session.rs` without Tauri types
   and unit-tested): `pick_markdown`, `startup_file`, `analyze(source)`,
   `compile(source, options)`, `export(source, options)`, `get/set/reset_config`,
   `load/save_config`. `source` is `{path}` or `{text}`. Differences from the first sketch:
   `compile` stores the HTML for the preview protocol and returns
   `{revision, changed, semantic_blocks, bytes}` instead of sending megabytes over IPC;
   `export` and the config commands open their own dialogs instead of taking a destination.
3. **Frontend** (`ui/`).
   - [x] Open (dialog, drag and drop, command-line argument), preview of the compiled HTML,
     recompile on Reload and when the window regains focus, AI-runtime / no-math switches,
     export.
   - [x] Block inspector, read-only: type, score, node range, end reason, `proves`, opener
     scores per type, features. Selecting a block scrolls the preview to it.
   - [x] Manual retype / rescope writing `ScopeOverrides`: type select, "− node" / "+ node"
     (explicit `end`), "Not a block", "Undo"; "Reset all", and Save… / Load… as JSON (a
     `--judge-log` file loads too). Commands `set_overrides`, `save_overrides`,
     `load_overrides`; `analyze` / `compile` apply them and report overrides that no longer
     fit the file. Keyed by node index, so they belong to one revision of one file; opening
     another file clears them.
   - [x] Scoring-config editor (**Scoring…**): scorer, threshold, weights, scope limits,
     keywords, cue lists; Apply, Defaults, Load…, Save…. A custom tree loaded from a file is
     kept but cannot be edited in the form.
   - [x] Selecting a block in the preview selects it in the inspector: `runtime.js` posts
     `{mathia: "select", id}` to its parent when the page is framed (links, buttons,
     formulas and selections excluded); the app accepts it only from its preview frame.
4. [x] Assets, batch, watching.
   - KaTeX stays compiled in (`bundled-katex`) rather than a Tauri resource: the HTML must
     be self-contained anyway, and the CLI shares the code path. Vazirmatn likewise
     (`bundled-fonts`).
   - [x] Batch compilation of a folder: **Export folder…** (`export_folder`, two folder
     dialogs, corrections not applied) and `mathia folder/ -o out/`.
   - [x] File watching: the app polls `source_stamp` (mtime + size) every second while
     visible and recompiles on change; `mathia --watch` polls its inputs.
5. Packaging and CI.
   - [x] CI (`.github/workflows/ci.yml`): vendor scripts, `cargo fmt --check`,
     `clippy --workspace --all-targets -D warnings`, `cargo test` for `math-core` alone and
     for the workspace, and the headless-Chrome tests.
   - [x] Release workflow (`release.yml`, on `v*` tags): `cargo tauri build` on macOS (arm64,
     x86_64), Windows and Linux, bundles attached to a draft release. Signing / notarisation
     happen only if the Apple secrets are set. Neither workflow has run yet.
   - [ ] Code-signing certificates (Apple Developer ID, Windows) and their secrets.
   - [ ] Auto-update: needs `tauri-plugin-updater`, an update key pair (the private key is a
     secret), and a place to host `latest.json`.
   - [ ] Real identifier and icon (`dev.mathia.desktop` and the icon are placeholders).

Checked so far: unit tests, `clippy --workspace`, and the debug build on macOS opening a
file given as an argument (preview and inspector render). The new frontend parts (inspector
corrections, scoring dialog, preview → inspector selection) were driven in headless Chrome
against a stubbed Tauri API, and the frame message by a runtime test; they have not been
clicked through in the real app. Not exercised: the dialogs, drag and drop, export, folder
export, Windows and Linux.

Known limit: Tauri treats a registered URI scheme as a local origin, so the ACL does not by
itself separate the preview frame from the main window. What protects the commands is that
the compiled document can only run its own hashed scripts (its CSP), and that no command
reads a path the user did not choose or writes without a save dialog.

## Open questions

- One HTML per Markdown file, or multi-file books with a shared TOC?
- In-page AI requests go direct from the browser today. Should the desktop app offer a
  relay (keys in the OS keychain instead of `localStorage`, providers without CORS)?
