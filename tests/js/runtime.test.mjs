import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { after, before, describe, test } from "node:test";
import { ANTHROPIC, GEMINI, OPENAI, mock, open, pages, saveKey, setup, sse, teardown } from "./harness.mjs";

before(setup, { timeout: 600_000 });
after(teardown);

/** Every test ends here: no CSP violation, no script error, nothing on the wire. */
async function finish(session, { hostileSvg = false } = {}) {
  const { csp, errors, leaked } = await session.close();
  // Parsing an SVG that carries on* attributes makes Chrome report the handler
  // as blocked, even in the inert DOMParser document: the policy working, not
  // a leak. Only tests that feed such an SVG may see it, and nothing else.
  const expected = hostileSvg ? csp.filter((v) => v === "script-src-attr inline") : [];
  assert.deepEqual(csp, expected, "CSP violations");
  assert.deepEqual(errors, [], "uncaught page errors");
  assert.deepEqual(leaked, [], "requests that were not mocked");
}

const firstBlock = "section.mx-block:has([data-mx-action=ask])";

describe("internals", () => {
  test("parseSSE gives the same payloads wherever the stream is cut", async () => {
    const s = await open(pages.en);
    const stream =
      sse.anthropic(["Hello ", "wörld — سلام", "$x^2$"]) +
      "data: first line\ndata: second line\n\n" +
      "id: 7\nretry: 10\ndata:nospace\r\n\r\n" +
      "data: [DONE]\r\r\n";
    const result = await s.page.evaluate((text) => {
      const { parseSSE } = window.Mathia.ai._internal;
      const whole = parseSSE(text, {});
      const bad = [];
      for (let cut = 0; cut <= text.length; cut++) {
        const state = {};
        const got = parseSSE(text.slice(0, cut), state).concat(parseSSE(text.slice(cut), state));
        if (JSON.stringify(got) !== JSON.stringify(whole)) bad.push(cut);
      }
      const state = {};
      let bytewise = [];
      for (const ch of text) bytewise = bytewise.concat(parseSSE(ch, state));
      return { whole, bad, bytewise };
    }, stream);
    assert.deepEqual(result.bad, []);
    assert.deepEqual(result.bytewise, result.whole);
    assert.equal(result.whole.length, 8);
    assert.ok(result.whole.includes("first line\nsecond line"));
    assert.ok(result.whole.includes("nospace"));
    assert.equal(result.whole.at(-1), "[DONE]");
    await finish(s);
  });

  test("extractDelta reads each provider's format and surfaces stream errors", async () => {
    const s = await open(pages.en);
    const r = await s.page.evaluate(() => {
      const { extractDelta } = window.Mathia.ai._internal;
      const thrown = (fn) => {
        try {
          fn();
          return null;
        } catch (err) {
          return err.message;
        }
      };
      return {
        anthropic: extractDelta("anthropic", '{"type":"content_block_delta","delta":{"type":"text_delta","text":"a"}}'),
        anthropicOther: extractDelta("anthropic", '{"type":"message_start"}'),
        openai: extractDelta("openai", '{"choices":[{"delta":{"content":"b"}}]}'),
        openaiRole: extractDelta("openai", '{"choices":[{"delta":{"role":"assistant"}}]}'),
        custom: extractDelta("ollama", '{"choices":[{"delta":{"content":"c"}}]}'),
        gemini: extractDelta("gemini", '{"candidates":[{"content":{"parts":[{"text":"d"},{"text":"e"}]}}]}'),
        garbage: extractDelta("openai", "{ not json"),
        done: extractDelta("openai", "[DONE]"),
        error: thrown(() => extractDelta("anthropic", '{"type":"error","error":{"message":"overloaded"}}')),
      };
    });
    assert.deepEqual(r, {
      anthropic: "a",
      anthropicOther: "",
      openai: "b",
      openaiRole: "",
      custom: "c",
      gemini: "de",
      garbage: "",
      done: "",
      error: "overloaded",
    });
    await finish(s);
  });

  test("buildRequest keeps keys in headers, never in URLs", async () => {
    const s = await open(pages.en);
    const r = await s.page.evaluate(() => {
      const { buildRequest } = window.Mathia.ai._internal;
      const input = { system: "sys", messages: [{ role: "user", content: "q" }, { role: "assistant", content: "a" }] };
      const out = {};
      for (const p of ["anthropic", "openai", "gemini"]) {
        const req = buildRequest(p, "model-x", "SECRET", input);
        out[p] = { url: req.url, headers: req.init.headers, body: JSON.parse(req.init.body) };
      }
      const custom = buildRequest("ollama", "m", "", { ...input, endpoint: "http://localhost:11434/v1/" });
      out.custom = { url: custom.url, headers: custom.init.headers };
      return out;
    });
    for (const p of ["anthropic", "openai", "gemini"]) assert.ok(!r[p].url.includes("SECRET"), p);
    assert.equal(r.anthropic.url, "https://api.anthropic.com/v1/messages");
    assert.equal(r.anthropic.headers["x-api-key"], "SECRET");
    assert.equal(r.anthropic.headers["anthropic-dangerous-direct-browser-access"], "true");
    assert.equal(r.anthropic.body.system, "sys");
    assert.equal(r.anthropic.body.stream, true);
    assert.equal(r.openai.url, "https://api.openai.com/v1/chat/completions");
    assert.equal(r.openai.headers.authorization, "Bearer SECRET");
    assert.deepEqual(r.openai.body.messages[0], { role: "system", content: "sys" });
    assert.equal(
      r.gemini.url,
      "https://generativelanguage.googleapis.com/v1beta/models/model-x:streamGenerateContent?alt=sse",
    );
    assert.equal(r.gemini.headers["x-goog-api-key"], "SECRET");
    assert.deepEqual(r.gemini.body.contents.map((c) => c.role), ["user", "model"]);
    assert.equal(r.custom.url, "http://localhost:11434/v1/chat/completions");
    assert.equal(r.custom.headers.authorization, undefined);
    await finish(s);
  });

  test("sanitizeSvg keeps drawings and drops everything active or external", async () => {
    const s = await open(pages.en);
    const r = await s.page.evaluate(() => {
      const { sanitizeSvg } = window.Mathia.ai._internal;
      const hostile = `<svg viewBox="0 0 10 10" onload="alert(1)" xmlns="http://www.w3.org/2000/svg">
        <!-- comment -->
        <script>alert(2)</script>
        <style>@import url(https://evil.example/x.css);</style>
        <foreignObject><body xmlns="http://www.w3.org/1999/xhtml"><img src="https://evil.example/a.png"/></body></foreignObject>
        <image href="https://evil.example/b.png"/>
        <a href="https://evil.example/"><circle cx="1" cy="1" r="1"/></a>
        <use href="https://evil.example/sprite.svg#x"/>
        <use href="#ok"/>
        <rect id="ok" width="5" height="5" style="background:url(http://evil.example/c.png)"/>
        <rect width="5" height="5" style="fill:u\\72l(http://evil.example/d.png)"/>
        <rect width="5" height="5" fill="url(https://evil.example/e.svg#g)"/>
        <rect width="5" height="5" fill="url(#grad)" onclick="alert(3)"/>
        <animate attributeName="x" to="5"/>
        <text x="1" y="9" style="fill: red">label ≤ 1</text>
      </svg>`;
      const clean = sanitizeSvg(hostile);
      return {
        clean,
        bare: sanitizeSvg('<svg viewBox="0 0 1 1"><path d="M0 0L1 1"/></svg>'),
        xlink: sanitizeSvg('<svg viewBox="0 0 1 1"><defs><path id="p" d="M0 0"/></defs><use xlink:href="#p"/></svg>'),
        notSvg: sanitizeSvg("<html><body>hi</body></html>"),
        broken: sanitizeSvg("<svg><path></svg>"),
        empty: sanitizeSvg(""),
      };
    });
    for (const bad of [
      "script", "alert", "onload", "onclick", "evil.example", "foreignObject", "<image", "<a ",
      "<style", "animate", "comment", "@import", "background",
    ]) {
      assert.ok(!r.clean.includes(bad), `"${bad}" survived: ${r.clean}`);
    }
    assert.ok(r.clean.includes('viewBox="0 0 10 10"'));
    assert.ok(r.clean.includes('href="#ok"'));
    assert.ok(r.clean.includes('fill="url(#grad)"'));
    assert.ok(r.clean.includes('style="fill: red"'));
    assert.ok(r.clean.includes("label ≤ 1"));
    assert.ok(r.bare.includes('xmlns="http://www.w3.org/2000/svg"'), "namespace is added when missing");
    assert.ok(r.xlink.includes('href="#p"'));
    assert.equal(r.notSvg, null);
    assert.equal(r.broken, null);
    assert.equal(r.empty, null);
    await finish(s, { hostileSvg: true });
  });

  test("the Markdown renderer never produces active elements", async () => {
    const s = await open(pages.en);
    const r = await s.page.evaluate(() => {
      const host = document.createElement("div");
      document.body.appendChild(host);
      window.Mathia.ai.renderRich(
        host,
        [
          "# Title <script>window.__pwned = 1</script>",
          "",
          'A <img src=x onerror="window.__pwned = 2"> paragraph with **bold**, *italic*, `code`,',
          "a [link](javascript:window.__pwned=3) and $a^2 + b^2$.",
          "",
          "- one",
          "- two <iframe src=https://evil.example></iframe>",
          "",
          "1. first",
          "",
          "$$\\int_0^1 x\\,dx$$",
          "",
          "```html",
          "<script>window.__pwned = 4</script>",
          "```",
        ].join("\n"),
      );
      const count = (sel) => host.querySelectorAll(sel).length;
      return {
        active: count("script, img, iframe, a, object, embed, style, link, form"),
        handlers: Array.from(host.querySelectorAll("*")).filter((el) =>
          Array.from(el.attributes).some((a) => a.name.startsWith("on")),
        ).length,
        pwned: window.__pwned,
        h1: count("h1"),
        li: count("li"),
        strong: count("strong"),
        em: count("em"),
        pre: host.querySelector("pre").textContent,
        inlineMath: count(".math-inline .katex"),
        displayMath: count(".math-display .katex"),
        text: host.textContent,
        dir: host.querySelector("p").getAttribute("dir"),
      };
    });
    assert.equal(r.active, 0);
    assert.equal(r.handlers, 0);
    assert.equal(r.pwned, undefined);
    assert.deepEqual([r.h1, r.li, r.strong, r.em], [1, 3, 1, 1]);
    assert.equal(r.pre, "<script>window.__pwned = 4</script>");
    assert.equal(r.inlineMath, 1);
    assert.equal(r.displayMath, 1);
    assert.ok(r.text.includes("<img src=x"), "hostile markup is shown as text");
    assert.ok(r.text.includes("link") && !r.text.includes("javascript:"));
    assert.equal(r.dir, "auto");
    await finish(s);
  });
});

