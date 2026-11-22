# Mathia System Architecture & Technical Specification

Mathia is a high-performance, deterministic compiler and desktop study guide application. It transforms raw, OCR-transcribed Markdown and LaTeX documents (Persian, English, or mixed bilingual) or imported HTML files into single, self-contained, interactive HTML study guides. It can also standardize and repair OCR transcription errors or convert compiled documents back into structured Markdown.

This document serves as the authoritative architectural blueprint for both human engineers and AI coding agents navigating, modifying, or extending the codebase.

---

## 1. System Overview & Invariants

Mathia is designed around strict architectural invariants to guarantee determinism, zero-dependency deployment, and client-side security:

1. **Pure, Deterministic Core (`crates/math-core`)**:
   - The core compilation engine contains **no network calls, no asynchronous runtimes, no local server daemons, and no direct LLM integrations**.
   - Identical inputs with the same configuration always yield byte-identical abstract structures.
   - External decision models interact exclusively through explicit, optional trait boundaries ([`BlockJudge`](../crates/math-core/src/classifier.rs)).
2. **Zero-Inference Runtime Bloat**:
   - Machine learning is applied via a Linear-Chain Conditional Random Field (CRF).
   - Ingestion and inference run 100% in pure Rust using Viterbi dynamic programming. Model weights (`crf_weights.json`, ~50 KB) are embedded at compile time using `include_str!`.
   - The desktop app bundles no ONNX runtime, no Python runtime, no PyTorch binaries, and no local LLM weights.
3. **Hard Security & Isolation Boundary**:
   - Compiled HTML study guides enforce a strict Content Security Policy (CSP). Only scripts embedded by Mathia itself are permitted, matched via SHA-256 hashes (`'sha256-...'`).
   - Network access is blocked by default (`connect-src 'none'`). When `--ai` features are explicitly requested, `connect-src` is strictly scoped to designated provider origins.
   - Interactive LLM interactions (Ask & Illustrate) occur purely client-side in the reader's browser via [`assets/ai.js`](../crates/math-core/assets/ai.js). API keys are stored in the client browser's `localStorage` and are never embedded into compiled files or sent to Mathia binaries.
4. **Resilient Edit Invariance**:
   - User overrides and model corrections do not bind to volatile absolute line numbers or raw node array indices.
   - Overrides bind via 64-bit content-anchored hashes (`ContentAnchor`: hash of normalized text + occurrence index), allowing manual annotations to survive upstream document editing.

---

## 2. Component & Layer Architecture

The repository is structured into four primary layers, plus an offline dev-time ML workspace:

```text
                               ┌────────────────────────────────────────────────┐
                               │           Desktop UI Layer                     │
                               │  src-tauri (Tauri v2) + ui/ (HTML5/CSS3/ES6)   │
                               │  - Custom URI Scheme: mathia-preview://        │
                               │  - IPC Commands: analyze, compile, export, fix │
                               └───────────────────────┬────────────────────────┘
                                                       │
                               ┌───────────────────────▼────────────────────────┐
                               │             CLI Front-End                      │
                               │           crates/mathia-cli                    │
                               │  - CLI flags, watch mode, feature dumper       │
                               └───────────────────────┬────────────────────────┘
                                                       │
                           ┌───────────────────────────┴───────────────────────────┐
                           │                                                       │
┌──────────────────────────▼─────────────────────────┐         ┌───────────────────▼──────────────────┐
│             Decision Model Judge Seam              │         │            Pure Core Engine          │
│                crates/math-judge                   │         │            crates/math-core          │
│  - Trait BlockJudge implementation                 │         │  - Normalization & OCR repair        │
│  - Stdio JSON-Lines Subprocess (CommandJudge)      │◄───────►│  - AST Parser (pulldown-cmark)       │
│  - REST API Client (ApiJudge: Jev, Claude, OpenAI) │ (seam)  │  - Static Feature Extraction         │
│  - SHA-256 Content-Keyed Cache (CachedJudge)       │         │  - SequenceTagger (Heuristic / CRF)  │
│  - Replay Logs & Override Generation               │         │  - HTML5 & Markdown Compilers        │
└────────────────────────────────────────────────────┘         └───────────────────▲──────────────────┘
                                                                                   │
                                                                                   │ embedded weights
                                                                                   │ (compile-time)
                                                               ┌───────────────────┴──────────────────┐
                                                               │       Dev-Only ML Flywheel           │
                                                               │         ml/ (Python + uv)            │
                                                               │  - Synthetic OCR dataset generator   │
                                                               │  - sklearn-crfsuite (L-BFGS + L1/L2) │
                                                               │  - seqeval exact-span F1 benchmark   │
                                                               │  - Exports crf_weights.json          │
                                                               └──────────────────────────────────────┘
```

