# Phase 2 — interactive runtime: implementation spec

This spec is the contract for PLAN.md Phase 2. Work is cut into slices (S1–S5); each
slice must leave `cargo test` green and must not touch files outside its list unless it
says so.

## Ground rules (all slices)

- `math-core` stays pure: **no network, no LLM client in Rust**. Rust only decides
  whether the AI runtime is embedded and what the CSP allows. All network code is
  browser JavaScript.
- Vanilla JS, ES2017, no dependencies, no build step, wrapped in an IIFE, `"use strict"`.
  Same style as `crates/math-core/assets/runtime.js`.
- **Never use `innerHTML`, `outerHTML`, `insertAdjacentHTML`, `document.write`, `eval`,
  `new Function`, or inline event-handler attributes.** Build DOM with `createElement`,
  `textContent`, `setAttribute`. (The CSP allows scripts by hash only, so inline handlers
  would not run anyway.)
- Default output must not change: when AI is not enabled, the compiled HTML is
  byte-identical to today's (corpus snapshots in `crates/math-core/tests/snapshots` must
  not need regenerating).
- UI strings in both Persian and English, chosen by `document.documentElement.dir`
  (`rtl` → Persian), like the existing `toc_label` / `theme_label`.
- Styling goes in a separate `assets/ai.css` (embedded only when AI is enabled), using the
  existing CSS variables (`--mx-bg`, `--mx-text`, `--mx-border`, `--mx-gray-*`,
  `--mx-font-sans`, `--mx-font-mono`). Monochrome, matches `.mx-actions button`. Hidden in
  `@media print`.

## S1 — Rust: opt-in AI embedding and CSP

Files: `crates/math-core/src/html.rs`, `crates/math-core/src/lib.rs`,
`crates/math-core/templates/document.html`, `crates/mathia-cli/src/main.rs`, new
`crates/math-core/assets/ai.js` (stub for now: an IIFE that does nothing),
new `crates/math-core/assets/ai.css` (empty for now), `README.md` (usage line).

1. Add to `html.rs`:
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
   pub enum AiProvider { Anthropic, OpenAi, Gemini }
   impl AiProvider {
       pub const ALL: [AiProvider; 3];
       pub fn slug(self) -> &'static str;        // "anthropic" | "openai" | "gemini"
       pub fn origin(self) -> &'static str;      // "https://api.anthropic.com" | "https://api.openai.com" | "https://generativelanguage.googleapis.com"
       pub fn from_slug(s: &str) -> Option<Self>;
   }
   #[derive(Debug, Clone, PartialEq, Eq, Hash)]
   pub struct CustomProvider {
       pub slug: String,
       pub origin: String,
       pub endpoint: Option<String>,
   }
   #[derive(Debug, Clone, Default)]
   pub struct AiOptions {
       pub providers: Vec<AiProvider>,
       pub custom_providers: Vec<CustomProvider>,
   }  // empty = AI disabled
   ```
   and a field `pub ai: AiOptions` on `HtmlOptions`. Re-export `AiOptions`, `AiProvider`,
   `CustomProvider` from `lib.rs`.
2. When `options.ai` is non-empty (has built-in or custom providers):
   - embed `assets/ai.css` (after `style.css`, inside the same `<style id="mx-style">`) and
     `assets/ai.js` in a second script `<script id="mx-ai">…</script>` placed **after**
     `#mx-runtime`; escape `</script` like the runtime;
   - add the ai.js hash to `script-src`;
   - add `connect-src <origin> <origin> …` (deduplicated, in `ALL` order for built-ins,
     followed by custom origins) to the CSP;
   - put `data-mx-ai="anthropic openai custom"` (enabled slugs, `ALL` order for built-ins
     followed by custom, space-separated) on `<body>`, and optional
     `data-mx-ai-endpoints="slug=URL ..."` with default endpoints.
   When empty: none of the above; output identical to today.
3. CLI: `--ai` enables all three built-in providers; `--ai-provider NAME` (repeatable) enables
   chosen ones (unknown name → error listing valid names); `--ai-custom [NAME=]URL` (repeatable)
   enables custom OpenAI-compatible providers (extracting origin for CSP). Update `USAGE`,
   the doc comment and README.
4. Tests in `html.rs`: disabled → no `connect-src`, no `mx-ai`, no `data-mx-ai`;
   enabled with `[Gemini, Anthropic]` → `connect-src https://api.anthropic.com
   https://generativelanguage.googleapis.com`, `data-mx-ai="anthropic gemini"`, script
   present after `mx-runtime`, and its SHA-256 hash in the CSP (compute it the same way
   `content_security_policy` does).

## Addendum — any OpenAI-compatible API

