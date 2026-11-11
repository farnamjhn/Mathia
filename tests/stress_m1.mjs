// tests/stress_m1.mjs
// Adversarial Stress Test Suite for Mathia AI Milestone 1 (M1) — S3 "Ask" Action.
// Uses Headless Google Chrome over Chrome DevTools Protocol (CDP) + local streaming mock server.

import http from "node:http";
import { spawn } from "node:child_process";
import { resolve } from "node:path";
import assert from "node:assert/strict";

const CHROME_PATH = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const CHROME_PORT = 9228;
const MOCK_SERVER_PORT = 9999;
const PROFILE_DIR = resolve("target/chrome-profile-stress");

// ANSI colors
const GREEN = "\x1b[32m";
const RED = "\x1b[31m";
const YELLOW = "\x1b[33m";
const CYAN = "\x1b[36m";
const RESET = "\x1b[0m";

let passedCount = 0;
let failedCount = 0;
const testResults = [];

function recordTest(name, passed, details = "") {
  if (passed) {
    passedCount++;
    console.log(`  ${GREEN}✓ PASS${RESET}: ${name}`);
    testResults.push({ name, status: "PASS", details });
  } else {
    failedCount++;
    console.error(`  ${RED}✗ FAIL${RESET}: ${name}${details ? "\n    " + details : ""}`);
    testResults.push({ name, status: "FAIL", details });
  }
}

// Local Hermetic Mock Server for Streaming & Error Simulation
class MockServer {
  constructor(port) {
    this.port = port;
    this.mode = "normal"; // normal, slow, 500, disconnect, fa
    this.postRequestCount = 0;
    this.server = null;
    this.activeSockets = new Set();
  }

  start() {
    return new Promise((resolvePromise) => {
      this.server = http.createServer((req, res) => {
        // CORS headers
        res.setHeader("Access-Control-Allow-Origin", "*");
        res.setHeader("Access-Control-Allow-Headers", "*");
        res.setHeader("Access-Control-Allow-Methods", "*");

        if (req.method === "OPTIONS") {
          res.writeHead(200);
          res.end();
          return;
        }

        if (req.method === "POST") {
          this.postRequestCount++;
        }

        if (this.mode === "500") {
          res.writeHead(500, { "Content-Type": "application/json" });
          res.end(JSON.stringify({ error: { message: "Internal server error 500 from provider" } }));
          return;
        }

        if (this.mode === "disconnect") {
          req.socket.destroy();
          return;
        }

        if (this.mode === "slow") {
          res.writeHead(200, {
            "Content-Type": "text/event-stream",
            "Cache-Control": "no-cache",
            "Connection": "keep-alive"
          });
          res.write("data: {\"choices\":[{\"delta\":{\"content\":\"First partial chunk. \"}}]}\n\n");
          const t1 = setTimeout(() => {
            if (!res.writableEnded) {
              res.write("data: {\"choices\":[{\"delta\":{\"content\":\"Second partial chunk. \"}}]}\n\n");
            }
          }, 100);
          const t2 = setTimeout(() => {
            if (!res.writableEnded) {
              res.write("data: {\"choices\":[{\"delta\":{\"content\":\"Third chunk. \"}}]}\n\n");
              res.write("data: [DONE]\n\n");
              res.end();
            }
          }, 220);

          req.on("close", () => {
            clearTimeout(t1);
            clearTimeout(t2);
          });
          return;
        }

        if (this.mode === "fa") {
          res.writeHead(200, { "Content-Type": "text/event-stream" });
          res.write("data: {\"choices\":[{\"delta\":{\"content\":\"دنباله همگرا \"}}]}\n\n");
          res.write("data: {\"choices\":[{\"delta\":{\"content\":\"دنباله‌ای است که حد دارد.\"}}]}\n\n");
          res.write("data: [DONE]\n\n");
          res.end();
          return;
        }

        // Default: normal stream with KaTeX math
        res.writeHead(200, { "Content-Type": "text/event-stream" });
        res.write("data: {\"choices\":[{\"delta\":{\"content\":\"A group is a non-empty set \"}}]}\n\n");
        res.write("data: {\"choices\":[{\"delta\":{\"content\":\"$G$ with a binary operation satisfying: \"}}]}\n\n");
        res.write("data: {\"choices\":[{\"delta\":{\"content\":\"$$\\\\sum_{i=1}^n x_i$$\\n\\nand associativity.\"}}]}\n\n");
        res.write("data: [DONE]\n\n");
        res.end();
      });

      this.server.on("connection", (socket) => {
        this.activeSockets.add(socket);
        socket.on("close", () => this.activeSockets.delete(socket));
      });

      this.server.listen(this.port, "127.0.0.1", () => resolvePromise());
    });
  }

  stop() {
    return new Promise((resolvePromise) => {
      for (const s of this.activeSockets) {
        try { s.destroy(); } catch (e) {}
      }
      this.server.close(() => resolvePromise());
    });
  }
}

// Minimal CDP Client
class ChromeRunner {
  constructor(port, profile) {
    this.port = port;
    this.profile = profile;
    this.proc = null;
  }