### End-to-End Data Flow

The complete compilation pipeline processes input text through deterministic transformation phases:

```mermaid
flowchart TD
    In["Raw Input (Markdown / LaTeX or HTML)"] --> CleanCheck{"HTML or MD?"}
    
    %% HTML Path
    CleanCheck -- "HTML" --> HTMLClean["clean_html::clean_html() / analyze_html_detailed()"]
    HTMLClean --> ASTClean["DOM Parser & Sanitizer (html5ever / regex)"]
    
    %% Markdown Path
    CleanCheck -- "Markdown" --> Norm["normalize::normalize_markdown() & repair::wrap_bare_math()"]
    Norm --> Parse["parser::parse_markdown()"]
    Parse --> AST["Flat AST Top-Level Nodes (Vec<Node>)"]
    
    ASTClean --> AST
    
    %% Feature & Tagging
    AST --> Feat["features::Lexicon::extract() -> Vec<NodeFeatures>"]
    Feat --> TagBranch{"Sequence Tagger"}
    
    TagBranch -- "Linear / DecisionTree" --> HeurTag["tagger::HeuristicTagger::tag()"]
    TagBranch -- "CRF Model" --> CrfTag["crf::CrfTagger::decode_nodes() (Viterbi Trellis)"]
    
    HeurTag --> BioTags["BIO Sequence Tags (Vec<TaggedNode>)"]
    CrfTag --> BioTags
    
    BioTags --> TagsToBlocks["classifier::HeuristicClassifier::tags_to_blocks()"]
    
    %% Overrides & Judge
    TagsToBlocks --> Overrides["classifier::ScopeOverrides::resolve() (ContentAnchor)"]
    Overrides --> JudgeCheck{"Judge configured & Low Confidence?"}
    
    JudgeCheck -- "No / High Confidence" --> Doc["model::Document::new(nodes, blocks)"]
    JudgeCheck -- "Yes (Escalate)" --> JudgeRun["math_judge::BlockJudge::judge_batch()"]
    JudgeRun --> CacheLookup{"Cached in CachedJudge?"}
    CacheLookup -- "Hit" --> ApplyVerdict["Apply Verdict (Retype / EndAt / Reject)"]
    CacheLookup -- "Miss" --> ExecJudge["CommandJudge (stdio) / ApiJudge (REST)"]
    ExecJudge --> RecordCache["Write to CachedJudge & Replay Log"]
    RecordCache --> ApplyVerdict
    ApplyVerdict --> Doc
    
    %% Rendering
    Doc --> OutChoice{"Output Target"}
    OutChoice -- "Standalone HTML" --> Askama["html::render() (Askama template: document.html)"]
    Askama --> CSP["Calculate SHA-256 CSP Hashes & Whitelist AI Origins"]
    CSP --> OutHTML["Standalone HTML5 File (Embedded KaTeX, Fonts, Runtime JS)"]
    
    OutChoice -- "Markdown Export" --> ExportMD["markdown::render()"]
    ExportMD --> OutMD["Standardized .md File (**Theorem 1.** + LaTeX envs)"]
```

---

## 3. Directory & Module Map

