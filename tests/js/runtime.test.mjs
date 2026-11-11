import assert from "node:assert/strict";
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
