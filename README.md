<!-- Improved compatibility of back to top link: See: https://github.com/othneildrew/Best-README-Template/pull/73 -->
<a id="readme-top"></a>

<!-- PROJECT SHIELDS -->
[![Contributors][contributors-shield]][contributors-url]
[![Forks][forks-shield]][forks-url]
[![Stargazers][stars-shield]][stars-url]
[![Issues][issues-shield]][issues-url]
[![GPL-3.0 License][license-shield]][license-url]



<!-- PROJECT HEADER -->
<br />
<div align="center">
  <a href="https://github.com/farnamjhn/Mathia">
    <img src="src-tauri/icons/dock-light.png" alt="Mathia logo" width="120" height="120">
  </a>

  <h3 align="center">Mathia</h3>

  <p align="center">
    Compile OCR transcribed Markdown into a single interactive HTML study guide.
    <br />
    <br />
    <br />
    <a href="#usage">View Usage</a>
    &middot;
    <a href="https://github.com/farnamjhn/Mathia/issues/new?labels=bug">Report Bug</a>
    &middot;
    <a href="https://github.com/farnamjhn/Mathia/issues/new?labels=enhancement">Request Feature</a>
  </p>
</div>



<!-- TABLE OF CONTENTS -->
<details>
  <summary>Table of Contents</summary>
  <ol>
    <li>
      <a href="#about-the-project">About The Project</a>
      <ul>
        <li><a href="#built-with">Built With</a></li>
        <li><a href="#project-layout">Project Layout</a></li>
      </ul>
    </li>
    <li>
      <a href="#getting-started">Getting Started</a>
      <ul>
        <li><a href="#download-a-release">Download a Release</a></li>
        <li><a href="#build-from-source">Build from Source</a></li>
        <li><a href="#running-mathia">Running Mathia</a></li>
      </ul>
    </li>
    <li>
      <a href="#usage">Usage</a>
      <ul>
        <li><a href="#cli-options">CLI Options</a></li>
        <li><a href="#decision-models">Decision Models</a></li>
        <li><a href="#interactive-ai">Interactive AI</a></li>
        <li><a href="#desktop-app">Desktop App</a></li>
      </ul>
    </li>
    <li><a href="#development">Development</a></li>
    <li><a href="#license">License</a></li>
  </ol>
</details>



<!-- ABOUT THE PROJECT -->
## About The Project

Mathia compiles OCR-transcribed Markdown (Persian/English mathematics with LaTeX) into a
single, self-contained, interactive HTML study guide.

It detects mathematical structure — theorems, lemmas, propositions, corollaries,
definitions, proofs, examples and remarks — and renders them as styled callouts with a
sidebar, table of contents and Katex rendered maths. Output is deterministic: the same input
and configuration will always produce the same document.

Although the application is labeled with math all over, you can use it for any other field of
study. Computer science probably fits best outside of math, but physics, chemistry and
Similar fields should be fine with this compiler as well.

<p align="right">(<a href="#readme-top">back to top</a>)</p>



### Built With

* [![Rust][Rust-badge]][Rust-url]
* [![Tauri][Tauri-badge]][Tauri-url]
* [![KaTeX][KaTeX-badge]][KaTeX-url]

<p align="right">(<a href="#readme-top">back to top</a>)</p>



### Project Layout

| Path | Purpose |
|---|---|
| `crates/math-core` | Parser, feature extraction, block classifier and HTML renderer |
| `crates/math-judge` | Decision models around the classifier: JSON contract, external-process judge, verdict cache, replay logs |
| `crates/mathia-cli` | Command-line front end (`mathia`) |
| `src-tauri`, `ui` | Desktop app (Tauri v2, `mathia-app`) and its frontend |
| `docs/archive/PLAN.md` | Design decisions and roadmap |

<p align="right">(<a href="#readme-top">back to top</a>)</p>



<!-- GETTING STARTED -->
## Getting Started

You can either download the ready made desktop app or build Mathia from source.

### Download a Release