| Path | Primary Ownership | Permitted Dependencies | Forbidden Dependencies |
|---|---|---|---|
| `crates/math-core` | Parsing, AST representation, feature extraction, sequence classification, HTML/Markdown generation, mindmap tree extraction. | Standard library, `pulldown-cmark`, `askama`, `serde`, `serde_json`, `thiserror`, `regex`, `html5ever`. | `crates/math-judge`, `crates/mathia-cli`, `src-tauri`, networking libraries (`reqwest`), async runtimes (`tokio`), UI code. |
| `crates/math-judge` | External model contract (`JudgeRequest`/`JudgeResponse`), subprocess execution (`CommandJudge`), API judge (`ApiJudge`), caching (`CachedJudge`), replay logs. | Standard library, `crates/math-core`, `serde`, `serde_json`. | `src-tauri`, `crates/mathia-cli`, UI code, internal webview bindings. |
| `crates/mathia-cli` | Command-line binary (`mathia`), argument parsing, directory batching, `--watch` polling loop, `--dump-features` CLI. | `crates/math-core`, `crates/math-judge`, standard CLI dependencies. | `src-tauri`, webview libraries. |
| `src-tauri` | Desktop application lifecycle, window management, native file pickers, sandboxed preview protocol (`mathia-preview://`), session cache. | `crates/math-core`, `crates/math-judge`, `tauri`, `tauri-plugin-dialog`, `serde`. | Direct network client logic outside of `math-judge`. |
| `ui/` | Desktop UI frontend (Vanilla JS/CSS/HTML), block list inspector, manual correction tool, preview iframe coordinator. | Browser DOM APIs, Tauri IPC (`window.__TAURI__.core.invoke`). | Direct Node.js / OS filesystem access. |
| `ml/` | Dev-only Python flywheel for synthetic OCR data generation, L-BFGS CRF model training, validation tuning, and evaluation benchmarks. | Python 3.11+, `sklearn-crfsuite`, `scikit-learn`, `seqeval`, `pydantic`, `numpy`, `uv`. | Must never be bundled into release binaries or required at runtime. |

---

## 4. Abstract Syntax Tree (AST) & Node Architecture

The AST in Mathia is deliberately flat rather than deeply nested. This design allows sequence tagging algorithms (CRF Viterbi, greedy BIO) to operate over sequential node atoms without tree-traversal overhead.

### AST Node Definition (`crates/math-core/src/model.rs`)
The document consists of a top-level `Vec<Node>`:
- `kind: NodeKind`: Discriminates the structural role:
  - `Paragraph`: Ordinary running text.
  - `DisplayMath`: Paragraphs consisting exclusively of `$$ ... $$` math blocks.
  - `List` & `ListItem`: Top-level bulleted or numbered items. Lists are split into individual `ListItem` nodes by the parser so that an item like `- **Theorem 1.** ...` can open a block without absorbing sibling items.
  - `Heading(u8)`: Markdown headers `#` through `######` (levels 1–6).
  - `Quote`, `Code`, `Table`, `Rule`, `Other`.
- `text: String`: Raw prose stripped of LaTeX formulas, used for language detection and linguistic cues.
- `flat: String`: Prose with LaTeX formula sources spliced in at their original positions (delimited by spaces).
- `math: Vec<MathSpan>`: Isolated LaTeX mathematical expressions, recording the raw string and whether it was `display` (`$$`) or `inline` (`$`).
- `leading_emphasis: bool`: True if the node opens with `**...**` or `*...*` (the canonical format for OCR labels like `**Theorem 1.**`).
- `source: Range<usize>`: Exact byte offsets in the normalized Markdown source text.
- `env: Option<EnvSpan>`: Populated when the node originated from a LaTeX environment (`\begin{theorem} ... \end{theorem}`).

### Content Anchoring (`crates/math-core/src/classifier.rs`)
Manual overrides and decision verdicts must remain valid even when a user inserts paragraphs earlier in the document. Mathia implements **Content-Based Anchoring**:
1. When a block opener is identified, [`compute_node_anchor`](../crates/math-core/src/classifier.rs) extracts the first 64 characters of normalized prose from `node.flat`.
2. It computes a 64-bit deterministic hash using the internal pure-Rust SHA-256 implementation:
   $$\text{hash} = \text{u64::from\_be\_bytes}(\text{SHA256}(\text{normalized}[0..64])[0..8])$$
3. It tracks the occurrence index of that identical hash across prior nodes:
   $$\text{ContentAnchor} = \{ \text{hash: u64}, \text{occurrence: usize} \}$$