describe("settings", () => {
  test("key round-trip through the dialog, and forgetting it", async () => {
    const s = await open(pages.en);
    const { page, context } = s;
    await mock(context, GEMINI, JSON.stringify({ models: [] }));
    assert.deepEqual(await page.evaluate(() => window.Mathia.ai.providers), ["anthropic", "openai", "gemini", "openai-compatible"]);
    assert.deepEqual(await page.evaluate(() => window.Mathia.ai.settings()), {
      provider: "anthropic",
      model: "claude-sonnet-5-5",
      hasKey: false,
    });
    assert.equal(await page.evaluate(() => window.Mathia.ai.available()), false);

    await page.click("#mx-ai-toggle");
    await page.selectOption("#mx-ai-provider-select", "gemini");
    assert.equal(await page.inputValue("#mx-ai-model-input"), "");
    assert.equal(await page.locator("#mx-ai-provider-select option:checked").textContent(), "Google");
    assert.ok(!(await page.isVisible("#mx-ai-endpoint-field")), "endpoint is for custom providers only");
    assert.match(await page.textContent("#mx-ai-notice"), /generativelanguage\.googleapis\.com/);
    assert.equal(await page.getAttribute("#mx-ai-key-input", "type"), "password");
    await page.fill("#mx-ai-model-input", "gemini-custom");
    await page.fill("#mx-ai-key-input", "test-key-123");
    await page.click("#mx-ai-dialog .mx-ai-btn-primary");

    assert.deepEqual(await page.evaluate(() => window.Mathia.ai.settings()), {
      provider: "gemini",
      model: "gemini-custom",
      hasKey: true,
    });
    assert.equal(await page.evaluate(() => window.Mathia.ai.available()), true);
    assert.equal(await page.evaluate(() => document.getElementById("mx-ai-dialog").open), false);
    assert.equal(await page.inputValue("#mx-ai-key-input"), "", "key leaves the DOM with the dialog");
    assert.ok(!(await page.content()).includes("test-key-123"), "key is never written into the document");

    await page.reload();
    assert.equal(await page.evaluate(() => window.Mathia.ai.settings().hasKey), true, "survives reload");
    assert.equal(
      await page.evaluate(() => JSON.parse(localStorage.getItem("mathia-ai")).keys.gemini),
      "test-key-123",
    );

    await page.click("#mx-ai-toggle");
    await page.click("#mx-ai-dialog button:text-is('Forget key')");
    await page.click("#mx-ai-dialog button:text-is('Close')");
    assert.equal(await page.evaluate(() => window.Mathia.ai.settings().hasKey), false);
    assert.ok(!(await page.evaluate(() => localStorage.getItem("mathia-ai"))).includes("test-key-123"));

    await page.evaluate(() => {
      window.Mathia.ai.configure({ provider: "anthropic", key: "k1" });
      window.Mathia.ai.configure({ provider: "openai", key: "k2" });
      window.Mathia.ai.forgetKey();
    });
    assert.deepEqual(await page.evaluate(() => JSON.parse(localStorage.getItem("mathia-ai")).keys), {});
    await finish(s);
  });

  test("Google models are loaded from the API", async () => {
    const s = await open(pages.en);
    const { page, context } = s;
    const requests = await mock(context, GEMINI, JSON.stringify({
      models: [
        { name: "models/gemini-available", displayName: "Gemini Available", supportedGenerationMethods: ["generateContent"] },
        { name: "models/embedding-only", supportedGenerationMethods: ["embedContent"] },
      ],
    }));

    await page.click("#mx-ai-toggle");
    await page.selectOption("#mx-ai-provider-select", "gemini");
    await page.fill("#mx-ai-key-input", "google-key");
    await page.click("#mx-ai-refresh-models");
    await page.waitForFunction(() => document.querySelectorAll("#mx-ai-google-models option").length === 1);

    assert.equal(await page.inputValue("#mx-ai-model-input"), "gemini-available");
    assert.deepEqual(await page.locator("#mx-ai-google-models option").evaluateAll((options) => options.map((o) => o.value)), ["gemini-available"]);
    assert.equal(requests.length, 1);
    assert.match(requests[0].url, /\/v1beta\/models\?pageSize=1000/);
    assert.equal(requests[0].headers["x-goog-api-key"], "google-key");
    await finish(s);
  });
});