`AiProvider::Compatible` (slug `openai-compatible`, last in `ALL`) is the generic provider: no
fixed host, the reader enters the base URL in the settings dialog. Its CSP source is
`https: http:` (so `connect-src` is open to any http(s) host only when it is enabled). In
`ai.js` every slug that is not `anthropic`/`openai`/`gemini` is endpoint-based: it needs an
endpoint (http/https), the key is optional (sent as `Authorization: Bearer` when set), and the
endpoint may be a base URL or a pasted `…/chat/completions` URL (normalised to the base).
`--ai-custom [NAME=]URL` keeps the tighter behaviour: CSP pinned to that URL's origin.

## S2 — ai.js core: settings, providers, streaming, safe rendering

File: `crates/math-core/assets/ai.js`, `crates/math-core/assets/ai.css`.

Exposes `window.Mathia.ai` (extend the object created by runtime.js; ai.js runs after it):

```js
Mathia.ai = {
  providers: [...],                  // from body[data-mx-ai]
  settings(),                        // -> {provider, model, hasKey}
  configure({provider, model, key}), // persists, see storage
  forgetKey(provider?),              // one provider, or all when omitted
  available(),                       // enabled && navigator.onLine && key for current provider
  complete({system, messages, onDelta, signal}) -> Promise<string>, // streams, resolves full text
  renderRich(container, markdownText),  // safe Markdown + KaTeX into container
  _internal: { parseSSE, buildRequest, extractDelta, sanitizeSvg, markdownToNodes } // for tests
}
```

**Storage.** One `localStorage` entry `mathia-ai` = JSON
`{ "provider": "anthropic", "models": {"anthropic": "..."}, "keys": {"anthropic": "..."}, "endpoints": {"custom": "..."} }`.
Wrap every access in try/catch (private mode → keep in memory only for the session).
Keys are never written to the DOM except into the password input while the settings
dialog is open, never logged, never put in URLs.

**Settings dialog.** A gear button `#mx-ai-toggle` (fixed, next to the theme toggle,
`inset-inline-end: 64px`) opens a `<dialog id="mx-ai-dialog">` with: provider `<select>`
(only enabled providers: built-ins and custom), model text input (prefilled with default),
an optional API endpoint URL input (when a custom provider is selected, prefilled with
default from `data-mx-ai-endpoints` or `localStorage`), API key
`<input type="password" autocomplete="off">` (optional for local custom endpoints),
a short notice ("stored only in this browser; sent only to <origin>"), buttons Save,
Forget key, Forget all, Close.

Default models: anthropic `claude-sonnet-5-5`, openai `gpt-5-mini`, gemini
the live Google Models API. Model input remains free text for custom or legacy IDs.

**Providers** (`buildRequest(provider, model, key, {system, messages})` returns
`{url, init}` for `fetch`; messages are `[{role: "user"|"assistant", content: string}]`):

- anthropic: `POST https://api.anthropic.com/v1/messages`, headers `content-type:
  application/json`, `x-api-key`, `anthropic-version: 2023-06-01`,
  `anthropic-dangerous-direct-browser-access: true`; body `{model, max_tokens: 4096,
  system, messages, stream: true}`. Deltas: SSE events with JSON
  `{type:"content_block_delta", delta:{type:"text_delta", text}}`. Errors: `{type:"error",
  error:{message}}`.
- openai / custom OpenAI-compatible: `POST ${endpoint}/chat/completions` (where `endpoint`
  defaults to `https://api.openai.com/v1` for `openai`), headers `content-type: application/json`,
  `authorization: Bearer KEY` (if key provided);
  body `{model, stream: true, messages: [{role:"system",content:system}, ...messages]}`.
  Deltas: `choices[0].delta.content`; `data: [DONE]` ends.
- gemini: `POST https://generativelanguage.googleapis.com/v1beta/models/{model}:streamGenerateContent?alt=sse`,
  header `x-goog-api-key: KEY` (never the `key=` query param); body
  `{systemInstruction:{parts:[{text:system}]}, contents:[{role:"user"|"model", parts:[{text}]}]}`.
  Deltas: concatenation of `candidates[0].content.parts[*].text`.

`parseSSE(chunkText, state)` is incremental: accepts arbitrary chunk boundaries (even
mid-line, mid-`\r\n`), returns the list of complete `data:` payloads (multi-line `data:`
joined by `\n`), ignores comments/`event:`/`id:` lines. `complete` reads
`response.body.getReader()` with a `TextDecoder` (`stream: true`). Non-2xx → reject with
an `Error` whose message includes status and provider error message (parse JSON body if
possible). Must honour `signal` (AbortController).

**Safe rendering.** `markdownToNodes(text)` returns a `DocumentFragment` built with DOM
APIs only. Supported: paragraphs (blank-line separated), `#`–`###` headings, `-`/`*`/`1.`
lists (one level), fenced code blocks, inline code, `**bold**`, `*italic*`, `$…$`,
`$$…$$` (also `\(…\)`, `\[…\]`). Math becomes `<span class="math math-inline">` /
`<div class="math math-display">` with the TeX as textContent; then call
`Mathia.renderMath()` (already skips rendered nodes). Links are rendered as plain text.
Text direction: each paragraph gets `dir="auto"`.