4. An [`OverrideKey`](../crates/math-core/src/classifier.rs) contains both `anchor: Option<ContentAnchor>` and `fallback: Option<usize>` (the original node index).
5. When resolving overrides against a modified document ([`ScopeOverrides::resolve`](../crates/math-core/src/classifier.rs)), the system matches the anchor first. If text was inserted above the block, the anchor shifts automatically to the new node index, preventing manual corrections from sliding out of alignment.

---

## 5. Core Block Detection & Scoping Logic

Block detection operates across four modular stages:

### Stage 1: Input Normalization & OCR Repair (`normalize.rs`, `repair.rs`)
Before parsing, OCR transcription defects are rectified idempotently:
- Strip running headers and footers (page numbers like `12`, `۱۲`, or repeated header strings) that would break blocks across page boundaries.
- Unwrap full-document code fences (frequently produced by LLM OCR prompts).
- Convert `\( ... \)` and `\[ ... \]` to standard `$ ... $` and `$$ ... $$`.
- Wrap bare LaTeX display environments (`equation`, `align`, `alignat`, `gather`) in `$$`.
- Fix broken line breaks (`\\` in prose) and spaces inside bold markers (`** text **`).
- Balance unclosed `$` delimiters.
- In [`repair::wrap_bare_math`](../crates/math-core/src/repair.rs), identify bare mathematical expressions lacking delimiters (e.g., `اگر P(B) \neq 0 آن‌گاه`) and wrap them in `$ ... $`.

### Stage 2: Feature Extraction (`features.rs`)
For every AST node, [`Lexicon::extract`](../crates/math-core/src/features.rs) extracts a static [`NodeFeatures`](../crates/math-core/src/features.rs) vector:
- `lead: Option<KeywordHit>`: Checks if the node begins with an opener keyword (Persian or English) matching any [`BlockType::SEMANTIC`](../crates/math-core/src/model.rs). Extracts:
  - `number`: Parsed identifier (e.g., `3.2`, `۳-۱`).
  - `name`: Parenthetical title (e.g., `(هاینه–کانتور)`, `(Cauchy-Schwarz)`).
  - `labelled`: Whether the keyword is followed by a number, colon, period, or name.
  - `reference`: True if the keyword is followed by an active verb (e.g., "Theorem 2 implies..."), meaning the sentence *mentions* rather than *states* a theorem.
  - `refers_to`: Identified target statement for proofs (e.g., "Proof of Theorem 2").
- `math_density: f32`: Ratio of mathematical characters to total character length, saturated at 30% math ($0.30 \to 1.0$).
- `logic_density: f32`: Count of formal mathematical/logical symbols ($\forall, \exists, \implies, \iff, \in, \subseteq$, etc.), saturated at 3 symbols ($3 \to 1.0$).
- `qed: bool`: Scans the last 80 characters of the node for QED symbols ($\blacksquare, \square, \text{Q.E.D.}$, `■`, `□`, or Persian `چ.ب.ح`, `پایان برهان`).
- `definitional: bool`: Matches definition phrases ("if and only if", "تعریف می‌شود", "نامیده می‌شود").
- `continuation: bool`: Matches continuation connectives ("where", "که در آن", "بطوریکه").

### Stage 3: Opener Scoring & Sequence Tagging
The document is tagged using one of two sequence taggers implementing [`SequenceTagger`](../crates/math-core/src/tagger.rs):
- **HeuristicTagger**: Evaluates linear weights or the decision tree. If $\text{score} \ge \text{threshold}$ (default `0.50`), the node is tagged as `Begin(BlockType)`.
- **CrfTagger**: Runs the trained linear-chain CRF model via Viterbi decoding, assigning global optimal `Begin(BlockType)`, `Inside(BlockType)`, or `Outside` tags.

### Stage 4: Scoping & Block Construction (`classifier.rs`)
In [`tags_to_blocks`](../crates/math-core/src/classifier.rs):
1. **LaTeX Environment Bypass**: Any `EnvSpan` (`\begin{theorem}`) automatically commands score `1.0` and exact boundary coverage, overriding heuristic or CRF decisions unless explicitly suppressed by a user override.
2. **Greedy Scope Absorption**:
   - For statements (Theorems, Lemmas, Propositions, Corollaries, Definitions), the scope absorbs subsequent `DisplayMath`, `List`, `Table`, and continuation paragraphs until encountering a new opener, a heading, a non-continuation paragraph, or hitting [`scope.statement_max_nodes`](../crates/math-core/src/config.rs) (default 6).
   - For arguments (Proofs, Solutions), the scope extends until encountering a `qed` marker, a structural heading, or hitting [`scope.proof_max_nodes`](../crates/math-core/src/config.rs) (default 12).
