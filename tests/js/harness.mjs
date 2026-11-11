// Shared set-up for the runtime tests: compiles the fixtures once with the real
// CLI, drives the system Chrome through playwright-core, and keeps every test
// off the network (anything that is not file: and not mocked is aborted).

import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { chromium } from "playwright-core";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const FIXTURES = join(ROOT, "crates/math-core/tests/fixtures");

export const ANTHROPIC = "https://api.anthropic.com/**";
export const OPENAI = "https://api.openai.com/**";
export const GEMINI = "https://generativelanguage.googleapis.com/**";

let outDir = null;
let browser = null;
/** file: URLs of the compiled fixtures. */
export const pages = {};

function compile(fixture, name, ...flags) {
  const out = join(outDir, name);
  execFileSync(
    "cargo",
    ["run", "-q", "-p", "mathia", "--", join(FIXTURES, fixture), "-o", out, ...flags],
    { cwd: ROOT, stdio: ["ignore", "ignore", "inherit"] },
  );
  return pathToFileURL(out).href;
}

export async function setup() {
  outDir = mkdtempSync(join(tmpdir(), "mathia-js-"));
  pages.en = compile("algebra_en.md", "en-ai.html", "--ai");
  pages.fa = compile("analysis_fa.md", "fa-ai.html", "--ai");
  pages.plain = compile("algebra_en.md", "en-plain.html");
  browser = await chromium.launch({ channel: "chrome" });
}

export async function teardown() {
  if (browser) await browser.close();
  if (outDir) rmSync(outDir, { recursive: true, force: true });
}

/** A fresh browser context (own storage) with one page on `url`. */
export async function open(url) {
  const context = await browser.newContext();
  const leaked = [];
  await context.route(/^(?!file:)/, (route) => {
    leaked.push(route.request().url());
    return route.abort();
  });
  await context.addInitScript(() => {
    window.__csp = [];
    document.addEventListener("securitypolicyviolation", (e) => {
      window.__csp.push(e.violatedDirective + " " + e.blockedURI);
    });
  });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (err) => errors.push(String(err)));
  await page.goto(url);
  return {
    context,
    page,
    /** Requests that were neither file: nor mocked. */
    leaked,
    errors,
    async close() {
      const csp = await page.evaluate(() => window.__csp);
      await context.close();
      return { csp, errors, leaked };
    },
  };
}

/** Server-sent-event body in each provider's streaming format. */
export const sse = {
  anthropic(chunks) {
    const events = [
      'event: message_start\ndata: {"type":"message_start","message":{}}\n\n',
      ": keep-alive comment\n\n",
    ];
    for (const text of chunks) {
      const data = { type: "content_block_delta", index: 0, delta: { type: "text_delta", text } };
      events.push(`event: content_block_delta\r\ndata: ${JSON.stringify(data)}\r\n\r\n`);
    }
    events.push('event: message_stop\ndata: {"type":"message_stop"}\n\n');
    return events.join("");
  },
  openai(chunks) {
    return (
      chunks
        .map((content) => `data: ${JSON.stringify({ choices: [{ delta: { content } }] })}\n\n`)
        .join("") + "data: [DONE]\n\n"
    );
  },
  gemini(chunks) {
    return chunks
      .map((text) => {
        const data = { candidates: [{ content: { role: "model", parts: [{ text }] } }] };
        return `data: ${JSON.stringify(data)}\n\n`;
      })
      .join("");
  },
};

/**
 * Answers every request matching `pattern` with `reply` (a body string, or
 * `{status, body}`), and records what the page sent.
 */
export async function mock(context, pattern, reply) {
  const requests = [];
  await context.route(pattern, async (route) => {
    const request = route.request();
    requests.push({
      url: request.url(),
      headers: request.headers(),
      body: request.postDataJSON(),
    });
    const answer = typeof reply === "function" ? reply(requests.length) : reply;
    const { status = 200, body } = typeof answer === "string" ? { body: answer } : answer;
    await route.fulfill({
      status,
      headers: {
        "access-control-allow-origin": "*",
        "content-type": status === 200 ? "text/event-stream" : "application/json",
      },
      body,
    });
  });
  return requests;
}

/** Stores a key through the settings dialog, the way a reader would. */
export async function saveKey(page, provider, key) {
  await page.click("#mx-ai-toggle");
  await page.selectOption("#mx-ai-provider-select", provider);
  await page.fill("#mx-ai-key-input", key);
  await page.click("#mx-ai-dialog .mx-ai-btn-primary");
}