**Offline / disabled.** If `body[data-mx-ai]` is missing or empty: do nothing at all.
Toggle `document.documentElement.classList` `mx-ai-offline` on `online`/`offline` events
and at start; CSS hides `#mx-ai-toggle` and every `[data-mx-ai-action]` under that class.

## S3 — "Ask" action per block

File: `ai.js`, `ai.css`.

Register via `Mathia.registerAction({id: "ask", label: "Ask" / "پرسش", run})` for all
semantic block types. Buttons get `data-mx-ai-action`. Clicking toggles a panel
`<div class="mx-ai-panel" data-mx-ai-panel="BLOCK_ID">` appended at the end of the block
`<section>` (after `.mx-block-body`), containing: the conversation log, a `<textarea>`,
a checkbox "include neighbouring blocks" (default on), Send, Stop (while streaming),
Clear.

- If no key: the panel shows a one-line message with a button that opens the settings
  dialog.
- System prompt (constant, language-neutral): explain you are a mathematics tutor helping
  a student read a study guide; answer in the language of the question; use `$…$` / `$$…$$`
  for math; be concise and rigorous.
- First user message is composed as: block type and title, then the block text
  (`block.text`), then — if the checkbox is on — the previous and next semantic blocks'
  text (and, for a proof, the block it `proves`), each truncated to 4000 chars, then the
  question. Later turns send only the question; the whole conversation is resent each
  time.
- The answer streams into an assistant bubble; re-render with `renderRich` on each delta
  (throttled to one render per animation frame). Errors appear in the bubble with class
  `mx-ai-error`.
- Conversation kept per block in memory and mirrored to `sessionStorage`
  (`mathia-ai-chat:<location.pathname>:<blockId>`), so it survives reload but not the
  browser session. Clear wipes both.

## S4 — "Illustrate" action (explanatory SVG)

File: `ai.js`, `ai.css`.

- Action `{id: "illustrate", label: "Illustrate" / "شکل"}`, all semantic types.
- Prompt asks for exactly one self-contained SVG (viewBox, no scripts, no external
  references, no foreignObject, text labels in the block's language, `currentColor`
  strokes) inside a ```svg fence, followed by at most three sentences of caption.
- Extract the first `<svg …>…</svg>`; run `sanitizeSvg(string) -> string | null`:
  parse with `DOMParser` as `image/svg+xml` (reject on `parsererror`); walk the tree
  keeping only an allowlist of elements (`svg g defs title desc path rect circle ellipse
  line polyline polygon text tspan marker linearGradient radialGradient stop clipPath
  mask pattern symbol use`) and attributes (geometry/presentation attributes; no `on*`,
  no `style` containing `url(` other than `url(#…)`, `href`/`xlink:href` only if it starts
  with `#`); drop everything else; return `XMLSerializer` output.
- Display as `<img class="mx-ai-figure" alt="caption" src="data:image/svg+xml;charset=utf-8,…">`
  (encodeURIComponent) — an SVG in `<img>` cannot run script or fetch anything, which is a
  second line of defence. Caption under it via `renderRich`. Dark mode: wrap image in a
  white rounded box so `currentColor`/black strokes stay visible.
- Cache in `localStorage` key `mathia-ai-svg:<location.pathname>:<blockId>:<hash of block
  text>` (simple FNV-1a hex is fine), value `{svg, caption}`; a cached figure shows
  immediately, with a "Regenerate" button. On quota errors, ignore.

## S5 — JS test harness (headless Chrome)

Files: new `tests/js/package.json`, `tests/js/*.test.mjs`, `tests/js/README.md`,
`.gitignore` (add `tests/js/node_modules`).

- `playwright-core` only (devDependency), launched with `channel: "chrome"` (the system
  Chrome; no browser download). Runner: `node --test`.
- A fixture is built by running `cargo run -q -p mathia -- <fixture.md> -o <tmp>.html --ai`
  once per test run (use `crates/math-core/tests/fixtures/*.md` — pick one RTL and one LTR),
  opened via `file://`.
- Network is mocked with `page.route("https://api.anthropic.com/**", …)` etc., returning
  canned SSE bodies; nothing ever reaches the internet (`route("**", abort)` for anything
  not matched and not `file:`).
- Tests: unit tests of `_internal` (parseSSE split at every byte offset of a sample
  stream; extractDelta for the three providers; sanitizeSvg strips `<script>`,
  `onload`, `href="https://…"`, `foreignObject`, `style="background:url(http…)"`; markdown
  renderer produces no `<script>`/`<img>` from hostile input); settings round-trip and
  forget key; Ask flow end-to-end with a mocked Anthropic stream (answer text appears,
  math rendered by KaTeX); Illustrate flow with a mocked response (an `img.mx-ai-figure`
  appears, cached on reload); offline (`context.setOffline(true)` hides the controls);
  no-AI build has no `#mx-ai-toggle`; CSP violations: listen for
  `securitypolicyviolation` and assert none.