describe("ask", () => {
  test("streams an answer with math, keeps the conversation, clears it", async () => {
    const s = await open(pages.en);
    const { page, context } = s;
    const requests = await mock(context, ANTHROPIC, (n) =>
      n === 1
        ? sse.anthropic(["A group is a set ", "$G$ with an operation:\n\n", "$$a \\cdot (b \\cdot c)", " = (a \\cdot b) \\cdot c$$"])
        : sse.anthropic(["Second answer."]),
    );

    const block = page.locator(firstBlock).first();
    await block.locator("[data-mx-action=ask]").click();
    const panel = block.locator(".mx-ai-panel");
    await panel.locator(".mx-ai-missing-key").waitFor();
    assert.ok(await panel.locator(".mx-ai-input").isDisabled(), "no key: cannot type");

    await panel.locator(".mx-ai-missing-key button").click();
    await page.selectOption("#mx-ai-provider-select", "anthropic");
    await page.fill("#mx-ai-key-input", "sk-test");
    await page.click("#mx-ai-dialog .mx-ai-btn-primary");
    assert.equal(await panel.locator(".mx-ai-missing-key").count(), 0);

    await panel.locator(".mx-ai-input").fill("What is a group?");
    await panel.locator(".mx-ai-btn-send").click();
    const answer = panel.locator(".mx-ai-bubble-assistant").first();
    await answer.locator(".math-display .katex").waitFor();
    assert.match(await answer.textContent(), /A group is a set/);
    assert.equal(await answer.locator(".math-inline .katex").count(), 1);
    assert.equal(await panel.locator(".mx-ai-bubble-user").textContent(), "What is a group?");
    await panel.locator(".mx-ai-btn-send").waitFor();

    const first = requests[0];
    assert.equal(first.headers["x-api-key"], "sk-test");
    assert.ok(!first.url.includes("sk-test"));
    assert.equal(first.body.model, "claude-sonnet-5-5");
    assert.equal(first.body.messages.length, 1);
    const prompt = first.body.messages[0].content;
    const blockText = await block.evaluate((el) => window.Mathia.block(el.dataset.blockId).text);
    assert.ok(prompt.startsWith("Block: "));
    assert.ok(prompt.includes(blockText.slice(0, 40)), "block text is sent");
    assert.ok(prompt.includes("Next block: "), "neighbours are included by default");
    assert.ok(prompt.endsWith("Question: What is a group?"));

    await panel.locator(".mx-ai-input").fill("And then?");
    await panel.locator(".mx-ai-input").press("Enter");
    await panel.locator(".mx-ai-bubble-assistant >> text=Second answer.").waitFor();
    assert.deepEqual(requests[1].body.messages.map((m) => m.role), ["user", "assistant", "user"]);
    assert.equal(requests[1].body.messages[2].content, "And then?", "later turns send only the question");

    await page.reload();
    const again = page.locator(firstBlock).first();
    await again.locator("[data-mx-action=ask]").click();
    assert.equal(await again.locator(".mx-ai-bubble").count(), 4, "conversation survives reload");
    assert.equal(await again.locator(".mx-ai-bubble-assistant .katex").count(), 2);

    await again.locator(".mx-ai-btn-clear").click();
    assert.equal(await again.locator(".mx-ai-bubble").count(), 0);
    assert.equal(
      await page.evaluate(() => Object.keys(sessionStorage).filter((k) => k.startsWith("mathia-ai-chat:")).length),
      0,
    );

    await again.locator("[data-mx-action=ask]").click();
    assert.ok(await again.locator(".mx-ai-panel").isHidden(), "the button toggles the panel");
    await finish(s);
  });

  test("any OpenAI-compatible endpoint, typed into the dialog", async () => {
    const s = await open(pages.en);
    const { page, context } = s;
    const llm = await mock(context, "https://llm.example.test/**", sse.openai(["From ", "a custom server."]));
    const block = page.locator(firstBlock).nth(0);

    await page.click("#mx-ai-toggle");
    await page.selectOption("#mx-ai-provider-select", "openai-compatible");
    assert.ok(await page.isVisible("#mx-ai-endpoint-field"), "endpoint is asked for");
    assert.equal(await page.inputValue("#mx-ai-model-input"), "", "no model is guessed for an unknown server");
    assert.equal(await page.evaluate(() => window.Mathia.ai.available()), false, "needs an endpoint");

    // A pasted full URL is accepted and reduced to the base; no key is needed.
    await page.fill("#mx-ai-endpoint-input", "https://llm.example.test/v1/chat/completions");
    assert.match(await page.textContent("#mx-ai-notice"), /https:\/\/llm\.example\.test/);
    await page.fill("#mx-ai-model-input", "my-model");
    await page.click("#mx-ai-dialog .mx-ai-btn-primary");
    assert.equal(await page.evaluate(() => window.Mathia.ai.available()), true);

    await block.locator("[data-mx-action=ask]").click();
    await block.locator(".mx-ai-input").fill("Q");
    await block.locator(".mx-ai-btn-send").click();
    await block.locator(".mx-ai-bubble-assistant >> text=From a custom server.").waitFor();
    assert.equal(llm[0].url, "https://llm.example.test/v1/chat/completions");
    assert.equal(llm[0].headers.authorization, undefined, "no key, no Authorization header");
    assert.equal(llm[0].body.model, "my-model");
    assert.equal(llm[0].body.stream, true);

    // With a key, it is sent as a bearer token.
    await page.evaluate(() => window.Mathia.ai.configure({ provider: "openai-compatible", key: "sk-any" }));
    await block.locator(".mx-ai-input").fill("Q2");
    await block.locator(".mx-ai-btn-send").click();
    await block.locator(".mx-ai-bubble-assistant").nth(1).waitFor();
    assert.equal(llm[1].headers.authorization, "Bearer sk-any");
    await finish(s);
  });

  test("without neighbours, and through the other providers", async () => {
    const s = await open(pages.en);
    const { page, context } = s;
    const openai = await mock(context, OPENAI, sse.openai(["From ", "OpenAI."]));
    const gemini = await mock(context, GEMINI, sse.gemini(["From ", "Gemini."]));
    const blocks = page.locator(firstBlock);

    await saveKey(page, "openai", "sk-openai");
    await blocks.nth(0).locator("[data-mx-action=ask]").click();
    await blocks.nth(0).locator(".mx-ai-context-toggle").uncheck();
    await blocks.nth(0).locator(".mx-ai-input").fill("Q1");
    await blocks.nth(0).locator(".mx-ai-btn-send").click();
    await blocks.nth(0).locator(".mx-ai-bubble-assistant >> text=From OpenAI.").waitFor();
    assert.equal(openai[0].headers.authorization, "Bearer sk-openai");
    assert.equal(openai[0].body.messages[0].role, "system");
    assert.ok(!openai[0].body.messages[1].content.includes("Context:"));

    await saveKey(page, "gemini", "g-key");
    await blocks.nth(1).locator("[data-mx-action=ask]").click();
    await blocks.nth(1).locator(".mx-ai-input").fill("Q2");
    await blocks.nth(1).locator(".mx-ai-btn-send").click();
    await blocks.nth(1).locator(".mx-ai-bubble-assistant >> text=From Gemini.").waitFor();
    assert.equal(gemini[0].headers["x-goog-api-key"], "g-key");
    assert.ok(!gemini[0].url.includes("g-key"));
    assert.ok(gemini[0].body.contents[0].parts[0].text.includes("Preceding block: "));
    await finish(s);
  });

  test("a provider error is shown in the bubble and not kept", async () => {
    const s = await open(pages.en);
    const { page, context } = s;
    await mock(context, ANTHROPIC, {
      status: 401,
      body: JSON.stringify({ type: "error", error: { type: "authentication_error", message: "invalid x-api-key" } }),
    });
    await saveKey(page, "anthropic", "wrong");
    const block = page.locator(firstBlock).first();
    await block.locator("[data-mx-action=ask]").click();
    await block.locator(".mx-ai-input").fill("Hello?");
    await block.locator(".mx-ai-btn-send").click();
    const error = block.locator(".mx-ai-bubble.mx-ai-error");
    await error.waitFor();
    assert.match(await error.textContent(), /401.*invalid x-api-key/);
    await block.locator(".mx-ai-btn-send").waitFor();
    assert.ok(await block.locator(".mx-ai-input").isEnabled());
    assert.equal(
      await page.evaluate(() => Object.keys(sessionStorage).filter((k) => sessionStorage[k].includes("Hello?")).length),
      0,
    );
    await finish(s);
  });
});