3. **Argument Linking**: Proofs and Solutions are linked to their corresponding statements (`SemanticBlock.proves`) by explicit reference first (`refers_to`), falling back to the nearest preceding unlinked statement.
4. **Cross-Referencing (`crossref.rs`)**: Textual references in prose (e.g., "by Theorem 3.2", "طبق لم ۱") are converted to internal hyperlinks pointing to the target block's HTML id (`#block-theorem-1`).

---

## 6. Heuristic Decision Tree (`crates/math-core/src/tree.rs`)

As an alternative to linear weighted summation, users can configure a static decision tree via `[scorer] model = "tree"` in `scoring.toml`.

### Data Model
```rust
pub enum TreeNode {
    Split {
        test: Test,
        yes: Box<TreeNode>,
        no: Box<TreeNode>,
    },
    Leaf {
        score: f32,
    },
}
```

### Tree Evaluation Mechanics
The tree is evaluated once per `(NodeFeatures, candidate BlockType)`. Each [`Test`](../crates/math-core/src/tree.rs) asks a contextual question relative to the candidate type:
- `Lead`: Does the node open with a keyword matching this candidate type without being an active reference?
- `Labelled`: Is that keyword formatted as a label?
- `Emphasized`: Does the node start with bold or italic emphasis?
- `Mention`: Does a keyword of this type appear anywhere in the node?
- `Qed`: Is a QED marker present?
- `ProofCue` / `StatementCue` / `Definitional` / `Continuation`: Are structural phrases present?
- `Kind { kinds }`: Is the candidate type in this set?
- `Statement`: Is the candidate type a theorem-like statement?
- `MathAtLeast { value }` / `LogicAtLeast { value }`: Does density meet or exceed the threshold?

The tree is strictly validated on load: maximum recursion depth is capped at 32 (`MAX_TREE_DEPTH`), leaf scores must reside in $[0.0, 1.0]$, and thresholds must be normalized.

---

## 7. Machine Learning: Linear-Chain CRF (`crf.rs` & `ml/`)

The CRF treats block detection as joint sequence labeling, directly predicting transitions between block openers, body paragraphs, and outside text.

### The Sequence Labeling Formulation
Given a sequence of AST nodes $X = (x_1, x_2, \dots, x_N)$, the model predicts an optimal tag sequence $Y = (y_1, y_2, \dots, y_N)$ where each $y_t \in \mathcal{Y}$ is a BIO tag:
- `O`: Outside any block (regular prose, headings, unassigned tables).
- `B-<TYPE>`: Opener of a block (12 semantic types: `B-THEOREM`, `B-LEMMA`, `B-PROOF`, etc.).
- `I-<TYPE>`: Inside/continuation of an open block.

Total label space $|\mathcal{Y}| = 1 + 12 \times 2 = 25$ states.

### Feature Space & Sliding Window
Features for node $t$ incorporate context from window $[t-1, t, t+1]$:
- Prefixes indicate position relative to $t$: `"-1:"`, `"0:"`, `"+1:"`.
- `node_kind`: `paragraph`, `display_math`, `list_item`, `heading`, `quote`, etc.
- `ends_colon`: True if node text ends with `:` or `：`.
- `lead`, `labelled`, `body_empty`, `emphasized`, `qed`.
- Discretized densities:
  - `math_density`: `zero` ($\le 0.001$), `low` ($\le 0.30$), `medium` ($\le 0.70$), `high` ($> 0.70$).
  - `logic_density`: `zero` ($\le 0.001$), `low` ($\le 0.35$), `medium` ($\le 0.70$), `high` ($> 0.70$).
- Relational transition features:
  - `math_density_delta`: `up` ($> +0.05$), `down` ($< -0.05$), `flat`, or `none`.
  - Boundary signals: `is_first_node`, `is_last_node`.

### CRF Viterbi Trellis Diagram

