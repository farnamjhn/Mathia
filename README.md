# Mathia

Mathia compiles OCR-transcribed Markdown (Persian/English mathematics with LaTeX) into a
single, self-contained, interactive HTML study guide.

It detects mathematical structure — theorems, lemmas, propositions, corollaries,
definitions, proofs, examples and remarks — and renders them as styled callouts with a
sidebar table of contents and KaTeX-rendered math. Output is deterministic: the same input
and configuration always produce the same document.

## Layout

| Path | Purpose |
|---|---|
| `crates/math-core` | Parser, feature extraction, block classifier and HTML renderer |
| `crates/mathia-cli` | Command-line front end (`mathia`) |
| `src-tauri`, `ui` | Desktop app (Tauri v2, `mathia-app`) and its frontend |
| `PLAN.md` | Design decisions and roadmap |

## Usage

```bash
cargo run -p mathia -- input.md -o output.html
```

Options:

- `--config scoring.toml` — scoring configuration (TOML or JSON); `--dump-config` prints the defaults.
- `--explain` — print every node's scores and why each block ended where it did.
- `--font Vazirmatn.woff2` — embed a WOFF2 text font in the output.
- `--katex-dir DIR` — inline a local KaTeX distribution; `--no-math` leaves LaTeX unrendered.
- `--ai` — embed the interactive AI runtime (all providers: Anthropic, OpenAI, Gemini).
- `--ai-provider NAME` — enable a specific AI provider (repeatable: `anthropic`, `openai`, `gemini`).
- `--ai-custom [NAME=]URL` — enable a custom OpenAI-compatible AI provider (repeatable: e.g. `https://api.deepseek.com/v1`, `ollama=http://localhost:11434/v1`).

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
compiled document next to a block inspector, and export the HTML. It needs the vendored
KaTeX (`scripts/vendor-katex.sh`) and, on Linux, Tauri's
[system dependencies](https://tauri.app/start/prerequisites/). The app is early; see
[PLAN.md](PLAN.md), Phase 3.

## Development

```bash
cargo test                 # math-core and the CLI
cargo test -p mathia-app   # the desktop app (builds Tauri)
```

The JavaScript inside compiled documents is tested in headless Chrome (see
[tests/js/README.md](tests/js/README.md)):

```bash
cd tests/js && npm install && npm test
```

See [PLAN.md](PLAN.md) for the current state and next steps.

## License

Mathia is free software, licensed under the
[GNU General Public License v3.0 or later](LICENSE).