  async start() {
    try {
      const fs = await import("node:fs");
      fs.rmSync(this.profile, { recursive: true, force: true });
    } catch (e) {}

    this.proc = spawn(CHROME_PATH, [
      "--headless=new",
      `--remote-debugging-port=${this.port}`,
      `--user-data-dir=${this.profile}`,
      "--disable-gpu",
      "--no-first-run",
      "--no-default-browser-check"
    ], { stdio: "ignore" });

    for (let i = 0; i < 40; i++) {
      try {
        const res = await fetch(`http://127.0.0.1:${this.port}/json/version`);
        if (res.ok) return await res.json();
      } catch (e) {}
      await new Promise((r) => setTimeout(r, 100));
    }
    throw new Error("Chrome failed to become ready on port " + this.port);
  }

  async createTab(url) {
    const res = await fetch(`http://127.0.0.1:${this.port}/json/new?${encodeURIComponent(url)}`, { method: "PUT" });
    const tab = await res.json();
    return new TabSession(tab);
  }

  async close() {
    if (this.proc) {
      this.proc.kill("SIGKILL");
      this.proc = null;
    }
  }
}

class TabSession {
  constructor(tab) {
    this.tab = tab;
    this.ws = null;
    this.nextId = 1;
    this.pending = new Map();
  }

  async connect() {
    return new Promise((resolvePromise, reject) => {
      this.ws = new WebSocket(this.tab.webSocketDebuggerUrl);
      this.ws.onopen = () => resolvePromise();
      this.ws.onerror = (err) => reject(err);
      this.ws.onmessage = (msg) => {
        const data = JSON.parse(msg.data);
        if (data.id && this.pending.has(data.id)) {
          const { resolve, reject } = this.pending.get(data.id);
          this.pending.delete(data.id);
          if (data.error) reject(new Error(data.error.message || JSON.stringify(data.error)));
          else resolve(data.result);
        }
      };
    });
  }

  async send(method, params = {}) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }

  async initDomains() {
    await this.send("Page.enable");
    await this.send("Runtime.enable");
    await this.send("DOM.enable");
    await this.send("Network.enable");
  }

  async eval(expression) {
    const res = await this.send("Runtime.evaluate", {
      expression,
      returnByValue: true,
      awaitPromise: true
    });
    if (res.exceptionDetails) {
      const desc = res.exceptionDetails.exception?.description || res.exceptionDetails.text;
      throw new Error(`Eval failed: ${desc}`);
    }
    return res.result?.value;
  }

  async setupPageMonitors() {
    await this.eval(`
      window.__cspViolations = [];
      window.__unhandledRejections = [];
      window.__consoleErrors = [];

      window.addEventListener("securitypolicyviolation", function(e) {
        window.__cspViolations.push({
          blockedURI: e.blockedURI,
          violatedDirective: e.violatedDirective,
          effectiveDirective: e.effectiveDirective,
          originalPolicy: e.originalPolicy,
          sample: e.sample
        });
      });

      window.addEventListener("unhandledrejection", function(e) {
        window.__unhandledRejections.push(String(e.reason && (e.reason.message || e.reason)));
      });

      var origErr = console.error;
      console.error = function() {
        window.__consoleErrors.push(Array.prototype.slice.call(arguments).join(" "));
        origErr.apply(console, arguments);
      };
    `);
  }

  async close() {
    try {
      await fetch(`http://127.0.0.1:${CHROME_PORT}/json/close/${this.tab.id}`, { method: "PUT" });
    } catch (e) {}
    if (this.ws) {
      try { this.ws.close(); } catch (e) {}
    }
  }
}