Prebuilt desktop bundles for macOS, Windows and Linux are attached to each
[GitHub release](https://github.com/farnamjhn/Mathia/releases). Download the one for your
platform and open it. Releases are built automatically when a `v*` tag is pushed, and the
bundles are unsigned unless signing secrets are configured, so your OS may ask you to confirm
before opening the app.

### Build from Source

#### Prerequisites

* Rust toolchain
  ```sh
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
* On Linux, Tauri's [system dependencies](https://tauri.app/start/prerequisites/) (desktop app only)

#### Installation

1. Clone the repo
   ```sh
   git clone https://github.com/farnamjhn/Mathia.git
   cd Mathia
   ```
2. Vendor the pinned, checksummed KaTeX (once)
   ```sh
   ./scripts/vendor-katex.sh
   ```
3. Vendor the pinned, checksummed Vazirmatn font subset (once)
   ```sh
   ./scripts/vendor-fonts.sh
   ```

### Running Mathia

**Desktop app**

```sh
cargo run -p mathia-app
```

**Command line**

```sh
# a single file
cargo run -p mathia -- input.md -o output.html

# a folder of chapters, recompiled whenever an input changes
cargo run -p mathia -- chapters/ -o site/ --watch
```

A folder input compiles every Markdown file in it (not recursive) into the output folder;
`--watch` recompiles whenever an input changes.

**Fixing broken Markdown**

Transcriptions often arrive with broken LaTeX: inline formulas with no `$ $` around them
(`اگر P(B) \neq 0 آن‌گاه`), `\( \)` delimiters, bare `align` environments, unbalanced `$`,
or the entire document within a code fence. Mathia repairs these whenever it compiles, so the
preview and the exported HTML are right either way. To repair the Markdown file itself:

```sh
# writes notes.fixed.md next to the input (or use -o; a folder works too)
cargo run -p mathia -- notes.md --fix
```

In the desktop app it's simply **Options → Fix Markdown...** Formulas are recognized
conservatively: a LaTeX command or a sub/superscript, a relation between operands (`x = 5`),
a function value (`P(E)`), and, in Persian text, a Latin letter standing alone or a numeric
interval. English words and parenthesised glosses are never touched.

<p align="right">(<a href="#readme-top">back to top</a>)</p>



<!-- USAGE EXAMPLES -->
## Usage

### CLI Options

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

### Decision Models

`--judge CMD` lets an external model review every block: `CMD` reads one JSON request per line
on stdin (block text with LaTeX, neighbours, proposed type and scope) and answers one verdict
per line (`accept`, `incomplete`, `overreach`, `reject`, `{"retype": "lemma"}`,
`{"end_at": 7}`, `{"start_at": 3}`, optionally with a `confidence`).

`--judge-cache FILE` keeps verdicts keyed by block text, type and `--judge-model`, so an
unchanged document is not sent again; `--judge-log FILE` records the resulting corrections,
and `--overrides FILE` replays them later without the model. The contract is documented in
`crates/math-judge`.

### Interactive AI

With an `--ai*` option the compiled page gets a settings button (⚙) and two actions on every
block: **Ask** (a conversation about the block) and **Illustrate** (an explanatory figure).
The reader pastes their own API key into the page; it is kept in that browser's
`localStorage`, never written into the file, and sent only to the chosen provider. Without
these options the output contains no AI code and allows no network connections.

**Any OpenAI-compatible API.** The `openai-compatible` provider (included in `--ai`) lets the
reader type any base URL (`https://openrouter.ai/api/v1`, `https://api.groq.com/openai/v1`,
`http://localhost:1234/v1`, …) and model name into the settings dialog; the API key is
optional for local servers. The page then allows connections to any `http(s)` host, so use
`--ai-provider` / `--ai-custom` instead when you want the page pinned to specific origins.
The server must send CORS headers that allow the page's origin (`file://` pages send
`Origin: null`).

### Text Highlighting

Select text in a compiled lesson and choose **Highlight selection** (or press
`Ctrl/Cmd+Shift+H`). The action appears next to the selection and works with mouse, touch,
and keyboard. The browser keeps handling RTL/LTR selection. Within a paragraph, the
highlight background uses the tallest line's height so prose and inline formulas share
an even band. Highlights live in memory for the open lesson and disappear when the lesson
is reloaded.

Undo and redo work from the keyboard (`Ctrl/Cmd+Z`, `Ctrl/Cmd+Shift+Z`, `Ctrl+Y`) and, on a
page opened on its own — as on a phone — from the two floating buttons in the bottom corner.

Prose, code, and KaTeX's visible HTML layer can be highlighted, including selections that
span RTL and LTR runs, inline formulas, block boundaries, and wrapped lines. KaTeX's hidden
MathML copy is not painted, and SVG figures (AI illustrations or imported SVG) have no
selectable text ranges, so they cannot be highlighted. Browsers without `CSS.highlights`
simply do not show the action.