describe("illustrate", () => {
  const svg =
    '<svg viewBox="0 0 120 60" xmlns="http://www.w3.org/2000/svg" onload="window.__pwned = 1">' +
    "<script>window.__pwned = 2</script>" +
    '<circle cx="30" cy="30" r="20" fill="none" stroke="currentColor"/>' +
    '<text x="70" y="34">G</text></svg>';
  const reply = "Here it is.\n\n```svg\n" + svg + "\n```\n\nThe set $G$ drawn as a circle.";

  test("draws a sanitised figure, caches it, regenerates on request", async () => {
    const s = await open(pages.en);
    const { page, context } = s;
    const requests = await mock(context, ANTHROPIC, sse.anthropic([reply.slice(0, 50), reply.slice(50)]));
    await saveKey(page, "anthropic", "sk-test");

    const block = page.locator(firstBlock).first();
    await block.locator("[data-mx-action=illustrate]").click();
    const img = block.locator("img.mx-ai-figure");
    await img.waitFor();
    const src = await img.getAttribute("src");
    assert.ok(src.startsWith("data:image/svg+xml;charset=utf-8,"));
    const drawn = decodeURIComponent(src.slice(src.indexOf(",") + 1));
    assert.ok(drawn.includes("<circle") && drawn.includes(">G</text>"));
    assert.ok(!drawn.includes("script") && !drawn.includes("onload") && !drawn.includes("__pwned"));
    assert.ok(await img.evaluate((el) => el.complete && el.naturalWidth > 0), "the image decodes");
    assert.equal(await page.evaluate(() => window.__pwned), undefined);
    assert.equal(await block.locator("svg circle").count(), 0, "the SVG is never inlined into the page");
    assert.equal(await block.locator(".mx-ai-figure-caption .katex").count(), 1);
    assert.match(await img.getAttribute("alt"), /drawn as a circle/);
    assert.equal(requests.length, 1);
    assert.ok(requests[0].body.system.includes("SVG"));
    assert.ok(requests[0].body.messages[0].content.startsWith("Block: "));

    const cached = await page.evaluate(() => Object.keys(localStorage).filter((k) => k.startsWith("mathia-ai-svg:")));
    assert.equal(cached.length, 1);
    assert.match(cached[0], /:[0-9a-f]{8}$/);

    await page.reload();
    const again = page.locator(firstBlock).first();
    assert.equal(await again.locator("img.mx-ai-figure").count(), 0, "nothing is shown until asked");
    await again.locator("[data-mx-action=illustrate]").click();
    await again.locator("img.mx-ai-figure").waitFor();
    assert.equal(requests.length, 1, "the cached figure needs no request");

    await again.locator(".mx-ai-btn-regenerate").click();
    await again.locator(".mx-ai-btn-regenerate").waitFor();
    assert.equal(requests.length, 2);

    await again.locator("[data-mx-action=illustrate]").click();
    assert.ok(await again.locator(".mx-ai-figure-wrap").isHidden(), "the button toggles the figure");
    await finish(s, { hostileSvg: true });
  });

  test("an answer without a usable SVG is an error, and nothing is cached", async () => {
    const s = await open(pages.en);
    const { page, context } = s;
    await mock(context, ANTHROPIC, sse.anthropic(["I cannot draw this. <svg><unclosed></svg>"]));
    await saveKey(page, "anthropic", "sk-test");
    const block = page.locator(firstBlock).first();
    await block.locator("[data-mx-action=illustrate]").click();
    await block.locator(".mx-ai-figure-status.mx-ai-error").waitFor();
    assert.ok(await block.locator(".mx-ai-figure-box").isHidden());
    assert.equal(
      await page.evaluate(() => Object.keys(localStorage).filter((k) => k.startsWith("mathia-ai-svg:")).length),
      0,
    );
    await finish(s);
  });

  test("a tampered cache entry is sanitised again before display", async () => {
    const s = await open(pages.en);
    const { page } = s;
    await page.evaluate((evil) => {
      const { getSvgStorageKey } = window.Mathia.ai._internal;
      const block = window.Mathia.blocks().find((b) => b.element.querySelector("[data-mx-action=illustrate]"));
      localStorage.setItem(getSvgStorageKey(block), JSON.stringify({ svg: evil, caption: "c" }));
    }, svg);
    const block = page.locator(firstBlock).first();
    await block.locator("[data-mx-action=illustrate]").click();
    const src = await block.locator("img.mx-ai-figure").getAttribute("src");
    assert.ok(!decodeURIComponent(src).includes("script"));
    assert.ok(await block.locator(".mx-ai-btn-regenerate").isHidden(), "no key: no regenerate");
    await finish(s, { hostileSvg: true });
  });

  test("reuses an SVG that came with the imported HTML", async () => {
    const s = await open(pages.figure);
    const { page, context } = s;
    const requests = await mock(context, ANTHROPIC, sse.anthropic(["not used"]));
    await saveKey(page, "anthropic", "sk-test");

    const block = page.locator('section.mx-block[data-mx-figure="true"]').first();
    const original = block.locator("figure").first();
    assert.ok(await original.isVisible(), "the imported figure is part of the prose");

    await block.locator("[data-mx-action=illustrate]").click();
    const img = block.locator("img.mx-ai-figure");
    await img.waitFor();
    const src = await img.getAttribute("src");
    assert.ok(src.startsWith("data:image/svg+xml;charset=utf-8,"));
    const drawn = decodeURIComponent(src.slice(src.indexOf(",") + 1));
    assert.ok(drawn.includes("<circle"));
    assert.match(await img.getAttribute("alt"), /A vertex labelled G/);
    assert.ok(await original.isHidden(), "the prose copy is hidden while the panel is open");
    assert.equal(requests.length, 0, "an imported figure needs no model request");

    await block.locator("[data-mx-action=illustrate]").click();
    assert.ok(await original.isVisible(), "closing the panel brings the prose figure back");
    await finish(s);
  });
});

describe("degradation", () => {
  test("offline hides the controls and says why in an open panel", async () => {
    const s = await open(pages.en);
    const { page, context } = s;
    await saveKey(page, "anthropic", "sk-test");
    const block = page.locator(firstBlock).first();
    await block.locator("[data-mx-action=ask]").click();
    assert.ok(await block.locator(".mx-ai-input").isEnabled());

    await context.setOffline(true);
    await page.waitForFunction(() => document.documentElement.classList.contains("mx-ai-offline"));
    assert.ok(await page.locator("#mx-ai-toggle").isHidden());
    assert.equal(await page.locator("[data-mx-ai-action]:visible").count(), 0);
    assert.ok((await page.locator("[data-mx-ai-action]").count()) > 0);
    assert.equal(await page.evaluate(() => window.Mathia.ai.available()), false);
    assert.ok(await block.locator(".mx-ai-input").isDisabled());
    assert.match(await block.locator(".mx-ai-missing-key").textContent(), /offline/);
    assert.ok(await block.locator(".mx-ai-missing-key button").isHidden());

    await context.setOffline(false);
    await page.waitForFunction(() => !document.documentElement.classList.contains("mx-ai-offline"));
    assert.ok(await page.locator("#mx-ai-toggle").isVisible());
    assert.ok(await block.locator("[data-mx-action=ask]").isVisible());
    assert.ok(await block.locator(".mx-ai-input").isEnabled());
    assert.equal(await block.locator(".mx-ai-missing-key").count(), 0);
    await finish(s);
  });

  test("a document compiled without --ai has no AI runtime at all", async () => {
    const s = await open(pages.plain);
    const { page } = s;
    assert.equal(await page.locator("#mx-ai-toggle, #mx-ai-dialog, #mx-ai, [data-mx-ai-action]").count(), 0);
    assert.equal(await page.evaluate(() => typeof window.Mathia.ai), "undefined");
    assert.equal(await page.evaluate(() => document.body.hasAttribute("data-mx-ai")), false);
    assert.ok((await page.locator(".katex").count()) > 0, "the base runtime still renders math");
    assert.ok(!(await page.content()).includes("connect-src"));
    await finish(s);
  });

  test("Persian documents get Persian controls", async () => {
    const s = await open(pages.fa);
    const { page } = s;
    assert.equal(await page.evaluate(() => document.documentElement.dir), "rtl");
    const block = page.locator(firstBlock).first();
    assert.equal(await block.locator("[data-mx-action=ask]").textContent(), "پرسش");
    assert.equal(await block.locator("[data-mx-action=illustrate]").textContent(), "شکل");
    await block.locator("[data-mx-action=ask]").click();
    assert.equal(await block.locator(".mx-ai-btn-send").textContent(), "ارسال");
    assert.match(await block.locator(".mx-ai-missing-key").textContent(), /کلید/);
    const prompt = await page.evaluate(() => {
      const { assembleIllustratePrompt } = window.Mathia.ai._internal;
      const b = window.Mathia.blocks().find((x) => x.element.querySelector("[data-mx-action=illustrate]"));
      return assembleIllustratePrompt(b);
    });
    assert.ok(prompt.includes("Language of the block: Persian."));
    await finish(s);
  });
});

