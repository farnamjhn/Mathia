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

Optionally pass `--katex-dir DIR` to inline a local KaTeX distribution.

## Development

```bash
cargo test
```

See [PLAN.md](PLAN.md) for the current state and next steps.

## License

Mathia is free software, licensed under the
[GNU General Public License v3.0 or later](LICENSE).
