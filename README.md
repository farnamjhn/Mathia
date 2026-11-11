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
| `src-tauri` | Command-line front end (Tauri shell planned) |
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

## Development

```bash
cargo test
```

See [PLAN.md](PLAN.md) for the current state and next steps.

## License

Mathia is free software, licensed under the
[GNU General Public License v3.0 or later](LICENSE).
