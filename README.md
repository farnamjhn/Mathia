# Mathia

Mathia compiles OCR-transcribed Markdown (Persian/English mathematics with LaTeX) into a
single, self-contained, interactive HTML study guide.

It detects mathematical structure — theorems, lemmas, propositions, corollaries,
definitions, proofs, examples and remarks — and renders them as styled callouts with a
sidebar table of contents and KaTeX-rendered math. Output is deterministic: the same input
and configuration always produce the same document.

although the application is labeled with math all over everything, you can probably use this for any other field of study.
probably computer science fits this suite the best in fileds other than math but i'm pretty sure any field like physics, chemistry and... would fit this compiler well.

## Layout

| Path | Purpose |
|---|---|
| `crates/math-core` | Parser, feature extraction, block classifier and HTML renderer |
| `crates/math-judge` | Decision models around the classifier: JSON contract, external-process judge, verdict cache, replay logs |
| `crates/mathia-cli` | Command-line front end (`mathia`) |
| `src-tauri`, `ui` | Desktop app (Tauri v2, `mathia-app`) and its frontend |
| `docs/archive/PLAN.md` | Design decisions and roadmap |

## Usage

```bash
./scripts/vendor-katex.sh    # once: pinned, checksummed KaTeX
./scripts/vendor-fonts.sh    # once: pinned, checksummed Vazirmatn subset
cargo run -p mathia -- input.md -o output.html
cargo run -p mathia -- chapters/ -o site/ --watch
```

A folder input compiles every Markdown file in it (not recursive) into the output folder;
`--watch` recompiles whenever an input changes.

Options:

- `--config scoring.toml` — scoring configuration (TOML or JSON); `--dump-config` prints the
  defaults. `[scorer] model = "tree"` switches from the weighted sum to a static decision tree
  over the same features.
- `--explain` — print every node's scores and why each block ended where it did.
- `--font F.woff2` — embed this WOFF2 text font instead of the bundled Vazirmatn; `--no-font`
  embeds none.
- `--katex-dir DIR` — inline a local KaTeX distribution; `--no-math` leaves LaTeX unrendered.
- `--ai` — embed the interactive AI runtime (all providers: Anthropic, OpenAI, Gemini, and
  `openai-compatible`).
- `--ai-provider NAME` — enable a specific AI provider (repeatable: `anthropic`, `openai`,
  `gemini`, `openai-compatible`).
- `--ai-custom [NAME=]URL` — pin a custom OpenAI-compatible provider to one endpoint
  (repeatable: e.g. `https://api.deepseek.com/v1`, `ollama=http://localhost:11434/v1`).

**Decision models.** `--judge CMD` lets an external model review every block: `CMD` reads one
JSON request per line on stdin (block text with LaTeX, neighbours, proposed type and scope) and
answers one verdict per line (`accept`, `incomplete`, `overreach`, `reject`,
`{"retype": "lemma"}`, `{"end_at": 7}`, `{"start_at": 3}`, optionally with a `confidence`).
`--judge-cache FILE` keeps verdicts keyed by block text, type and `--judge-model`, so an
unchanged document is not sent again; `--judge-log FILE` records the resulting corrections,
and `--overrides FILE` replays them later without the model. The contract is documented in
`crates/math-judge`.

**Any OpenAI-compatible API.** The `openai-compatible` provider (included in `--ai`) lets the
reader type any base URL (`https://openrouter.ai/api/v1`, `https://api.groq.com/openai/v1`,
`http://localhost:1234/v1`, …) and model name into the settings dialog; the API key is
optional for local servers. The page then allows connections to any `http(s)` host, so use
`--ai-provider` / `--ai-custom` instead when you want the page pinned to specific origins.
The server must send CORS headers that allow the page's origin (`file://` pages send
`Origin: null`).

With an `--ai*` option the compiled page gets a settings button (⚙) and two actions on every
block: **Ask** (a conversation about the block) and **Illustrate** (an explanatory figure).
The reader pastes their own API key into the page; it is kept in that browser's
`localStorage`, never written into the file, and sent only to the chosen provider. Without
these options the output contains no AI code and allows no network connections.

## Desktop app

```bash
cargo run -p mathia-app
```

Open a Markdown file (or drop one on the window, or pass it as an argument) to see the
compiled document next to a block inspector, and export the HTML (or a whole folder). The
preview follows edits made in another editor. Click a block in the preview or the list to
inspect it, then correct it by hand — change its type, make it shorter or longer, or
mark it as not a block; corrections can be saved as JSON for `mathia --overrides`. The list
flags the blocks detected with low confidence as *unsure*, so they can be checked first.
**Options → Detection settings…** edits the thresholds, weights, keywords and scorer, and
loads or saves configuration files. The **Transcription prompt** button
(also shown on the start screen) holds a ready-made prompt to paste into your OCR /
vision LLM, so the Markdown it transcribes uses the conventions Mathia detects. It needs the vendored
KaTeX and Vazirmatn (`scripts/vendor-katex.sh`, `scripts/vendor-fonts.sh`) and, on Linux, Tauri's
[system dependencies](https://tauri.app/start/prerequisites/). The app is early; see
[PLAN.md](docs/archive/PLAN.md), Phase 3.

## Development

```bash
cargo test                 # math-core, math-judge and the CLI
cargo test -p mathia-app   # the desktop app (builds Tauri)
```

CI (`.github/workflows/ci.yml`) runs rustfmt, clippy with warnings denied, all Rust tests and
the headless-Chrome tests. Pushing a `v*` tag runs `release.yml`, which builds the desktop
bundles for macOS, Windows and Linux into a draft release (signed only when signing secrets
are configured).

The JavaScript inside compiled documents is tested in headless Chrome (see
[tests/js/README.md](tests/js/README.md)):

```bash
cd tests/js && npm install && npm test
```

See [PLAN.md](docs/archive/PLAN.md) for the current state and next steps.

## License

Mathia is free software, licensed under the
[GNU General Public License v3.0 or later](LICENSE).