describe("search", () => {
  test("finds, highlights and steps through matches; clears cleanly", async () => {
    const s = await open(pages.plain);
    const { page } = s;
    const input = page.locator("#mx-search-input");
    assert.ok(await page.locator("#mx-search").isVisible());
    const before = await page.locator("#mx-main").innerHTML();
    await input.fill("group");
    await page.waitForSelector("mark.mx-hit");
    const total = await page.locator("mark.mx-hit").count();
    assert.ok(total > 0);
    assert.match(await page.locator("#mx-search-count").textContent(), new RegExp(`^1 / ${total}$`));
    assert.equal(await page.locator(".mx-hit-current").count(), 1);
    await input.press("Enter");
    assert.match(await page.locator("#mx-search-count").textContent(), /^2 \//);
    await input.fill("zzzqqq-nothing");
    await page.waitForSelector("#mx-search-count.mx-search-empty");
    assert.equal(await page.locator("mark.mx-hit").count(), 0);
    await input.press("Escape");
    assert.equal(await page.locator("#mx-main").innerHTML(), before, "clearing restores the DOM");
    await page.keyboard.press("/");
    assert.equal(await page.evaluate(() => document.activeElement.id), "mx-search-input");
    await finish(s);
  });

  test("Persian search folds Arabic letter variants", async () => {
    const s = await open(pages.fa);
    const { page } = s;
    const n = await page.evaluate(() => window.Mathia.search("ك"));
    assert.equal(typeof n, "number");
    assert.equal(await page.evaluate(() => document.getElementById("mx-search-input").placeholder), "جستجو در متن…");
    await finish(s);
  });
});

describe("copy", () => {
  async function withClipboard(url) {
    const s = await open(url);
    await s.context.grantPermissions(["clipboard-read", "clipboard-write"], { origin: "file://" }).catch(() => {});
    await s.page.evaluate(() => {
      window.__clip = [];
      const write = (t) => { window.__clip.push(t); return Promise.resolve(); };
      Object.defineProperty(navigator, "clipboard", { value: { writeText: write }, configurable: true });
    });
    return s;
  }

  test("every semantic block has a Copy button that copies its text with LaTeX", async () => {
    for (const url of [pages.plain, pages.en]) {
      const s = await withClipboard(url);
      const { page } = s;
      const blocks = await page.locator("section.mx-block:has(.mx-block-head)").count();
      assert.equal(await page.locator("[data-mx-action=copy], button[data-mx-action='copy']").count(), blocks);
      const block = page.locator("section.mx-block:has(.math)").first();
      await block.locator("button[data-mx-action=copy]").click();
      const text = (await page.evaluate(() => window.__clip)).pop();
      assert.match(text, /\$[^$]+\$/, "formulas are copied as LaTeX");
      assert.ok(!text.includes("Copy"), "no UI text in the copy");
      assert.equal(await block.locator("button[data-mx-action=copy]").textContent(), "Copied");
      await finish(s);
    }
  });

  test("formulas copy as LaTeX source", async () => {
    const s = await withClipboard(pages.plain);
    const { page } = s;
    const tex = await page.evaluate(() => document.querySelector(".math-display").dataset.mxTex);
    const display = page.locator(".math-display").first();
    await display.hover();
    await display.locator(".mx-math-copy").click();
    assert.equal((await page.evaluate(() => window.__clip)).pop(), tex);
    const inline = page.locator(".math-inline").first();
    if (await inline.count()) {
      const itex = await inline.evaluate((el) => el.dataset.mxTex);
      await inline.click();
      assert.equal((await page.evaluate(() => window.__clip)).pop(), itex);
    }
    await finish(s);
  });
});

describe("highlights", () => {
  async function select(page, from, to, { backward = false } = {}) {
    return page.evaluate(({ from, to, backward }) => {
      const main = document.getElementById("mx-main");
      const walker = document.createTreeWalker(main, NodeFilter.SHOW_TEXT);
      let startNode = null;
      let startOffset = 0;
      let endNode = null;
      let endOffset = 0;
      let node;
      while ((node = walker.nextNode())) {
        const text = node.nodeValue || "";
        if (!startNode) {
          const at = text.indexOf(from);
          if (at === -1) continue;
          startNode = node;
          startOffset = at;
          const after = text.indexOf(to, at + from.length);
          if (after !== -1) {
            endNode = node;
            endOffset = after + to.length;
            break;
          }
        } else {
          const at = text.indexOf(to);
          if (at !== -1) {
            endNode = node;
            endOffset = at + to.length;
            break;
          }
        }
      }
      if (!startNode || !endNode) throw new Error(`selection text not found: ${from} / ${to}`);
      const sel = window.getSelection();
      sel.removeAllRanges();
      if (backward) sel.setBaseAndExtent(endNode, endOffset, startNode, startOffset);
      else sel.setBaseAndExtent(startNode, startOffset, endNode, endOffset);
      const range = sel.getRangeAt(0);
      return {
        text: range.toString(),
        rects: range.getClientRects().length,
        backward: sel.anchorNode === endNode,
      };
    }, { from, to, backward });
  }

  async function ranges(page) {
    return page.evaluate(() => {
      const highlight = CSS.highlights.get("mathia-highlight");
      return highlight ? Array.from(highlight).map((range) => range.toString()) : [];
    });
  }

  async function waitForToolbar(page) {
    await page.waitForSelector("#mx-highlight-toolbar:not([hidden])");
  }

  test("highlights an RTL lesson with inline LTR math in both directions", async () => {
    const s = await open(pages.fa);
    const { page } = s;
    await page.waitForSelector("#block-definition-1 .katex");
    const before = await page.locator("#mx-main").innerHTML();
    const formula = page.locator("#block-definition-1 .katex").first();
    const mathBefore = await formula.evaluate((el) => el.outerHTML);

    const forward = await select(page, "تابع", "را");
    assert.equal(forward.backward, false);
    assert.match(forward.text, /تابع/);
    assert.match(forward.text, /را/);
    assert.ok(
      await page.evaluate(() => Boolean(document.querySelector("#block-definition-1 .math-inline .katex"))),
      "inline math uses KaTeX's HTML/MathML renderer",
    );
    assert.ok(
      await page.evaluate(() => Array.from(document.styleSheets).some((sheet) => {
        try {
          return Array.from(sheet.cssRules).some((rule) => rule.selectorText === "::highlight(mathia-highlight)");
        } catch (err) {
          return false;
        }
      })),
      "the highlight pseudo-element is styled",
    );
    await waitForToolbar(page);
    assert.equal(await page.locator("#mx-highlight-button").textContent(), "برجسته‌سازی انتخاب");
    await page.locator("#mx-highlight-button").click();

    assert.equal(await page.evaluate(() => Mathia.highlights.count()), 1);
    assert.equal(await page.evaluate(() => window.getSelection().isCollapsed), true);
    await page.waitForSelector("#block-definition-1 [data-mx-highlight-painted]");
    assert.equal(await formula.evaluate((el) => el.outerHTML), mathBefore, "highlighting leaves KaTeX markup alone");
    const first = await ranges(page);
    assert.equal(first.length, 1);
    assert.match(first[0], /تابع/);
    assert.match(first[0], /را/);

    const backward = await select(page, "نتیجه", "است", { backward: true });
    assert.equal(backward.backward, true);
    await page.keyboard.press("Control+Shift+H");
    assert.equal(await page.evaluate(() => Mathia.highlights.count()), 2);
    assert.equal((await ranges(page)).length, 2);
    await page.evaluate(() => Mathia.highlights.clear());
    assert.equal(await page.locator("#mx-main").innerHTML(), before, "clearing removes the painting styles");
    await finish(s);
  });

  test("uses the tallest line for every highlight band in a paragraph", async () => {
    const s = await open(pages.fa);
    const { page } = s;
    await page.evaluate(() => {
      const math = document.querySelector(".math-inline .mfrac").closest(".math-inline").cloneNode(true);
      math.style.fontSize = "2em";
      const p = document.createElement("p");
      p.id = "uniform-highlight";
      p.append("آغاز کوتاه", document.createElement("br"), "فرمول ", math, " در میان متن",
        document.createElement("br"), "پایان کوتاه بدون انتخاب");
      document.getElementById("mx-main").prepend(p);
    });
    const paragraph = page.locator("#uniform-highlight");
    await paragraph.evaluate((el) => {
      window.getSelection().setBaseAndExtent(el.firstChild, 0, el.lastChild, "پایان کوتاه".length);
    });
    await page.keyboard.press("Control+Shift+H");
    await page.waitForSelector("#uniform-highlight[data-mx-highlight-painted]");
    const { heights, lineHeight, mathHeight } = await paragraph.evaluate((el) => ({
      heights: getComputedStyle(el, "::before").backgroundSize.split(", ").map((size) => parseFloat(size.split(" ")[1])),
      lineHeight: parseFloat(getComputedStyle(el).lineHeight),
      mathHeight: el.querySelector(".math-inline").getBoundingClientRect().height,
    }));
    assert.equal(heights.length, 3, "one continuous band per line, including across the inline formula");
    assert.ok(heights.every((height) => height === heights[0]), "all bands use the same height");
    assert.ok(heights[0] > lineHeight && heights[0] >= mathHeight - 0.001, "the tallest formula sets the height");
    await finish(s);
  });

  test("supports multi-element and wrapped selections, and rejects outside the lesson", async () => {
    const s = await open(pages.fa);
    const { page } = s;
    await page.setViewportSize({ width: 420, height: 800 });
    await page.waitForTimeout(100);

    const wrapped = await select(page, "فرض", "می‌شوند");
    assert.ok(wrapped.rects > 1, `selection wraps lines (${wrapped.rects} rects)`);
    await waitForToolbar(page);
    await page.keyboard.press("Control+Shift+H");
    assert.equal(await page.evaluate(() => Mathia.highlights.count()), 1);

    const across = await select(page, "تابع", "هست");
    assert.match(across.text, /تابع/);
    assert.match(across.text, /هست/);
    await page.keyboard.press("Control+Shift+H");
    assert.equal(await page.evaluate(() => Mathia.highlights.count()), 2);

    await page.evaluate(() => {
      const link = document.querySelector("#mx-toc a");
      const text = document.createTreeWalker(link, NodeFilter.SHOW_TEXT).nextNode();
      const sel = window.getSelection();
      sel.removeAllRanges();
      sel.setBaseAndExtent(text, 0, text, Math.min(4, text.nodeValue.length));
    });
    await page.waitForTimeout(50);
    assert.equal(await page.locator("#mx-highlight-toolbar").isHidden(), true);
    await page.keyboard.press("Control+Shift+H");
    assert.equal(await page.evaluate(() => Mathia.highlights.count()), 2, "outside selection is ignored");

    await page.evaluate(() => {
      const link = document.querySelector("#mx-toc a");
      const tocText = document.createTreeWalker(link, NodeFilter.SHOW_TEXT).nextNode();
      const mainText = document.createTreeWalker(document.getElementById("mx-main"), NodeFilter.SHOW_TEXT).nextNode();
      const sel = window.getSelection();
      sel.removeAllRanges();
      sel.setBaseAndExtent(tocText, 0, mainText, Math.min(3, mainText.nodeValue.length));
    });
    await page.waitForTimeout(50);
    assert.equal(await page.locator("#mx-highlight-toolbar").isHidden(), true);
    await page.keyboard.press("Control+Shift+H");
    assert.equal(await page.evaluate(() => Mathia.highlights.count()), 2, "cross-boundary selection is ignored");
    await finish(s);
  });

  test("activates with touch and keyboard without losing the captured selection", async () => {
    const s = await open(pages.fa, { hasTouch: true });
    const { page } = s;
    await select(page, "تابع", "را");
    await waitForToolbar(page);
    const box = await page.locator("#mx-highlight-button").boundingBox();
    assert.ok(box);
    await page.touchscreen.tap(box.x + box.width / 2, box.y + box.height / 2);
    assert.equal(await page.evaluate(() => Mathia.highlights.count()), 1);
    assert.equal(await page.evaluate(() => window.getSelection().isCollapsed), true);

    await select(page, "نتیجه", "است");
    await page.keyboard.press("Control+Shift+H");
    assert.equal(await page.evaluate(() => Mathia.highlights.count()), 2);
    await finish(s);
  });

  test("hides the action when CSS Custom Highlight API is unavailable", async () => {
    const s = await open(pages.plain);
    const { page } = s;
    await page.addInitScript(() => {
      Object.defineProperty(CSS, "highlights", { value: undefined, configurable: true });
    });
    await page.reload();
    assert.equal(await page.evaluate(() => Mathia.highlights.supported), false);
    assert.equal(await page.locator("#mx-highlight-toolbar").count(), 0);
    await select(page, "group", "operation");
    await page.keyboard.press("Control+Shift+H");
    assert.equal(await page.evaluate(() => Mathia.highlights.count()), 0);
    await finish(s);
  });

  test("undoes and redoes highlights from the buttons and the keyboard", async () => {
    const s = await open(pages.fa);
    const { page } = s;
    const count = () => page.evaluate(() => Mathia.highlights.count());
    const undo = page.locator("#mx-highlight-undo");
    const redo = page.locator("#mx-highlight-redo");

    await select(page, "تابع", "را");
    await page.keyboard.press("Control+Shift+H");
    await select(page, "نتیجه", "است");
    await page.keyboard.press("Control+Shift+H");
    assert.equal(await count(), 2);
    assert.equal(await undo.isDisabled(), false);
    assert.equal(await redo.isDisabled(), true);

    await undo.click();
    assert.equal(await count(), 1);
    assert.equal(await redo.isDisabled(), false);

    await page.keyboard.press("Control+Z");
    assert.equal(await count(), 0);
    await page.keyboard.press("Control+Shift+Z");
    assert.equal(await count(), 1);
    await redo.click();
    assert.equal(await count(), 2);

    // A new highlight is a new branch: the redo stack is gone.
    await undo.click();
    assert.equal(await count(), 1);
    await select(page, "فرض", "می‌شوند");
    await page.keyboard.press("Control+Shift+H");
    assert.equal(await count(), 2);
    assert.equal(await redo.isDisabled(), true);
    await finish(s);
  });

  test("cleans up registrations and listeners on destroy", async () => {
    const s = await open(pages.fa);
    const { page } = s;
    await select(page, "تابع", "را");
    await page.keyboard.press("Control+Shift+H");
    assert.equal(await page.evaluate(() => Mathia.highlights.count()), 1);
    assert.equal(await page.evaluate(() => Boolean(CSS.highlights.get("mathia-highlight"))), true);

    await page.evaluate(() => Mathia.highlights.destroy());
    assert.equal(await page.evaluate(() => Mathia.highlights.count()), 0);
    assert.equal(await page.evaluate(() => Boolean(CSS.highlights.get("mathia-highlight"))), false);
    assert.equal(await page.locator("#mx-highlight-toolbar").count(), 0);

    await select(page, "نتیجه", "است");
    await page.waitForTimeout(50);
    assert.equal(await page.locator("#mx-highlight-toolbar").count(), 0);
    await page.keyboard.press("Control+Shift+H");
    assert.equal(await page.evaluate(() => Mathia.highlights.count()), 0);
    await finish(s);
  });
});

describe("host frame", () => {
  test("a click on a block tells the embedding app which block it was", async () => {
    // The desktop app shows the document in a frame and listens for this.
    const dir = mkdtempSync(join(tmpdir(), "mathia-host-"));
    const host = join(dir, "host.html");
    writeFileSync(
      host,
      `<!doctype html><script>window.got = [];
       addEventListener("message", (e) => window.got.push(e.data));</script>
       <iframe id="f" src="${pages.plain}" style="width:900px;height:700px"></iframe>`,
    );
    const s = await open(pathToFileURL(host).href);
    const frame = s.page.frameLocator("#f");
    const body = frame.locator("section.mx-theorem .mx-block-body p").first();
    await body.waitFor();
    const id = await frame.locator("section.mx-theorem").first().getAttribute("data-block-id");
    await body.click({ position: { x: 4, y: 4 } });
    await s.page.waitForFunction(() => window.got.length > 0);
    assert.deepEqual(await s.page.evaluate(() => window.got), [{ mathia: "select", id }]);
    // Buttons (Copy) do not count as selecting a block.
    await frame.locator("section.mx-theorem [data-mx-action=copy]").first().click();
    assert.equal(await s.page.evaluate(() => window.got.length), 1);
    await finish(s);
    rmSync(dir, { recursive: true, force: true });

    // Opened on its own, the document posts nothing.
    const alone = await open(pages.plain);
    const posted = await alone.page.evaluate(() => {
      let count = 0;
      window.addEventListener("message", () => count++);
      document.querySelector("section.mx-theorem .mx-block-body p").click();
      return new Promise((done) => setTimeout(() => done(count), 100));
    });
    assert.equal(posted, 0);
    await finish(alone);
  });

  test("a drag offers Expand block, naming the block and the node it ends in", async () => {
    const dir = mkdtempSync(join(tmpdir(), "mathia-expand-"));
    const host = join(dir, "host.html");
    writeFileSync(
      host,
      `<!doctype html><script>window.got = [];
       addEventListener("message", (e) => window.got.push(e.data));</script>
       <iframe id="f" src="${pages.plain}" style="width:900px;height:700px"></iframe>`,
    );
    const s = await open(pathToFileURL(host).href);
    const frame = s.page.frames().find((f) => f !== s.page.mainFrame());
    await frame.waitForSelector("section.mx-proof .mx-block-body p");

    // The app frame owns undo and the correction buttons; a page on its own does not.
    assert.equal(await frame.locator("#mx-expand-button").count(), 1);
    assert.equal(await frame.locator("#mx-highlight-history").count(), 0);

    const expected = await frame.evaluate(() => {
      const proof = document.querySelector("section.mx-proof");
      const first = proof.querySelector(".mx-block-body p");
      const after = Array.from(document.querySelectorAll("#mx-main .mx-block-body p"))
        .find((p) => p.textContent.startsWith("By Theorem 1.3"));
      if (!after) throw new Error("the plain paragraph after the proof was not found");
      const range = document.createRange();
      range.setStart(first.firstChild, 0);
      range.setEnd(after.lastChild, after.lastChild.nodeValue.length);
      const sel = window.getSelection();
      sel.removeAllRanges();
      sel.addRange(range);
      return {
        proof: proof.dataset.blockId,
        owner: after.closest("[data-block-id]").dataset.blockId,
        text: after.textContent.replace(/\s+/g, " ").trim(),
      };
    });
    await frame.waitForSelector("#mx-highlight-toolbar:not([hidden])");
    await frame.locator("#mx-expand-button").click();
    await s.page.waitForFunction(() => window.got.length > 0);
    assert.deepEqual(await s.page.evaluate(() => window.got), [
      { mathia: "expand", id: expected.proof, endId: expected.owner, text: expected.text },
    ]);
    await finish(s);
    rmSync(dir, { recursive: true, force: true });

    // Opened on its own there is nothing to correct, so only the highlight is offered.
    const alone = await open(pages.plain);
    assert.equal(await alone.page.locator("#mx-expand-button").count(), 0);
    await finish(alone);
  });
});

describe("deterministic mind map", () => {
  test("tree targets, views, collapse, zoom, and document navigation", async () => {
    const s = await open(pages.plain);
    const initial = await s.page.evaluate(() => {
      const tree = window.Mathia.mindmap.tree;
      const walk = (n) => [n, ...n.children.flatMap(walk)];
      const nodes = walk(tree);
      return {
        title: tree.label,
        missing: nodes.filter((n) => !document.getElementById(n.id)).map((n) => n.id),
        proof: nodes.some((n) => /proof|solution/.test(n.node_type)),
        total: nodes.length,
      };
    });
    assert.equal(initial.title, "algebra_en.md");
    assert.deepEqual(initial.missing, []);
    assert.equal(initial.proof, false);
    await s.page.getByRole("button", { name: "Split view", exact: true }).click();
    const visible = () => s.page.locator(".mx-map-node:visible").count();
    assert.equal(await visible(), initial.total);
    const canvas = s.page.locator(".mx-map-world");
    const beforeZoom = await canvas.getAttribute("style");
    await s.page.getByRole("button", { name: "Zoom in", exact: true }).click();
    assert.notEqual(await canvas.getAttribute("style"), beforeZoom);
    await s.page.getByRole("button", { name: "Collapse all", exact: true }).click();
    assert.equal(await visible(), 1);
    await s.page.getByRole("button", { name: "Expand all", exact: true }).click();
    assert.equal(await visible(), initial.total);
    await s.page.getByRole("button", { name: "Mind map", exact: true }).click();
    assert.equal(await s.page.locator("#mx-main").isVisible(), false);
    await s.page.locator('[data-map-id="block-theorem-1"] .mx-map-label').click();
    assert.equal(await s.page.locator("#mx-main").isVisible(), true);
    assert.equal(await s.page.locator("#block-theorem-1").evaluate((el) => el.classList.contains("mathia-scroll-highlight")), true);
    await finish(s);
  });

  test("Persian cards isolate KaTeX and fit their measured height", async () => {
    const s = await open(pages.mindmap);
    await s.page.getByRole("button", { name: "نمای دوگانه", exact: true }).click();
    assert.equal(await s.page.locator(".mx-map-card[dir=auto]").count() > 0, true);
    assert.ok(await s.page.locator(".mx-map-card .katex").count() > 0);
    assert.deepEqual(await s.page.locator(".mx-map-card .katex").evaluateAll((math) => math.map((el) => {
      const style = getComputedStyle(el);
      return { direction: style.direction, bidi: style.unicodeBidi };
    })), Array(await s.page.locator(".mx-map-card .katex").count()).fill({ direction: "ltr", bidi: "isolate" }));
    const bad = await s.page.evaluate(() => Array.from(document.querySelectorAll(".mx-map-card")).filter((card) => {
      const node = card.closest(".mx-map-node");
      return card.scrollHeight > parseFloat(node.style.height);
    }).length);
    assert.equal(bad, 0);
    await finish(s);
  });

  test("HTML cards stay attached to connectors across fit, zoom, and branch changes", async () => {
    for (const url of [pages.plain, pages.mindmap]) {
      const s = await open(url);
      const rtl = url === pages.mindmap;
      await s.page.getByRole("button", { name: rtl ? "نقشه ذهنی" : "Mind map", exact: true }).click();
      async function assertGeometry() {
        // Wait until card position transitions finish before comparing screen
        // coordinates, including the shared HTML camera and SVG edge matrix.
        await s.page.waitForTimeout(300);
        const geometry = await s.page.evaluate(() => {
          const viewport = document.querySelector(".mx-map-canvas");
          const nodes = Array.from(viewport.querySelectorAll(".mx-map-node"));
          const rects = new Map(nodes.map((n) => [n.dataset.mapId, n.querySelector(".mx-map-card").getBoundingClientRect()]));
          const errors = [];
          viewport.querySelectorAll(".mx-map-edge").forEach((edge) => {
            const from = rects.get(edge.dataset.mapFrom), to = rects.get(edge.dataset.mapTo);
            const matrix = edge.getScreenCTM();
            const start = edge.getPointAtLength(0).matrixTransform(matrix);
            const end = edge.getPointAtLength(edge.getTotalLength()).matrixTransform(matrix);
            if (Math.abs(start.x - from.right) > 3 || Math.abs(start.y - (from.top + from.height / 2)) > 3
                || Math.abs(end.x - to.left) > 3 || Math.abs(end.y - (to.top + to.height / 2)) > 3) {
              errors.push(edge.dataset.mapTo);
            }
          });
          return { errors, foreignObjects: viewport.querySelectorAll("foreignObject").length };
        });
        assert.deepEqual(geometry.errors, [], "cards and connector endpoints share screen coordinates");
        assert.equal(geometry.foreignObjects, 0, "interactive KaTeX stays outside SVG on WebKit");
      }
      await assertGeometry();
      await s.page.getByRole("button", { name: rtl ? "بزرگ‌نمایی" : "Zoom in", exact: true }).click();
      await assertGeometry();
      await s.page.getByRole("button", { name: rtl ? "بستن همه" : "Collapse all", exact: true }).click();
      await s.page.getByRole("button", { name: rtl ? "باز کردن همه" : "Expand all", exact: true }).click();
      await assertGeometry();
      await s.page.getByRole("button", { name: rtl ? "جا دادن" : "Fit", exact: true }).click();
      await assertGeometry();
      await finish(s);
    }
  });

  test("dragging moves one card at the current zoom without navigating or panning", async () => {
    const s = await open(pages.plain);
    await s.page.getByRole("button", { name: "Mind map", exact: true }).click();
    await s.page.getByRole("button", { name: "Zoom in", exact: true }).click();
    const node = s.page.locator('[data-map-id="sec-1"]');
    const state = () => node.evaluate((el) => ({ x: parseFloat(el.style.left), y: parseFloat(el.style.top),
      scale: new DOMMatrix(getComputedStyle(document.querySelector(".mx-map-world")).transform).a,
      camera: document.querySelector(".mx-map-world").style.transform }));
    const before = await state(), rect = await node.locator(".mx-map-label").boundingBox();
    await s.page.mouse.move(rect.x + rect.width / 2, rect.y + rect.height / 2);
    await s.page.mouse.down();
    await s.page.mouse.move(rect.x + rect.width / 2 + 80, rect.y + rect.height / 2 - 40, { steps: 8 });
    await s.page.mouse.up();
    const moved = await state();
    assert.ok(Math.abs(moved.x - before.x - 80 / before.scale) < 1);
    assert.ok(Math.abs(moved.y - before.y + 40 / before.scale) < 1);
    assert.equal(moved.camera, before.camera);
    assert.equal(await s.page.locator("#mx-main").isVisible(), false);
    await s.page.getByRole("button", { name: "Collapse all", exact: true }).click();
    await s.page.getByRole("button", { name: "Expand all", exact: true }).click();
    assert.equal((await state()).x, moved.x);
    assert.equal((await state()).y, moved.y);
    await s.page.getByRole("button", { name: "Reset layout", exact: true }).click();
    assert.equal((await state()).x, before.x);
    assert.equal((await state()).y, before.y);
    await node.locator(".mx-map-label").click();
    assert.equal(await s.page.locator("#mx-main").isVisible(), true);
    await finish(s);
  });

  test("SVG and PNG export complete without errors or network access", async () => {
    const s = await open(pages.plain);
    await s.page.getByRole("button", { name: "Mind map", exact: true }).click();
    // Move the root beyond the original origin; exports must include it.
    await s.page.locator('[data-map-id="mx-main"] .mx-map-label').press("Alt+ArrowLeft");
    for (const format of ["SVG", "PNG"]) {
      const pending = s.page.waitForEvent("download", { timeout: 30_000 });
      await s.page.getByRole("button", { name: `Export mind map as ${format}`, exact: true }).click();
      const download = await pending;
      assert.equal(download.suggestedFilename(), `mathia-mindmap.${format.toLowerCase()}`);
      assert.equal(await download.failure(), null);
      if (format === "SVG") {
        const exported = readFileSync(await download.path(), "utf8");
        assert.match(exported, /viewBox="-44 /, "export includes negative card coordinates");
        assert.match(exported, /<foreignObject\b[^>]*\bx="-20"/, "export retains the moved root");
      }
    }
    assert.equal(await s.page.locator(".mx-map-status").textContent(), "");
    await finish(s);
  });
});

describe("obsidian-style graph", () => {
  test("renders every node, filters, and navigates into the document", async () => {
    const s = await open(pages.plain);
    const total = await s.page.evaluate(() => {
      const walk = (node) => [node, ...node.children.flatMap(walk)];
      return walk(window.Mathia.mindmap.tree).length;
    });
    await s.page.getByRole("button", { name: "Graph", exact: true }).click();
    assert.equal(await s.page.locator("#mx-main").isVisible(), false);
    assert.equal(await s.page.locator(".mx-graph-panel").isVisible(), true);
    assert.equal(await s.page.locator(".mx-graph-node").count(), total);
    assert.ok(await s.page.locator(".mx-graph-link").count() >= total - 1);
    assert.ok(await s.page.locator(".mx-graph-link.mx-graph-ref").count() >= 1, "cross-references stay distinct");
    const positions = await s.page.locator(".mx-graph-node").evaluateAll((nodes) => nodes.map((node) => node.getAttribute("transform")));
    assert.equal(new Set(positions).size, total, "force layout separates every node");

    await s.page.locator(".mx-graph-search").fill("Lagrange");
    assert.ok(await s.page.locator(".mx-graph-node.mx-graph-search-match").count() >= 1);
    await s.page.locator(".mx-graph-search").fill("");

    const sections = s.page.getByRole("checkbox", { name: "Sections", exact: true });
    await sections.uncheck();
    assert.equal(await s.page.locator('.mx-graph-node[data-graph-id="sec-1"]').isVisible(), false);
    await sections.check();
    assert.equal(await s.page.locator('.mx-graph-node[data-graph-id="sec-1"]').isVisible(), true);

    await s.page.locator('.mx-graph-node[data-graph-id="block-theorem-1"]').dispatchEvent("click");
    assert.equal(await s.page.locator("#mx-main").isVisible(), true);
    assert.equal(await s.page.locator("#block-theorem-1").evaluate((el) => el.classList.contains("mathia-scroll-highlight")), true);
    await finish(s);
  });
});