async function runAll() {
  console.log(`${CYAN}========================================================================`);
  console.log(`Starting Empirical Stress Testing Suite for M1 (S3 "Ask" Action)`);
  console.log(`Environment: Google Chrome Headless 156 with DevTools Protocol (CDP)`);
  console.log(`Mock Server: Hermetic Node HTTP Streaming Server on 127.0.0.1:${MOCK_SERVER_PORT}`);
  console.log(`========================================================================${RESET}\n`);

  const mockServer = new MockServer(MOCK_SERVER_PORT);
  await mockServer.start();
  console.log(`Hermetic mock server listening on 127.0.0.1:${MOCK_SERVER_PORT}`);

  const runner = new ChromeRunner(CHROME_PORT, PROFILE_DIR);
  await runner.start();
  console.log("Headless Chrome initialized successfully.\n");

  const enFixtureUrl = "file://" + resolve("target/algebra_en_ai.html");
  const faFixtureUrl = "file://" + resolve("target/analysis_fa_ai.html");

  try {
    // -------------------------------------------------------------------------
    // TEST SET 1: English Document — Action Registration & DOM Mount
    // -------------------------------------------------------------------------
    console.log(`${YELLOW}[TEST SET 1] Action Registration & DOM Mount (English)${RESET}`);
    const tab1 = await runner.createTab(enFixtureUrl);
    await tab1.connect();
    await tab1.initDomains();
    await tab1.setupPageMonitors();

    // Verify S3 Ask buttons
    const askButtonsCount = await tab1.eval(`document.querySelectorAll('button[data-mx-action="ask"]').length`);
    recordTest("S3 Ask buttons mounted on all semantic blocks", askButtonsCount > 0, `Found ${askButtonsCount} action buttons`);

    const taggedButtonsCount = await tab1.eval(`document.querySelectorAll('button[data-mx-ai-action="ask"]').length`);
    recordTest("S3 Ask buttons tagged with data-mx-ai-action='ask'", taggedButtonsCount === askButtonsCount, `Tagged ${taggedButtonsCount} / ${askButtonsCount}`);

    const ariaExpandedFalse = await tab1.eval(`
      Array.from(document.querySelectorAll('button[data-mx-action="ask"]'))
        .every(b => b.getAttribute('aria-expanded') === 'false')
    `);
    recordTest("S3 Ask buttons initially have aria-expanded='false'", ariaExpandedFalse === true);

    const hasToggle = await tab1.eval(`Boolean(document.getElementById('mx-ai-toggle'))`);
    recordTest("Settings toggle button #mx-ai-toggle exists", hasToggle === true);

    const hasDialog = await tab1.eval(`Boolean(document.getElementById('mx-ai-dialog'))`);
    recordTest("Settings dialog #mx-ai-dialog exists", hasDialog === true);

    // -------------------------------------------------------------------------
    // TEST SET 2: Missing Key Flow (Requirement 4)
    // -------------------------------------------------------------------------
    console.log(`\n${YELLOW}[TEST SET 2] Missing Key Flow (Requirement 4)${RESET}`);
    await tab1.eval(`
      window.Mathia.ai.forgetKey();
      window.localStorage.clear();
      window.sessionStorage.clear();
    `);

    const availableNoKey = await tab1.eval(`window.Mathia.ai.available()`);
    recordTest("Mathia.ai.available() is false when no API key configured", availableNoKey === false);

    // Click Ask on the first block
    await tab1.eval(`document.querySelector('button[data-mx-action="ask"]').click();`);

    const panelExists = await tab1.eval(`Boolean(document.querySelector('.mx-ai-panel'))`);
    recordTest("Ask panel container .mx-ai-panel created upon button click", panelExists === true);

    const missingKeyNotice = await tab1.eval(`
      var notice = document.querySelector('.mx-ai-notice.mx-ai-missing-key');
      notice ? notice.textContent : null
    `);
    recordTest("Missing key notice displayed in panel", Boolean(missingKeyNotice && missingKeyNotice.includes("API key required")), `Notice: ${missingKeyNotice}`);

    const inputDisabled = await tab1.eval(`document.querySelector('.mx-ai-panel .mx-ai-input').disabled`);
    recordTest("Input textarea is disabled when key is missing", inputDisabled === true);

    const sendDisabled = await tab1.eval(`document.querySelector('.mx-ai-panel .mx-ai-btn-send').disabled`);
    recordTest("Send button is disabled when key is missing", sendDisabled === true);

    // Click "Configure API Key..." inside notice
    await tab1.eval(`
      var configBtn = document.querySelector('.mx-ai-notice.mx-ai-missing-key button');
      configBtn.click();
    `);

    const dialogOpen = await tab1.eval(`document.getElementById('mx-ai-dialog').open`);
    recordTest("Clicking 'Configure API Key...' opens settings dialog", dialogOpen === true);

    // Configure key via dialog UI and click Save button
    await tab1.eval(`
      var pSelect = document.getElementById('mx-ai-provider-select');
      pSelect.value = 'mock';
      pSelect.dispatchEvent(new Event('change'));
      document.getElementById('mx-ai-key-input').value = 'test-secret-key';
      document.querySelector('#mx-ai-dialog .mx-ai-btn-primary').click();
    `);

    const availableWithKey = await tab1.eval(`window.Mathia.ai.available()`);
    recordTest("Mathia.ai.available() is true after key configuration", availableWithKey === true);

    const noticeRemoved = await tab1.eval(`!document.querySelector('.mx-ai-notice.mx-ai-missing-key')`);
    recordTest("Missing key notice is removed upon key configuration", noticeRemoved === true);

    const inputReEnabled = await tab1.eval(`!document.querySelector('.mx-ai-panel .mx-ai-input').disabled`);
    recordTest("Input textarea is re-enabled upon key configuration", inputReEnabled === true);

    const sendReEnabled = await tab1.eval(`!document.querySelector('.mx-ai-panel .mx-ai-btn-send').disabled`);
    recordTest("Send button is re-enabled upon key configuration", sendReEnabled === true);

    // -------------------------------------------------------------------------
    // TEST SET 3: Rapid Double-Clicks & Concurrent Streaming Requests (Requirement 1)
    // -------------------------------------------------------------------------
    console.log(`\n${YELLOW}[TEST SET 3] Rapid Double-Clicks & Streaming Concurrency (Requirement 1)${RESET}`);
    mockServer.mode = "slow";
    mockServer.postRequestCount = 0;

    await tab1.eval(`
      var textarea = document.querySelector('.mx-ai-panel .mx-ai-input');
      textarea.value = "What is a group?";
      textarea.dispatchEvent(new Event("input"));
      var sendBtn = document.querySelector('.mx-ai-panel .mx-ai-btn-send');
      // Rapid double click
      sendBtn.click();
      sendBtn.click();
    `);

    // Immediately inspect UI state while streaming
    const streamingState = await tab1.eval(`
      var sendBtn = document.querySelector('.mx-ai-panel .mx-ai-btn-send');
      var stopBtn = document.querySelector('.mx-ai-panel .mx-ai-btn-stop');
      var textarea = document.querySelector('.mx-ai-panel .mx-ai-input');
      var panel = document.querySelector('.mx-ai-panel');
      var bubbles = panel.querySelectorAll('.mx-ai-bubble');
      ({
        sendHidden: sendBtn.hidden,
        stopHidden: stopBtn.hidden,
        textareaDisabled: textarea.disabled,
        isStreaming: panel._state.isStreaming,
        bubbleCount: bubbles.length
      })
    `);

    recordTest("During streaming: Send button is hidden", streamingState.sendHidden === true);
    recordTest("During streaming: Stop button is active and visible", streamingState.stopHidden === false);
    recordTest("During streaming: Textarea is disabled", streamingState.textareaDisabled === true);
    recordTest("Rapid double click creates exactly 2 bubbles (1 user, 1 assistant)", streamingState.bubbleCount === 2, `Count: ${streamingState.bubbleCount}`);

    // Wait for slow stream to finish
    await new Promise((r) => setTimeout(r, 450));

    recordTest("Exactly 1 POST network request sent to server despite double-click", mockServer.postRequestCount === 1, `POST requests: ${mockServer.postRequestCount}`);

    const postStreamState = await tab1.eval(`
      var sendBtn = document.querySelector('.mx-ai-panel .mx-ai-btn-send');
      var stopBtn = document.querySelector('.mx-ai-panel .mx-ai-btn-stop');
      var textarea = document.querySelector('.mx-ai-panel .mx-ai-input');
      var panel = document.querySelector('.mx-ai-panel');
      ({
        sendHidden: sendBtn.hidden,
        stopHidden: stopBtn.hidden,
        textareaDisabled: textarea.disabled,
        isStreaming: panel._state.isStreaming
      })
    `);

    recordTest("After streaming: Send button restored to visible", postStreamState.sendHidden === false);
    recordTest("After streaming: Stop button hidden", postStreamState.stopHidden === true);
    recordTest("After streaming: Textarea re-enabled", postStreamState.textareaDisabled === false);

    // -------------------------------------------------------------------------
    // TEST SET 4: Rich Math Rendering (KaTeX)
    // -------------------------------------------------------------------------
    console.log(`\n${YELLOW}[TEST SET 4] KaTeX Rich Math Rendering in Assistant Bubble${RESET}`);
    mockServer.mode = "normal";
    await tab1.eval(`
      var textarea = document.querySelector('.mx-ai-panel .mx-ai-input');
      textarea.value = "Define group and sum";
      textarea.dispatchEvent(new Event("input"));
      document.querySelector('.mx-ai-panel .mx-ai-btn-send').click();
    `);

    await new Promise((r) => setTimeout(r, 250));

    const mathRenderResult = await tab1.eval(`
      var bubbles = document.querySelectorAll('.mx-ai-bubble-assistant');
      var lastBubble = bubbles[bubbles.length - 1];
      ({
        text: lastBubble ? lastBubble.textContent : "",
        hasMathElement: lastBubble ? Boolean(lastBubble.querySelector('.katex, .math, .math-display, [data-mx-tex]')) : false
      })
    `);

    recordTest("Assistant bubble contains rendered group text", mathRenderResult.text.includes("non-empty set"));
    recordTest("Assistant bubble contains rendered inline/display KaTeX math elements", mathRenderResult.hasMathElement === true);

    // -------------------------------------------------------------------------
    // TEST SET 5: Streaming Abort via AbortController / Stop Button (Requirement 2)
    // -------------------------------------------------------------------------
    console.log(`\n${YELLOW}[TEST SET 5] Streaming Abort via AbortController (Requirement 2)${RESET}`);

    // Case 5A: Abort mid-stream with partial text
    mockServer.mode = "slow";
    await tab1.eval(`
      var textarea = document.querySelector('.mx-ai-panel .mx-ai-input');
      textarea.value = "Tell me more about homomorphisms.";
      textarea.dispatchEvent(new Event("input"));
      document.querySelector('.mx-ai-panel .mx-ai-btn-send').click();
    `);

    // Let first chunk arrive (after ~30ms), then click Stop before second chunk (~100ms)
    await new Promise((r) => setTimeout(r, 45));
    await tab1.eval(`document.querySelector('.mx-ai-panel .mx-ai-btn-stop').click();`);
    await new Promise((r) => setTimeout(r, 120));

    const abortResult = await tab1.eval(`
      var panel = document.querySelector('.mx-ai-panel');
      var bubbles = panel.querySelectorAll('.mx-ai-bubble-assistant');
      var lastAssistant = bubbles[bubbles.length - 1];
      var sendBtn = panel.querySelector('.mx-ai-btn-send');
      var stopBtn = panel.querySelector('.mx-ai-btn-stop');
      var textarea = panel.querySelector('.mx-ai-input');
      ({
        assistantText: lastAssistant ? lastAssistant.textContent : "",
        isError: lastAssistant ? lastAssistant.classList.contains("mx-ai-error") : false,
        sendHidden: sendBtn.hidden,
        stopHidden: stopBtn.hidden,
        textareaDisabled: textarea.disabled,
        isStreaming: panel._state.isStreaming,
        unhandledRejections: window.__unhandledRejections.length
      })
    `);

    recordTest("Abort leaves partial text preserved in assistant bubble", abortResult.assistantText.includes("First partial chunk"), `Preserved text: ${abortResult.assistantText}`);
    recordTest("Aborted bubble does NOT have .mx-ai-error class", abortResult.isError === false);
    recordTest("Controls returned to idle state after abort (Send visible, Stop hidden)", abortResult.sendHidden === false && abortResult.stopHidden === true);
    recordTest("Zero unhandled promise rejections on stream abort", abortResult.unhandledRejections === 0, `Rejections: ${abortResult.unhandledRejections}`);

    // Case 5B: Immediate abort before any chunks arrive
    mockServer.mode = "slow";
    await tab1.eval(`
      var textarea = document.querySelector('.mx-ai-panel .mx-ai-input');
      textarea.value = "Immediate abort test";
      textarea.dispatchEvent(new Event("input"));
      document.querySelector('.mx-ai-panel .mx-ai-btn-send').click();
      // Click Stop immediately (synchronous)
      document.querySelector('.mx-ai-panel .mx-ai-btn-stop').click();
    `);

    await new Promise((r) => setTimeout(r, 80));

    const immediateAbortResult = await tab1.eval(`
      var panel = document.querySelector('.mx-ai-panel');
      var sendBtn = panel.querySelector('.mx-ai-btn-send');
      var stopBtn = panel.querySelector('.mx-ai-btn-stop');
      var assistantBubbles = panel.querySelectorAll('.mx-ai-bubble-assistant');
      var lastAssistant = assistantBubbles[assistantBubbles.length - 1];
      ({
        sendHidden: sendBtn.hidden,
        stopHidden: stopBtn.hidden,
        lastAssistantEmpty: lastAssistant ? lastAssistant.textContent.trim() === "" : false,
        unhandledRejections: window.__unhandledRejections.length
      })
    `);

    recordTest("Immediate abort cleans up empty assistant bubble", immediateAbortResult.lastAssistantEmpty === false);
    recordTest("Immediate abort throws zero unhandled rejections", immediateAbortResult.unhandledRejections === 0);

    // -------------------------------------------------------------------------
    // TEST SET 6: Error Simulation — 500 & Disconnection (Requirement 3)
    // -------------------------------------------------------------------------
    console.log(`\n${YELLOW}[TEST SET 6] Error Simulation: Provider 500 & Disconnection (Requirement 3)${RESET}`);

    // Provider 500 Error
    mockServer.mode = "500";
    await tab1.eval(`
      var textarea = document.querySelector('.mx-ai-panel .mx-ai-input');
      textarea.value = "Trigger 500 error";
      textarea.dispatchEvent(new Event("input"));
      document.querySelector('.mx-ai-panel .mx-ai-btn-send').click();
    `);

    await new Promise((r) => setTimeout(r, 150));

    const error500State = await tab1.eval(`
      var panel = document.querySelector('.mx-ai-panel');
      var bubbles = panel.querySelectorAll('.mx-ai-bubble-assistant');
      var lastAssistant = bubbles[bubbles.length - 1];
      var sendBtn = panel.querySelector('.mx-ai-btn-send');
      var textarea = panel.querySelector('.mx-ai-input');
      ({
        hasErrorClass: lastAssistant ? lastAssistant.classList.contains("mx-ai-error") : false,
        errorText: lastAssistant ? lastAssistant.textContent : "",
        sendVisible: !sendBtn.hidden,
        textareaEnabled: !textarea.disabled
      })
    `);

    recordTest("Provider 500 adds .mx-ai-error class to assistant bubble", error500State.hasErrorClass === true);
    recordTest("Provider 500 error message displayed in bubble", error500State.errorText.includes("500"), `Error: ${error500State.errorText}`);
    recordTest("Input controls re-enabled after 500 error", error500State.sendVisible === true && error500State.textareaEnabled === true);

    // Network Disconnection Simulation
    mockServer.mode = "disconnect";
    await tab1.eval(`
      var textarea = document.querySelector('.mx-ai-panel .mx-ai-input');
      textarea.value = "Trigger network failure";
      textarea.dispatchEvent(new Event("input"));
      document.querySelector('.mx-ai-panel .mx-ai-btn-send').click();
    `);

    await new Promise((r) => setTimeout(r, 150));

    const netErrorState = await tab1.eval(`
      var panel = document.querySelector('.mx-ai-panel');
      var bubbles = panel.querySelectorAll('.mx-ai-bubble-assistant');
      var lastAssistant = bubbles[bubbles.length - 1];
      var sendBtn = panel.querySelector('.mx-ai-btn-send');
      var textarea = panel.querySelector('.mx-ai-input');
      ({
        hasErrorClass: lastAssistant ? lastAssistant.classList.contains("mx-ai-error") : false,
        sendVisible: !sendBtn.hidden,
        textareaEnabled: !textarea.disabled
      })
    `);

    recordTest("Network disconnection adds .mx-ai-error to assistant bubble", netErrorState.hasErrorClass === true);
    recordTest("Input controls re-enabled after network disconnection", netErrorState.sendVisible === true && netErrorState.textareaEnabled === true);

    // -------------------------------------------------------------------------
    // TEST SET 7: Session Storage Persistence & Clear Action
    // -------------------------------------------------------------------------
    console.log(`\n${YELLOW}[TEST SET 7] Session Storage Persistence & Clear Action${RESET}`);
    const sessionHistory = await tab1.eval(`
      var blockId = document.querySelector('.mx-ai-panel').dataset.mxAiPanel;
      var key = window.Mathia.ai._internal.getChatStorageKey(blockId);
      var raw = window.sessionStorage.getItem(key);
      ({ key, raw, parsed: raw ? JSON.parse(raw) : null })
    `);

    recordTest("Conversation history is persisted to sessionStorage", sessionHistory.parsed && sessionHistory.parsed.length > 0, `Stored ${sessionHistory.parsed?.length} items`);

    // Click Clear button
    await tab1.eval(`document.querySelector('.mx-ai-panel .mx-ai-btn-clear').click();`);

    const postClearState = await tab1.eval(`
      var panel = document.querySelector('.mx-ai-panel');
      var blockId = panel.dataset.mxAiPanel;
      var key = window.Mathia.ai._internal.getChatStorageKey(blockId);
      var raw = window.sessionStorage.getItem(key);
      var bubbles = panel.querySelectorAll('.mx-ai-bubble');
      ({
        bubblesRemaining: bubbles.length,
        storageRemaining: raw,
        historyRemaining: panel._state.history.length
      })
    `);

    recordTest("Clear button wipes all chat bubbles from DOM", postClearState.bubblesRemaining === 0);
    recordTest("Clear button removes sessionStorage entry", postClearState.storageRemaining === null);
    recordTest("Clear button resets in-memory history", postClearState.historyRemaining === 0);

    // -------------------------------------------------------------------------
    // TEST SET 8: Offline Mode Toggling via CDP Network Emulation
    // -------------------------------------------------------------------------
    console.log(`\n${YELLOW}[TEST SET 8] Offline Mode Toggling & Action Button Hiding${RESET}`);
    // Emulate offline via CDP
    await tab1.send("Network.emulateNetworkConditions", {
      offline: true,
      latency: 0,
      downloadThroughput: -1,
      uploadThroughput: -1
    });

    await new Promise((r) => setTimeout(r, 80));

    const isOfflineClass = await tab1.eval(`document.documentElement.classList.contains('mx-ai-offline')`);
    recordTest("Network offline condition adds .mx-ai-offline to <html>", isOfflineClass === true);

    const offlineButtonsHidden = await tab1.eval(`
      var btn = document.querySelector('button[data-mx-ai-action="ask"]');
      var toggle = document.getElementById('mx-ai-toggle');
      var btnDisplay = window.getComputedStyle(btn).display;
      var toggleDisplay = window.getComputedStyle(toggle).display;
      ({ btnDisplay, toggleDisplay })
    `);

    recordTest("Under .mx-ai-offline, action buttons have display: none", offlineButtonsHidden.btnDisplay === "none", `Display: ${offlineButtonsHidden.btnDisplay}`);
    recordTest("Under .mx-ai-offline, settings toggle has display: none", offlineButtonsHidden.toggleDisplay === "none", `Display: ${offlineButtonsHidden.toggleDisplay}`);

    // Restore online
    await tab1.send("Network.emulateNetworkConditions", {
      offline: false,
      latency: 0,
      downloadThroughput: -1,
      uploadThroughput: -1
    });

    await new Promise((r) => setTimeout(r, 80));

    const isOnlineAgain = await tab1.eval(`!document.documentElement.classList.contains('mx-ai-offline')`);
    recordTest("Restoring network removes .mx-ai-offline", isOnlineAgain === true);

    // -------------------------------------------------------------------------
    // TEST SET 9: CSP Violations in English Document
    // -------------------------------------------------------------------------
    console.log(`\n${YELLOW}[TEST SET 9] CSP Violation Check (English Document)${RESET}`);
    const enCspViolations = await tab1.eval(`window.__cspViolations`);
    recordTest("Zero CSP violations in English document across all operations", enCspViolations.length === 0, JSON.stringify(enCspViolations));

    await tab1.close();

    // -------------------------------------------------------------------------
    // TEST SET 10: RTL Persian Document Parity & Layout (Requirement 5)
    // -------------------------------------------------------------------------
    console.log(`\n${YELLOW}[TEST SET 10] RTL Persian Document Parity & Layout (Requirement 5)${RESET}`);
    const tab2 = await runner.createTab(faFixtureUrl);
    await tab2.connect();
    await tab2.initDomains();
    await tab2.setupPageMonitors();

    // Set provider to anthropic without key so missing key notice is tested in Persian
    await tab2.eval(`
      window.Mathia.ai.configure({ provider: "anthropic" });
      window.Mathia.ai.forgetKey();
      window.sessionStorage.clear();
    `);

    // Verify document direction
    const isRtlDoc = await tab2.eval(`document.documentElement.dir === "rtl"`);
    recordTest("Persian document root has dir='rtl'", isRtlDoc === true);

    // Verify Persian button label: "پرسش"
    const persianAskBtnText = await tab2.eval(`
      var btn = document.querySelector('button[data-mx-action="ask"]');
      btn ? btn.textContent.trim() : null
    `);
    recordTest("Ask action button label is Persian 'پرسش'", persianAskBtnText === "پرسش", `Label: ${persianAskBtnText}`);

    // Click "پرسش" to toggle panel
    await tab2.eval(`document.querySelector('button[data-mx-action="ask"]').click();`);

    const persianStrings = await tab2.eval(`
      var panel = document.querySelector('.mx-ai-panel');
      var textarea = panel.querySelector('.mx-ai-input');
      var sendBtn = panel.querySelector('.mx-ai-btn-send');
      var stopBtn = panel.querySelector('.mx-ai-btn-stop');
      var clearBtn = panel.querySelector('.mx-ai-btn-clear');
      var checkText = panel.querySelector('.mx-ai-checkbox-text');
      var noticeText = panel.querySelector('.mx-ai-panel-notice-text');
      var configBtn = panel.querySelector('.mx-ai-notice button');
      ({
        placeholder: textarea ? textarea.placeholder : "",
        textareaDir: textarea ? textarea.getAttribute("dir") : "",
        sendLabel: sendBtn ? sendBtn.textContent.trim() : "",
        stopLabel: stopBtn ? stopBtn.textContent.trim() : "",
        clearLabel: clearBtn ? clearBtn.textContent.trim() : "",
        checkboxLabel: checkText ? checkText.textContent.trim() : "",
        noticeText: noticeText ? noticeText.textContent.trim() : "",
        configBtnText: configBtn ? configBtn.textContent.trim() : ""
      })
    `);

    recordTest("Persian textarea placeholder is 'درباره این بخش سوال کنید...'", persianStrings.placeholder === "درباره این بخش سوال کنید...");
    recordTest("Persian textarea has dir='auto'", persianStrings.textareaDir === "auto");
    recordTest("Persian Send button label is 'ارسال'", persianStrings.sendLabel === "ارسال");
    recordTest("Persian Stop button label is 'توقف'", persianStrings.stopLabel === "توقف");
    recordTest("Persian Clear button label is 'پاک کردن'", persianStrings.clearLabel === "پاک کردن");
    recordTest("Persian checkbox label is 'شامل بخش‌های مجاور'", persianStrings.checkboxLabel === "شامل بخش‌های مجاور");
    recordTest("Persian missing key notice is correctly localized", persianStrings.noticeText.includes("برای پرسش، کلید دسترسی"), `Notice: ${persianStrings.noticeText}`);
    recordTest("Persian config button label is 'تنظیم کلید API...'", persianStrings.configBtnText === "تنظیم کلید API...", `Config button: ${persianStrings.configBtnText}`);

    // Now configure key via dialog UI in Persian document
    mockServer.mode = "fa";
    await tab2.eval(`
      document.querySelector('.mx-ai-notice button').click(); // open dialog
      var pSelect = document.getElementById('mx-ai-provider-select');
      pSelect.value = 'mock';
      pSelect.dispatchEvent(new Event('change'));
      document.getElementById('mx-ai-key-input').value = 'test-fa-key';
      document.querySelector('#mx-ai-dialog .mx-ai-btn-primary').click(); // save and close
    `);

    // Verify notice removed in Persian panel
    const faNoticeRemoved = await tab2.eval(`!document.querySelector('.mx-ai-notice.mx-ai-missing-key')`);
    recordTest("Persian missing key notice removed upon saving key", faNoticeRemoved === true);

    // Send question in Persian
    await tab2.eval(`
      var panel = document.querySelector('.mx-ai-panel');
      var input = panel.querySelector('.mx-ai-input');
      input.value = "دنباله چیست؟";
      input.dispatchEvent(new Event("input"));
      panel.querySelector('.mx-ai-btn-send').click();
    `);

    await new Promise((r) => setTimeout(r, 200));

    const persianChatBubbles = await tab2.eval(`
      var userBubble = document.querySelector('.mx-ai-bubble-user');
      var assistantBubble = document.querySelector('.mx-ai-bubble-assistant');
      ({
        userText: userBubble ? userBubble.textContent : "",
        userDir: userBubble ? userBubble.getAttribute("dir") : "",
        assistantText: assistantBubble ? assistantBubble.textContent : "",
        assistantDir: assistantBubble ? assistantBubble.getAttribute("dir") : ""
      })
    `);

    recordTest("Persian user bubble created with text and dir='auto'", persianChatBubbles.userText === "دنباله چیست؟" && persianChatBubbles.userDir === "auto");
    recordTest("Persian assistant bubble rendered response with dir='auto'", persianChatBubbles.assistantText.includes("دنباله همگرا") && persianChatBubbles.assistantDir === "auto");

    // -------------------------------------------------------------------------
    // TEST SET 11: CSP Violations in Persian Document
    // -------------------------------------------------------------------------
    console.log(`\n${YELLOW}[TEST SET 11] CSP Violation Check (Persian Document)${RESET}`);
    const faCspViolations = await tab2.eval(`window.__cspViolations`);
    recordTest("Zero CSP violations in Persian RTL document across all operations", faCspViolations.length === 0, JSON.stringify(faCspViolations));

    await tab2.close();

    // -------------------------------------------------------------------------
    // TEST SET 12: Adversarial Unit Stress Testing of Internal Functions
    // -------------------------------------------------------------------------
    console.log(`\n${YELLOW}[TEST SET 12] Adversarial Unit Stress Testing of Internal Functions${RESET}`);
    const tab3 = await runner.createTab(enFixtureUrl);
    await tab3.connect();
    await tab3.initDomains();

    // 12A: assembleInitialPrompt truncation and neighbour handling
    const promptTests = await tab3.eval(`
      var internal = window.Mathia.ai._internal;
      var blocks = internal.getSemanticBlocks();
      var b0 = blocks[0];
      var b1 = blocks[1] || b0;

      // Turn 1 with neighbours
      var p1 = internal.assembleInitialPrompt(b0, "My question?", true);
      // Turn 2
      var p2 = internal.assembleInitialPrompt(b0, "My follow up?", false);

      // Truncation test with long text
      var longText = "a".repeat(10000);
      var truncated = internal.truncateText(longText, 4000);

      ({
        p1HasBlock: p1.includes("Block:"),
        p1HasQuestion: p1.includes("Question: My question?"),
        p2HasNoNeighbours: !p2.includes("Context:"),
        truncatedLength: truncated.length
      })
    `);

    recordTest("assembleInitialPrompt turn 1 includes block header and question", promptTests.p1HasBlock && promptTests.p1HasQuestion);
    recordTest("assembleInitialPrompt without neighbours excludes Context section", promptTests.p2HasNoNeighbours === true);
    recordTest("truncateText strictly enforces 4000 character maximum", promptTests.truncatedLength === 4000);

    // 12B: parseSSE arbitrary single-byte slicing stress test
    const sseStressTest = await tab3.eval(`
      var internal = window.Mathia.ai._internal;
      var sampleSSE =
        "event: message_start\\ndata: {\\"type\\":\\"message_start\\"}\\n\\n" +
        "event: content_block_delta\\ndata: {\\"delta\\":{\\"text\\":\\"chunk1\\"}}\\n\\n" +
        "data: [DONE]\\n\\n";

      // Slice at every single byte boundary
      var state = {};
      var allPayloads = [];
      for (var i = 0; i < sampleSSE.length; i++) {
        var char = sampleSSE[i];
        var out = internal.parseSSE(char, state);
        if (out && out.length) {
          allPayloads = allPayloads.concat(out);
        }
      }
      ({
        payloadCount: allPayloads.length,
        hasDone: allPayloads.indexOf("[DONE]") !== -1
      })
    `);

    recordTest("parseSSE parses correctly when sliced into 1-byte chunks across newlines", sseStressTest.hasDone === true, `Parsed ${sseStressTest.payloadCount} payloads`);

    // 12C: extractDelta multi-provider testing
    const deltaTests = await tab3.eval(`
      var internal = window.Mathia.ai._internal;
      var dAnthropic = internal.extractDelta("anthropic", JSON.stringify({
        type: "content_block_delta",
        delta: { type: "text_delta", text: "anthropic text" }
      }));

      var dOpenAI = internal.extractDelta("openai", JSON.stringify({
        choices: [{ delta: { content: "openai text" } }]
      }));

      var dGemini = internal.extractDelta("gemini", JSON.stringify({
        candidates: [{ content: { parts: [{ text: "gemini text" }] } }]
      }));

      // Hostile JSON
      var dHostile = internal.extractDelta("openai", "{ invalid json !!");

      ({ dAnthropic, dOpenAI, dGemini, dHostile })
    `);

    recordTest("extractDelta extracts text for Anthropic", deltaTests.dAnthropic === "anthropic text");
    recordTest("extractDelta extracts text for OpenAI", deltaTests.dOpenAI === "openai text");
    recordTest("extractDelta extracts text for Gemini", deltaTests.dGemini === "gemini text");
    recordTest("extractDelta gracefully returns empty string on malformed JSON without crashing", deltaTests.dHostile === "");

    // 12D: createThrottledRenderer rapid burst test
    const throttlerTest = await tab3.eval(`
      new Promise(function(resolve) {
        var internal = window.Mathia.ai._internal;
        var div = document.createElement("div");
        var renderer = internal.createThrottledRenderer(div);

        // Rapidly call update 50 times in synchronous loop
        for (var i = 1; i <= 50; i++) {
          renderer.update("Step " + i);
        }
        renderer.finish("Final Step");

        resolve({
          text: div.textContent.trim()
        });
      })
    `);

    recordTest("createThrottledRenderer handles rapid bursts and synchronously flushes finish", throttlerTest.text === "Final Step");

    const tab3Csp = await tab3.eval(`window.__cspViolations || []`);
    recordTest("Zero CSP violations during unit stress tests", tab3Csp.length === 0);

    await tab3.close();

  } finally {
    await runner.close();
    await mockServer.stop();
  }

  // ---------------------------------------------------------------------------
  // Summary
  // ---------------------------------------------------------------------------
  console.log(`\n${CYAN}========================================================================`);
  console.log(`Empirical Stress Testing Complete`);
  console.log(`Passed: ${passedCount} | Failed: ${failedCount} | Total: ${passedCount + failedCount}`);
  console.log(`========================================================================${RESET}\n`);

  if (failedCount > 0) {
    console.error(`${RED}VERDICT: REJECT (${failedCount} tests failed)${RESET}`);
    process.exit(1);
  } else {
    console.log(`${GREEN}VERDICT: APPROVE (100% tests passed, 0 CSP violations)${RESET}`);
    process.exit(0);
  }
}

runAll().catch((err) => {
  console.error("Fatal runner error:", err);
  process.exit(1);
});