In the desktop app the same drag also offers **Expand block**, which grows the last block of
the selection over the text it covers — the usual fix when a proof or a remark continued as
plain paragraphs. See [Desktop App](#desktop-app).

### Desktop App

Open a Markdown file (or drop one on the window, or pass it as an argument) to see the
compiled document next to a block inspector and exported the HTML (or a whole folder). The
Preview follows edits made in another editor. Click a block in the preview or the list to
inspect it, then correct it by hand — change its type, make it shorter or longer, or
Mark it as not a block; corrections can be saved as JSON for mathia --overrides The list
flags the blocks that are detected with low confidence as 'unsure' so they can be checked first.
The **Type** menu narrows the list to one kind of block — every theorem, every lemma, every
proof — and shows how many of each the document has.

Drag over text in the preview and choose **Expand block** to grow the last block of the
selection over the text it covers; the block list and the inspector update at once.
Corrections and **Expand block** work on imported HTML pages as well as Markdown.

**Undo and redo** are the two arrows in the toolbar and the usual shortcuts: `⌘Z` / `Ctrl+Z`
to undo, `⇧⌘Z` / `Ctrl+Shift+Z` or `Ctrl+Y` to redo. They cover both corrections and text
highlights, in the order they were made.

**Export → Export as Markdown…** writes the compiled document back out as Markdown: the same
text, with every block labelled the way the inspector shows it (`**Theorem 1.** …`), LaTeX
environments kept as environments, and the corrections applied. It is also how an HTML page
becomes Markdown — open the HTML file and export.

**Options → Detection settings…** edits the thresholds, weights, keywords and scorer, and
loads or saves configuration files. The **Transcription prompt** button (also shown on the
start screen) holds a ready-made prompt to paste into your OCR / vision LLM, so the Markdown
It transcribes using the conventions that Mathia detects. **Options → Fix Markdown…** saves a
repaired copy of the open file (see [Running Mathia](#running-mathia)) and switches to it.

The app requires the vendored KaTeX and Vazirmatn (see [Build from Source](#build-from-source)). It is
early; see [PLAN.md](docs/archive/PLAN.md), Phase 3.

<p align="right">(<a href="#readme-top">back to top</a>)</p>



<!-- DEVELOPMENT -->
## Development

```sh
cargo test                 # math-core, math-judge and the CLI
cargo test -p mathia-app   # the desktop app (builds Tauri)
```

CI (`.github/workflows/ci.yml`) runs rustfmt, clippy with warnings denied, all Rust tests and
the headless-Chrome tests. Pushing a `v*` tag runs `release.yml`, which builds the desktop
bundles for macOS, Windows and Linux into a draft release (signed only when signing secrets
are configured).

The JavaScript inside compiled documents is tested in headless Chrome (see
[tests/js/README.md](tests/js/README.md)):

```sh
cd tests/js && npm install && npm test
```

See [PLAN.md](docs/archive/PLAN.md) for the current state and next steps.

<p align="right">(<a href="#readme-top">back to top</a>)</p>



<!-- LICENSE -->
## License

Mathia is free software, distributed under the
[GNU General Public License v3.0 or later](LICENSE). See `LICENSE` for more information.

<p align="right">(<a href="#readme-top">back to top</a>)</p>



<!-- MARKDOWN LINKS & IMAGES -->
[contributors-shield]: https://img.shields.io/github/contributors/farnamjhn/Mathia?style=for-the-badge
[contributors-url]: https://github.com/farnamjhn/Mathia/graphs/contributors
[forks-shield]: https://img.shields.io/github/forks/farnamjhn/Mathia?style=for-the-badge
[forks-url]: https://github.com/farnamjhn/Mathia/network/members
[stars-shield]: https://img.shields.io/github/stars/farnamjhn/Mathia?style=for-the-badge
[stars-url]: https://github.com/farnamjhn/Mathia/stargazers
[issues-shield]: https://img.shields.io/github/issues/farnamjhn/Mathia?style=for-the-badge
[issues-url]: https://github.com/farnamjhn/Mathia/issues
[license-shield]: https://img.shields.io/github/license/farnamjhn/Mathia?style=for-the-badge
[license-url]: https://github.com/farnamjhn/Mathia/blob/main/LICENSE
[Rust-badge]: https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white
[Rust-url]: https://www.rust-lang.org/
[Tauri-badge]: https://img.shields.io/badge/Tauri-24C8DB?style=for-the-badge&logo=tauri&logoColor=white
[Tauri-url]: https://tauri.app/
[KaTeX-badge]: https://img.shields.io/badge/KaTeX-329894?style=for-the-badge&logo=latex&logoColor=white
[KaTeX-url]: https://katex.org/