```mermaid
graph LR
    subgraph Node_t_minus_1 ["Node t-1 (Features: 0:lead=theorem, 0:labelled=true)"]
        O_prev["O"]
        B_THM_prev["B-THEOREM"]
        I_THM_prev["I-THEOREM"]
        B_PRF_prev["B-PROOF"]
    end

    subgraph Node_t ["Node t (Features: -1:ends_colon=true, 0:math_density=high)"]
        O_curr["O"]
        B_THM_curr["B-THEOREM"]
        I_THM_curr["I-THEOREM"]
        B_PRF_curr["B-PROOF"]
    end

    subgraph Node_t_plus_1 ["Node t+1 (Features: 0:qed=true)"]
        O_next["O"]
        I_PRF_next["I-PROOF"]
    end

    %% Valid high-probability transitions
    B_THM_prev -- "T(B-THM -> I-THM) + StateFeat(I-THM)" --> I_THM_curr
    I_THM_curr -- "T(I-THM -> B-PRF) + StateFeat(B-PRF)" --> B_PRF_next["B-PROOF"]
    B_PRF_curr -- "T(B-PRF -> I-PRF) + StateFeat(I-PRF)" --> I_PRF_next
    
    %% Penalized transitions (CRF learns near -infinity)
    O_prev -. "Heavy Penalty (Invalid BIO)" .-> I_THM_curr
```

### Pure-Rust Viterbi Decoder (`crates/math-core/src/crf.rs`)
1. **Emission Matrix**: For node $t \in [0, N-1]$ and label $j \in [0, K-1]$:
   $$\text{emissions}[t][j] = \sum_{f \in \text{features}(t)} \text{weight}(f, j)$$
2. **Forward Dynamic Programming**:
   $$V[0][j] = \text{emissions}[0][j]$$
   $$V[t][j] = \max_{i \in [0, K-1]} \left( V[t-1][i] + \text{transitions}[i][j] \right) + \text{emissions}[t][j]$$
   $$\text{backpointer}[t][j] = \arg\max_{i \in [0, K-1]} \left( V[t-1][i] + \text{transitions}[i][j] \right)$$
3. **Backtracking**: Recovers the globally optimal sequence path $y^*$.
4. **Max-Marginal Confidence Margin**:
   The decoder computes backward pass $B[t][j]$. For each node $t$, the margin between the best path score and second-best alternative path score determines `TaggedNode.confidence`. Low confidence flags ambiguous boundaries for human review or judge escalation.

---

## 8. Block Judge & LLM Wiring (`crates/math-judge`)

When heuristics or the CRF encounter ambiguity, `math-core` escalates blocks to `math-judge`.

### Escalation Conditions
Confidence gating triggers escalation under two specific conditions:
1. The CRF Viterbi confidence margin falls below [`config.ml_confidence_threshold`](../crates/math-core/src/config.rs) (default `0.30`).
2. An `Outside` paragraph exhibiting high logic density ($\ge 0.50$) immediately follows a `Theorem` or `Lemma` statement (suspected unlabelled proof).

### The JSON Protocol Contract
`math-judge` communicates with external processes via line-delimited JSON:

#### `JudgeRequest` (sent to model):
```json
{
  "protocol": 1,
  "id": "block-theorem-1",
  "kind": "theorem",
  "start": 2,
  "end": 5,
  "nodes_total": 42,
  "text": "**Theorem 1.** Let $V$ be a finite-dimensional vector space...",
  "before": [[0, "Section Introduction..."], [1, "Recall earlier definitions..."]],
  "after": [[5, "Proof. We proceed by induction on $n$..."]],
  "proves": null,
  "score": 0.85
}
```

#### `JudgeResponse` (returned by model):
```json
{
  "id": "block-theorem-1",
  "verdict": "accept",
  "confidence": 0.95
}
```

Possible verdicts:
- `"accept"`: The proposed block scope is complete and correct.
- `"incomplete"`: Widen the scope by one node and re-evaluate.
- `"overreach"`: Narrow the scope by one node.
- `"reject"`: Not a block of this type; drop the opener.
- `{"retype": "<new_type>"}`: Keep the scope, change the block type.
- `{"end_at": <node_index>}`: Explicitly truncate or expand end boundary.
- `{"start_at": <node_index>}`: Explicitly move start boundary.

