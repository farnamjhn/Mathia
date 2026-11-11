// Mathia desktop frontend. No framework and no build step: this file is loaded
// as-is and talks to the Rust side through the global Tauri API
// (`app.withGlobalTauri`). All text is set with `textContent`; nothing here
// parses HTML. The compiled document lives in the sandboxed preview frame.
(function () {
  "use strict";

  const { invoke, convertFileSrc } = window.__TAURI__.core;
  const { listen } = window.__TAURI__.event;

  const PREVIEW_SCHEME = "mathia-preview";
  const MARKDOWN = /\.(md|markdown|mdown|txt)$/i;
  const AI_PROVIDERS = ["anthropic", "openai", "gemini", "openai-compatible"];

  const $ = (id) => document.getElementById(id);
  const el = {
    open: $("open"), reload: $("reload"), export: $("export"),
    ai: $("ai"), noMath: $("no-math"), showText: $("show-text"),
    status: $("status"), blocks: $("blocks"), frame: $("frame"), empty: $("empty"),
    drop: $("drop"), detail: $("detail"), detailTitle: $("detail-title"),
    detailFacts: $("detail-facts"), detailScores: $("detail-scores"),
    detailFeatures: $("detail-features"),
    guide: $("guide"), guideDialog: $("guide-dialog"), guideClose: $("guide-close"),
    viewPrompt: $("view-prompt"), promptText: $("prompt-text"),
    copyPrompt: $("copy-prompt"), copyPromptEmpty: $("copy-prompt-empty"),
  };

  /** @type {{path: string} | null} */
  let source = null;
  let analysis = null;
  let revision = 0;
  let selected = null;
  let busy = false;

  function setStatus(text, isError) {
    el.status.textContent = text;
    el.status.classList.toggle("error", Boolean(isError));
  }

  function options() {
    return {
      noMath: el.noMath.checked,
      aiProviders: el.ai.checked ? AI_PROVIDERS : [],
    };
  }

  function fileName(path) {
    return path.split(/[\\/]/).pop();
  }

  function node(tag, className, text) {
    const n = document.createElement(tag);
    if (className) n.className = className;
    if (text !== undefined) n.textContent = text;
    return n;
  }

  function previewUrl(blockId) {
    const base = convertFileSrc(String(revision), PREVIEW_SCHEME);
    return blockId ? base + "#" + encodeURIComponent(blockId) : base;
  }

  // Compile and analyse the current source; the frame reloads only when the
  // HTML actually changed, so its scroll position survives a no-op refresh.
  async function refresh(quiet) {
    if (!source || busy) return;
    busy = true;
    if (!quiet) setStatus("Compiling…");
    try {
      const compiled = await invoke("compile", { source, options: options() });
      if (compiled.changed || compiled.revision !== revision) {
        revision = compiled.revision;
        el.frame.src = previewUrl(selected);
        analysis = await invoke("analyze", { source });
        renderBlocks();
      }
      el.frame.hidden = false;
      el.empty.hidden = true;
      el.reload.disabled = el.export.disabled = false;
      document.title = fileName(source.path) + " — Mathia";
      setStatus(
        fileName(source.path) + " · " + compiled.semantic_blocks + " semantic blocks · " +
          Math.round(compiled.bytes / 1024) + " kB"
      );
    } catch (error) {
      setStatus(String(error), true);
    } finally {
      busy = false;
    }
  }

  async function openPath(path) {
    source = { path };
    selected = null;
    await refresh();
  }

  async function open() {
    try {
      const path = await invoke("pick_markdown");
      if (path) await openPath(path);
    } catch (error) {
      setStatus(String(error), true);
    }
  }

  async function exportHtml() {
    if (!source) return;
    try {
      const dest = await invoke("export", { source, options: options() });
      if (dest) setStatus("Exported to " + dest);
    } catch (error) {
      setStatus(String(error), true);
    }
  }

  function blockClass(kind) {
    return kind === "heading" || kind === "text" ? kind : "semantic";
  }

  function renderBlocks() {
    el.blocks.replaceChildren();
    if (!analysis) return;
    const showText = el.showText.checked;
    for (const block of analysis.document.blocks) {
      if (block.kind === "text" && !showText) continue;
      const button = node("button", "block " + blockClass(block.kind));
      button.type = "button";
      button.dataset.id = block.id;
      if (block.id === selected) button.setAttribute("aria-current", "true");
      button.append(node("span", "kind", block.kind));
      const title = node("span", "block-title", block.title || block.id);
      title.dir = "auto";
      button.append(title);
      button.append(node("span", "block-score", block.score > 0 ? block.score.toFixed(2) : ""));
      const item = node("li");
      item.append(button);
      el.blocks.append(item);
    }
    const current = analysis.document.blocks.find((b) => b.id === selected);
    if (current) renderDetail(current);
    else {
      selected = null;
      el.detail.hidden = true;
    }
  }

  function fact(name, value) {
    if (value === null || value === undefined || value === "") return;
    el.detailFacts.append(node("dt", "", name), node("dd", "", String(value)));
  }

  function renderDetail(block) {
    el.detail.hidden = false;
    el.detailTitle.textContent = block.title || block.id;
    el.detailFacts.replaceChildren();
    fact("Id", block.id);
    fact("Type", block.kind);
    fact("Score", block.score > 0 ? block.score.toFixed(2) + " (threshold " + analysis.threshold.toFixed(2) + ")" : null);
    fact("Nodes", block.span.start + "–" + (block.span.end - 1) + " (" + (block.span.end - block.span.start) + ")");
    fact("Ended by", block.end);
    fact("Number", block.number);
    fact("Name", block.name);
    fact("Proves", block.proves);
    fact("Direction", block.direction);

    el.detailScores.replaceChildren();
    const opener = analysis.scores[block.span.start];
    const ranked = opener ? opener.scores.filter((s) => s[1] > 0).sort((a, b) => b[1] - a[1]) : [];
    for (const [kind, score] of ranked) {
      const row = node("li");
      const meter = node("meter");
      meter.min = 0;
      meter.max = 1;
      meter.value = Math.min(score, 1);
      row.append(node("span", "", kind), meter, node("span", "num", score.toFixed(2)));
      el.detailScores.append(row);
    }
    if (!ranked.length) el.detailScores.append(node("li", "", "none"));
    el.detailFeatures.textContent = block.features ? JSON.stringify(block.features, null, 2) : "—";
  }

  function select(id) {
    selected = id;
    for (const button of el.blocks.querySelectorAll(".block")) {
      if (button.dataset.id === id) button.setAttribute("aria-current", "true");
      else button.removeAttribute("aria-current");
    }
    const block = analysis && analysis.document.blocks.find((b) => b.id === id);
    if (block) renderDetail(block);
    // Changing only the fragment scrolls the frame without reloading it.
    el.frame.src = previewUrl(id);
  }

  // The OCR prompt lives in a read-only <textarea> in index.html, so what the
  // user sees is exactly what gets copied.
  function copyViaSelection(text) {
    const area = node("textarea", "offscreen");
    area.value = text;
    document.body.append(area);
    area.select();
    const ok = document.execCommand("copy");
    area.remove();
    if (!ok) throw new Error("copy command was refused");
  }

  async function copyPrompt(button) {
    const text = el.promptText.value;
    try {
      if (navigator.clipboard && navigator.clipboard.writeText) await navigator.clipboard.writeText(text);
      else copyViaSelection(text);
    } catch (_) {
      try {
        copyViaSelection(text);
      } catch (error) {
        setStatus("Could not copy the prompt: " + error.message, true);
        return;
      }
    }
    setStatus("OCR prompt copied — paste it into your OCR / vision model");
    const label = button.dataset.label || button.textContent;
    button.dataset.label = label;
    button.textContent = "Copied";
    clearTimeout(button.copyTimer);
    button.copyTimer = setTimeout(() => { button.textContent = label; }, 1600);
  }

  function showGuide() {
    if (!el.guideDialog.open) el.guideDialog.showModal();
    el.promptText.scrollTop = 0;
    el.copyPrompt.focus();
  }

  el.guide.addEventListener("click", showGuide);
  el.viewPrompt.addEventListener("click", showGuide);
  el.guideClose.addEventListener("click", () => el.guideDialog.close());
  el.copyPrompt.addEventListener("click", () => copyPrompt(el.copyPrompt));
  el.copyPromptEmpty.addEventListener("click", () => copyPrompt(el.copyPromptEmpty));
  // A click on the backdrop (outside the dialog box) closes it.
  el.guideDialog.addEventListener("click", (event) => {
    if (event.target === el.guideDialog) el.guideDialog.close();
  });

  el.open.addEventListener("click", open);
  el.reload.addEventListener("click", () => refresh());
  el.export.addEventListener("click", exportHtml);
  el.ai.addEventListener("change", () => refresh());
  el.noMath.addEventListener("change", () => refresh());
  el.showText.addEventListener("change", renderBlocks);
  el.blocks.addEventListener("click", (event) => {
    const button = event.target.closest(".block");
    if (button) select(button.dataset.id);
  });

  window.addEventListener("keydown", (event) => {
    if (el.guideDialog.open) return;
    if (!(event.metaKey || event.ctrlKey) || event.altKey) return;
    const key = event.key.toLowerCase();
    if (key === "o") open();
    else if (key === "e") exportHtml();
    else if (key === "r") refresh();
    else return;
    event.preventDefault();
  });

  // Pick up edits made in another program when the window comes back.
  window.addEventListener("focus", () => refresh(true));

  invoke("startup_file")
    .then((path) => { if (path) openPath(path); })
    .catch((error) => setStatus(String(error), true));

  listen("tauri://drag-enter", () => { el.drop.hidden = false; });
  listen("tauri://drag-leave", () => { el.drop.hidden = true; });
  listen("tauri://drag-drop", (event) => {
    el.drop.hidden = true;
    const path = (event.payload.paths || []).find((p) => MARKDOWN.test(p));
    if (path) openPath(path);
    else setStatus("Drop a Markdown file (.md, .markdown, .txt)", true);
  });
})();
