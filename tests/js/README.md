# Runtime tests (headless Chrome)

Tests for the JavaScript embedded in compiled documents (`crates/math-core/assets/runtime.js`
`ai.js`, and `mindmap.js`). They compile real fixtures with the `mathia` CLI and drive the system Chrome.

```sh
cd tests/js
npm install        # once; playwright-core only, no browser download
npm test           # = node --test
```

Requirements: Node 20+, Google Chrome installed, vendored KaTeX
(`scripts/vendor-katex.sh`), and a working `cargo`.

To run the mind map regressions with an already installed Playwright WebKit browser:

```sh
MATHIA_TEST_BROWSER=webkit node --test --test-name-pattern="deterministic mind map" runtime.test.mjs
```

These compare the cards with connector endpoints in screen coordinates after fit,
zoom, and branch changes, including Persian labels with KaTeX. They also exercise
card dragging, click navigation after dragging, layout reset, and exports with
cards moved beyond the original origin. WebKit coverage is
needed for the macOS desktop preview; Chromium alone cannot catch its SVG
`foreignObject` compositing issue.

How it works (`harness.mjs`):

- `setup()` runs `cargo run -p mathia -- <fixture>.md -o <tmp>.html [--ai]` once for an
  English fixture, a Persian (RTL) fixture, and an English one without `--ai`; pages are
  opened over `file://`.
- Nothing reaches the internet. Every request that is not `file:` is aborted and recorded,
  unless a test mocked it with `mock(context, ANTHROPIC | OPENAI | GEMINI, body)`; `sse.*`
  build canned streaming bodies in each provider's format.
- Every test ends with `finish()`, which fails on any CSP violation, uncaught page error
  or un-mocked request. The one exception: tests that feed the SVG sanitiser a figure with
  `on*` attributes pass `{ hostileSvg: true }`, because Chrome reports the blocked handler
  while parsing it (`script-src-attr`), which is the policy doing its job.

`../stress_m1.mjs` is an older, dependency-free script (raw DevTools protocol plus a local
streaming server on port 9999) that exercises real chunked streaming and aborts of the Ask
panel. It expects `target/algebra_en_ai.html` and `target/analysis_fa_ai.html` built with
`--ai --ai-custom mock=http://127.0.0.1:9999/v1`.

Pointer interactions also have a browser-independent runtime test:

```sh
node --test mindmap-interactions.test.mjs
```

It runs the actual mind map script with a minimal element/event fixture, checking
zoom-adjusted movement, drag versus click, cancellation, keyboard movement, and
position retention through folding and view changes. It does not verify painting.

The highlight tests cover RTL prose with inline LTR math, forward and backward
selections, wrapped and multi-element selections, multiple ranges, rejection of
selections outside the lesson, and mouse, touch, and keyboard activation.