### Judge Backends
1. **`CommandJudge`**: Launches an external binary or script (e.g., local model wrappers like Jev, Clef, or Laya) via child process `stdin`/`stdout`. Writing occurs on a dedicated background thread to prevent deadlocks on full stdout pipes.
2. **`ApiJudge`**: Direct HTTP calls to hosted decision models:
   - TypeSafe Jev (`POST /v1/systemone`)
   - Anthropic (`POST https://api.anthropic.com/v1/messages`)
   - OpenAI / Compatible (`POST https://api.openai.com/v1/chat/completions`)
   Constructs standardized multiple-choice and probability questions (`is_block`, `kind`, `next_continues`, `last_belongs`) and resolves responses against a strict truth table in [`crates/math-judge/src/jev.rs`](../crates/math-judge/src/jev.rs).
3. **`CachedJudge`**: Hashes `(block_text, block_type, model_version)` into SHA-256 keys. Cached verdicts bypass re-querying unchanged documents.
4. **`Replay`**: Records applied verdicts into a JSON log (`--judge-log`). Passing the log to `--overrides` reproduces the exact reviewed document offline without calling the model.

---

## 9. Compilation Pipelines & Security Model

### HTML5 Document Compilation (`crates/math-core/src/html.rs`)
The HTML renderer compiles [`Document`](../crates/math-core/src/model.rs) into a single standalone HTML5 file using Askama (`templates/document.html`).
- **Asset Inlining**:
  - Fonts: Vazirmatn variable font (weights 100–900) is embedded as a WOFF2 `data:font/woff2;base64,...` URI.
  - KaTeX: Pre-processed CSS with fonts inlined as data URIs and `katex.min.js` are inlined directly into `<style>` and `<script>` tags.
  - Runtime: Vanilla JS navigation runtime ([`assets/runtime.js`](../crates/math-core/assets/runtime.js)) and hierarchical mindmap navigator ([`assets/mindmap.js`](../crates/math-core/assets/mindmap.js)) are concatenated and embedded.
- **Content Security Policy (CSP)**:
  Mathia generates a strict CSP meta header:
  ```text
  default-src 'none';
  script-src 'sha256-<RUNTIME_JS>' 'sha256-<KATEX_JS>' 'sha256-<AI_JS>';
  style-src 'unsafe-inline';
  font-src data:;
  img-src data: blob: https:;
  connect-src <ALLOWED_AI_ORIGINS>;
  ```
  - Scripts execute **only** if their exact SHA-256 hash matches the header. Injected `<script>` tags, inline `onload` attributes, or `javascript:` links from raw Markdown are completely neutralized.
  - Script strings are normalized to LF (`normalize_newlines`) before hashing to prevent CRLF line-ending mismatches on Windows.

### Interactive Client-Side AI (`assets/ai.js`)
When compiled with `--ai` or specific `--ai-provider` flags:
1. `crates/math-core/assets/ai.js` and `ai.css` are embedded into the document.
2. Every semantic block renders **Ask** and **Illustrate** action buttons.
3. The reader inputs their API key into the in-document settings dialog.
4. Key storage: Saved strictly in `window.localStorage["mathia-ai"]`. The key never leaves the client browser and is transmitted only to the designated provider endpoint.
5. Zero Server Requirement: Streaming responses use client-side Server-Sent Events (`fetch` with `ReadableStream`). LaTeX in responses is rendered client-side via the embedded `katex.renderToString()`. SVG figures generated by **Illustrate** are validated and injected directly into the DOM using `createElementNS`.

### Markdown Export & HTML Import
- **Markdown Export (`crates/math-core/src/markdown.rs`)**:
  Renders a classified `Document` back to standardized Markdown. If compiled from source Markdown, untouched nodes preserve exact original formatting. Block openers receive standardized bold badges (`**Theorem 1.**`), and LaTeX environments restore `\begin{theorem}...\end{theorem}` syntax.
- **HTML Import & Cleaning (`crates/math-core/src/clean_html.rs`)**:
  Accepts external or legacy HTML documents, strips foreign scripts, ads, and stale navigation chrome, extracts mathematical spans and headers, and normalizes the content back into Mathia's AST for clean recompilation.
