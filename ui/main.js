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
    ai: $("ai"), math: $("render-math"), showText: $("show-text"),
    status: $("status"), blocks: $("blocks"), panes: $("panes"),
    preview: $("preview"), previewToggle: $("preview-toggle"),
    frame: $("frame"), empty: $("empty"), inspector: $("inspector"),
    docName: $("doc-name"), openWelcome: $("open-welcome"),
    filter: $("filter"), views: $("views"), blocksEmpty: $("blocks-empty"),
    countAll: $("count-all"), countUnsure: $("count-unsure"), countChanged: $("count-changed"),
    optionsButton: $("options-button"), optionsMenu: $("options-menu"),
    exportMore: $("export-more"), exportMenu: $("export-menu"),
    detailClose: $("detail-close"), detailNote: $("detail-note"), fixLength: $("fix-length"),
    drop: $("drop"), detail: $("detail"), detailTitle: $("detail-title"),
    detailFacts: $("detail-facts"), detailScores: $("detail-scores"),
    detailFeatures: $("detail-features"),
    guide: $("guide"), guideDialog: $("guide-dialog"), guideClose: $("guide-close"),
    viewPrompt: $("view-prompt"), promptText: $("prompt-text"),
    copyPrompt: $("copy-prompt"), copyPromptEmpty: $("copy-prompt-empty"),
    exportFolder: $("export-folder"),
    exportPdf: $("export-pdf"),
    recent: $("recent"), recentList: $("recent-list"), recentClear: $("recent-clear"),
    recentWrap: $("recent-wrap"), recentButton: $("recent-button"), recentMenu: $("recent-menu"),
    corrections: $("corrections"), correctionsCount: $("corrections-count"),
    correctionsLoad: $("corrections-load"), correctionsSave: $("corrections-save"),
    correctionsClear: $("corrections-clear"),
    fixKind: $("fix-kind"), fixShorter: $("fix-shorter"), fixLonger: $("fix-longer"),
    fixReject: $("fix-reject"), fixReset: $("fix-reset"),
    scoring: $("scoring"), configDialog: $("config-dialog"), configForm: $("config-form"),
    cfgScorer: $("cfg-scorer"), cfgThreshold: $("cfg-threshold"), cfgWeights: $("cfg-weights"),
    cfgScope: $("cfg-scope"), cfgKeywords: $("cfg-keywords"), cfgCues: $("cfg-cues"),
    configError: $("config-error"), configLoad: $("config-load"), configSave: $("config-save"),
    decision: $("decision"), decisionRun: $("decision-run"), decisionDialog: $("decision-dialog"),
    dmProvider: $("dm-provider"), dmModel: $("dm-model"), dmUrl: $("dm-url"), dmKey: $("dm-key"),
    dmKeyHint: $("dm-key-hint"), dmRemoveKey: $("dm-remove-key"), dmTest: $("dm-test"),
    dmCancel: $("dm-cancel"), dmSave: $("dm-save"), decisionMessage: $("decision-message"),
    configReset: $("config-reset"), configCancel: $("config-cancel"), configApply: $("config-apply"),
  };

  const SEMANTIC = [
    "theorem", "lemma", "proposition", "corollary", "definition", "proof", "example",
    "remark", "exercise", "solution", "claim", "notation",
  ];
  const CUE_LISTS = [
    "logic_symbols", "qed_markers", "definitional_cues", "proof_cues",
    "statement_cues", "continuation_cues", "reference_cues",
  ];
  /** How each block type is written in the list, as a textbook margin would. */
  const SHORT = {
    theorem: "Thm", lemma: "Lem", proposition: "Prop", corollary: "Cor", definition: "Def",
    proof: "Proof", example: "Ex", remark: "Rem", exercise: "Exer", solution: "Sol",
    claim: "Claim", notation: "Not", heading: "§", text: "¶",
  };
  /** A detected block scoring less than this above the threshold is listed
   *  as "unsure": the ones worth a second look. */
  const UNSURE_MARGIN = 0.1;
  /** How often the open file is checked for changes made elsewhere (ms). */
  const WATCH_INTERVAL = 1000;

  /** @type {{path: string} | null} */
  let source = null;
  let analysis = null;
  let revision = 0;
  let selected = null;
  /** First node of the selected block: keeps the selection when a
   *  correction changes the block's id. */
  let selectedStart = null;
  let busy = false;
  let again = false;
  let stamp = null;
  /** Hand corrections (math_core::ScopeOverrides), keyed by opener node. */
  let overrides = emptyOverrides();
  /** The configuration shown in the detection settings dialog. */
  let editing = null;
  /** Which blocks the list shows: "all", "unsure" or "changed". */
  let view = "all";
  /** Whether the user wants the preview pane (it only shows with a document). */
  let previewOpen = true;

  function emptyOverrides() {
    return { delta: {}, suppressed: [], retype: {}, end: {} };
  }

  function setStatus(text, isError) {
    el.status.textContent = text;
    el.status.classList.toggle("error", Boolean(isError));
  }

  function options() {
    return {
      noMath: !el.math.checked,
      aiProviders: el.ai.checked ? AI_PROVIDERS : [],
    };
  }

  function fileName(path) {
    return path.split(/[\\/]/).pop();
  }

  function titleCase(word) {
    return word.charAt(0).toUpperCase() + word.slice(1);
  }

  function kindLabel(kind) {
    return kind === "text" ? "Plain text" : titleCase(kind);
  }

  // The welcome screen stands in for both panes until a document compiles.
  function layout() {
    const has = Boolean(analysis);
    el.empty.hidden = has;
    el.inspector.hidden = !has;
    el.preview.hidden = !has || !previewOpen;
    el.panes.classList.toggle("preview-closed", has && !previewOpen);
    el.previewToggle.disabled = !has;
    updateDecisionRun();
    el.reload.disabled = el.export.disabled = el.exportPdf.disabled = !has;
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
    if (!source) return;
    if (busy) {
      // Run once more when the current compile is done, so a correction made
      // meanwhile is not lost.
      again = true;
      return;
    }
    busy = true;
    if (!quiet) setStatus("Compiling…");
    try {
      const compiled = await invoke("compile", { source, options: options() });
      if (compiled.changed || compiled.revision !== revision || !analysis) {
        revision = compiled.revision;
        analysis = await invoke("analyze", { source });
        renderBlocks();
        el.frame.src = previewUrl(selected);
      }
      layout();
      const name = fileName(source.path);
      document.title = name + " — Mathia";
      el.docName.textContent = name;
      el.docName.title = source.path;
      const found = compiled.semantic_blocks;
      setStatus(
        (found === 1 ? "1 block found" : found + " blocks found") + ", " +
          Math.round(compiled.bytes / 1024) + " kB of HTML"
      );
    } catch (error) {
      layout();
      setStatus(String(error), true);
    } finally {
      busy = false;
      if (again) {
        again = false;
        refresh(true);
      }
    }
  }

  async function openPath(path) {
    source = { path };
    selected = null;
    selectedStart = null;
    analysis = null;
    stamp = null;
    overrides = emptyOverrides();
    view = "all";
    el.filter.value = "";
    try {
      await invoke("set_overrides", { overrides });
    } catch (error) {
      setStatus(String(error), true);
      return;
    }
    await refresh();
  }

  // ── Recent files ──────────────────────────────────────────────────────

  function parentDir(path) {
    const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
    return i > 0 ? path.slice(0, i) : "";
  }

  // The same list is shown on the welcome screen and in the Recent menu.
  function recentButton(path, className) {
    const button = node("button", className);
    button.type = "button";
    button.title = path;
    button.append(node("span", "recent-name", fileName(path)));
    button.append(node("span", "recent-dir", parentDir(path)));
    button.addEventListener("click", () => openRecent(path));
    return button;
  }

  async function loadRecent() {
    let paths = [];
    try {
      paths = await invoke("recent_files");
    } catch (_) {
      // The list is a convenience; the welcome screen works without it.
    }
    el.recentList.replaceChildren(...paths.map((path) => {
      const item = node("li");
      item.append(recentButton(path, "recent-item"));
      return item;
    }));
    el.recentMenu.replaceChildren(
      ...paths.map((path) => recentButton(path, "menu-item recent-item"))
    );
    el.recent.hidden = paths.length === 0;
    el.recentWrap.hidden = paths.length === 0;
  }

  async function openRecent(path) {
    try {
      await openPath(await invoke("open_recent", { path }));
    } catch (error) {
      setStatus(String(error), true);
    }
    loadRecent();
  }

  async function open() {
    try {
      const path = await invoke("pick_markdown");
      if (path) await openPath(path);
    } catch (error) {
      setStatus(String(error), true);
    }
    loadRecent();
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

  async function exportPdf() {
    if (!source) return;
    try {
      await invoke("export_pdf", { source, options: options() });
      setStatus("Opened the print dialog: choose “Save as PDF”.");
    } catch (error) {
      setStatus(String(error), true);
    }
  }

  function blockClass(kind) {
    return kind === "heading" || kind === "text" ? kind : "semantic";
  }

  function isSemantic(block) {
    return block.kind !== "text" && block.kind !== "heading";
  }

  function unsure(block) {
    return isSemantic(block) && !corrected(block.span.start) && block.score > 0 &&
      block.score < analysis.threshold + UNSURE_MARGIN;
  }

  function matches(block, query) {
    if (!query) return true;
    const text = (block.title || "") + " " + block.kind + " " + block.id;
    return text.toLowerCase().includes(query);
  }

  function listed(block, query) {
    if (!matches(block, query)) return false;
    if (view === "unsure") return unsure(block);
    if (view === "changed") return corrected(block.span.start);
    if (block.kind === "heading") return !query;
    // A paragraph the user demoted stays listed, so the change can be undone.
    if (block.kind === "text") return el.showText.checked || corrected(block.span.start);
    return true;
  }

  function renderBlocks() {
    el.blocks.replaceChildren();
    if (!analysis) return;
    const blocks = analysis.document.blocks;
    const query = el.filter.value.trim().toLowerCase();
    let shown = 0;
    for (const block of blocks) {
      if (!listed(block, query)) continue;
      shown += 1;
      const button = node("button", "block " + blockClass(block.kind));
      button.type = "button";
      button.dataset.id = block.id;
      const kind = node("span", "kind", SHORT[block.kind] || block.kind);
      kind.title = kindLabel(block.kind);
      button.append(kind);
      const title = node("span", "block-title", block.title || kindLabel(block.kind));
      title.dir = "auto";
      button.append(title);
      if (corrected(block.span.start)) button.append(node("span", "mark changed", "changed"));
      else if (unsure(block)) button.append(node("span", "mark unsure", "unsure"));
      const item = node("li");
      item.append(button);
      el.blocks.append(item);
    }

    el.countAll.textContent = String(blocks.filter(isSemantic).length);
    el.countUnsure.textContent = String(blocks.filter(unsure).length);
    el.countChanged.textContent = String(blocks.filter((b) => corrected(b.span.start)).length);
    for (const button of el.views.querySelectorAll("button")) {
      button.setAttribute("aria-pressed", String(button.dataset.view === view));
    }
    el.blocksEmpty.hidden = shown > 0;
    el.blocksEmpty.textContent = query ? "No block matches “" + el.filter.value.trim() + "”."
      : view === "unsure" ? "Nothing here. Mathia is confident about every block it found."
      : view === "changed" ? "You have not changed any block."
      : "Mathia found no blocks in this file. Show plain paragraphs to mark some yourself.";

    const current = blocks.find((b) => b.id === selected) ||
      (selectedStart !== null &&
        blocks.find((b) => b.span.start <= selectedStart && selectedStart < b.span.end));
    if (current) {
      selected = current.id;
      for (const button of el.blocks.querySelectorAll(".block")) {
        if (button.dataset.id === selected) button.setAttribute("aria-current", "true");
      }
      renderDetail(current);
    } else {
      deselect();
    }
    renderCorrections();
  }

  function deselect() {
    selected = null;
    selectedStart = null;
    el.detail.hidden = true;
    for (const button of el.blocks.querySelectorAll('.block[aria-current="true"]')) {
      button.removeAttribute("aria-current");
    }
  }

  function fact(name, value) {
    if (value === null || value === undefined || value === "") return;
    el.detailFacts.append(node("dt", "", name), node("dd", "", String(value)));
  }

  function renderDetail(block) {
    el.detail.hidden = false;
    el.detailTitle.textContent = block.title || kindLabel(block.kind);
    const note = corrected(block.span.start) ? "You changed this block."
      : unsure(block) ? "Mathia is not sure about this one. Check its type and where it ends."
      : block.kind === "text" ? "An ordinary paragraph. Choose a type to turn it into a block."
      : "";
    el.detailNote.textContent = note;
    el.detailNote.hidden = !note;
    el.detailNote.className = "detail-note" +
      (corrected(block.span.start) ? " changed" : unsure(block) ? " unsure" : "");

    el.detailFacts.replaceChildren();
    fact("Confidence", block.score > 0 ? block.score.toFixed(2) + " (needs " + analysis.threshold.toFixed(2) + ")" : null);
    fact("Number", block.number);
    fact("Name", block.name);
    fact("Proves", block.proves);
    fact("Ended by", block.end ? block.end.replace(/_/g, " ") : null);
    fact("Parts", block.span.start + "–" + (block.span.end - 1));
    fact("Direction", block.direction);
    fact("Id", block.id);

    el.detailScores.replaceChildren();
    const opener = analysis.scores[block.span.start];
    const ranked = opener ? opener.scores.filter((s) => s[1] > 0).sort((a, b) => b[1] - a[1]) : [];
    for (const [kind, score] of ranked) {
      const row = node("li");
      const meter = node("meter");
      meter.min = 0;
      meter.max = 1;
      meter.value = Math.min(score, 1);
      row.append(node("span", "", titleCase(kind)), meter, node("span", "num", score.toFixed(2)));
      el.detailScores.append(row);
    }
    if (!ranked.length) el.detailScores.append(node("li", "none", "Nothing here looked like the start of a block."));
    el.detailFeatures.textContent = block.features ? JSON.stringify(block.features, null, 2) : "—";
    renderFixes(block);
  }

  // ── Hand corrections ──────────────────────────────────────────────────

  function openersCorrected() {
    const keys = new Set([
      ...Object.keys(overrides.delta), ...Object.keys(overrides.retype),
      ...Object.keys(overrides.end), ...overrides.suppressed.map(String),
    ]);
    return keys.size;
  }

  function renderCorrections() {
    el.corrections.hidden = !analysis;
    const count = openersCorrected();
    el.correctionsCount.textContent = count === 0 ? "No corrections yet"
      : count === 1 ? "1 block corrected" : count + " blocks corrected";
    el.correctionsSave.hidden = el.correctionsClear.hidden = count === 0;
  }

  function corrected(start) {
    return String(start) in overrides.delta || String(start) in overrides.retype ||
      String(start) in overrides.end || overrides.suppressed.includes(start);
  }

  function renderFixes(block) {
    const start = block.span.start;
    const semantic = isSemantic(block);
    const parts = block.span.end - start;
    el.fixKind.replaceChildren();
    if (!semantic) el.fixKind.append(new Option(kindLabel(block.kind), ""));
    for (const kind of SEMANTIC) el.fixKind.append(new Option(titleCase(kind), kind));
    el.fixKind.value = semantic ? block.kind : "";
    el.fixLength.textContent = parts === 1 ? "1 part" : parts + " parts";
    el.fixShorter.disabled = !semantic || block.end === "environment" || block.span.end - start < 2;
    el.fixLonger.disabled = !semantic || block.end === "environment" ||
      block.span.end >= analysis.document.nodes.length;
    el.fixReject.disabled = !semantic;
    el.fixReset.disabled = !corrected(start);
  }

  function clearOpener(start) {
    const key = String(start);
    delete overrides.delta[key];
    delete overrides.retype[key];
    delete overrides.end[key];
    overrides.suppressed = overrides.suppressed.filter((n) => n !== start);
  }

  async function correct(change, message) {
    const block = analysis && analysis.document.blocks.find((b) => b.id === selected);
    if (!block) return;
    change(block, block.span.start);
    selectedStart = block.span.start;
    await pushOverrides(message);
  }

  async function pushOverrides(message) {
    try {
      await invoke("set_overrides", { overrides });
      await refresh(true);
      // The marks and counts follow the corrections even when the HTML did not change.
      renderBlocks();
      if (message) setStatus(message);
    } catch (error) {
      setStatus(String(error), true);
    }
  }

  function retype(kind) {
    return correct((block, start) => {
      if (!kind) return;
      overrides.suppressed = overrides.suppressed.filter((n) => n !== start);
      if (kind === block.kind && !(String(start) in overrides.retype)) return;
      overrides.retype[String(start)] = kind;
    }, "Changed to " + kind);
  }

  function moveEnd(by) {
    return correct((block, start) => {
      const nodes = analysis.document.nodes.length;
      const end = Math.min(nodes, Math.max(start + 1, block.span.end + by));
      delete overrides.delta[String(start)];
      overrides.end[String(start)] = end;
    }, by < 0 ? "Block made shorter" : "Block made longer");
  }

  function reject() {
    return correct((block, start) => {
      clearOpener(start);
      overrides.suppressed.push(start);
    }, "Marked as plain text");
  }

  function resetBlock() {
    return correct((block, start) => clearOpener(start), "Your changes to this block were undone");
  }

  async function saveCorrections() {
    try {
      const dest = await invoke("save_overrides", { source });
      if (dest) setStatus("Corrections saved to " + dest);
    } catch (error) {
      setStatus(String(error), true);
    }
  }

  async function loadCorrections() {
    try {
      const loaded = await invoke("load_overrides");
      if (!loaded) return;
      overrides = Object.assign(emptyOverrides(), loaded);
      await pushOverrides("Corrections loaded");
    } catch (error) {
      setStatus(String(error), true);
    }
  }

  function clearCorrections() {
    overrides = emptyOverrides();
    return pushOverrides("All corrections undone");
  }

  // ── Selection ─────────────────────────────────────────────────────────

  function select(id, fromFrame) {
    selected = id;
    let current = null;
    for (const button of el.blocks.querySelectorAll(".block")) {
      if (button.dataset.id === id) {
        button.setAttribute("aria-current", "true");
        current = button;
      } else button.removeAttribute("aria-current");
    }
    const block = analysis && analysis.document.blocks.find((b) => b.id === id);
    if (block) {
      selectedStart = block.span.start;
      renderDetail(block);
    }
    if (fromFrame) {
      // A plain-text block is listed only with "plain text" on.
      if (current) current.scrollIntoView({ block: "nearest" });
      return;
    }
    // Changing only the fragment scrolls the frame without reloading it.
    el.frame.src = previewUrl(id);
  }

  // A click on a block in the preview selects it here (runtime.js posts the
  // block id when the document runs inside a frame).
  window.addEventListener("message", (event) => {
    if (event.source !== el.frame.contentWindow) return;
    const data = event.data;
    if (data && data.mathia === "print") {
      exportPdf();
      return;
    }
    if (!data || data.mathia !== "select" || typeof data.id !== "string") return;
    if (analysis && analysis.document.blocks.some((b) => b.id === data.id)) select(data.id, true);
  });

  // ── Detection settings dialog ────────────────────────────────────────────────────

  function numberField(container, label, value, attrs) {
    const input = node("input");
    input.type = "number";
    input.required = true;
    Object.assign(input, attrs);
    input.value = String(value);
    input.dataset.key = label;
    const wrap = node("label", "", label + " ");
    wrap.append(input);
    container.append(wrap);
  }

  function fillConfig(config) {
    editing = config;
    el.configError.hidden = true;
    el.cfgScorer.value = config.scorer && config.scorer.model === "tree" ? "tree" : "linear";
    el.cfgThreshold.value = String(config.threshold);
    el.cfgWeights.replaceChildren();
    for (const [key, value] of Object.entries(config.weights)) {
      numberField(el.cfgWeights, key, value, { min: 0, max: 1, step: 0.01 });
    }
    el.cfgScope.replaceChildren();
    for (const [key, value] of Object.entries(config.scope)) {
      numberField(el.cfgScope, key, value, { min: 1, max: 1000, step: 1 });
    }
    el.cfgKeywords.replaceChildren();
    for (const rule of config.keywords) {
      const input = node("input");
      input.type = "text";
      input.dir = "auto";
      input.dataset.kind = rule.kind;
      input.value = rule.words.join(", ");
      const id = "cfg-kw-" + rule.kind;
      input.id = id;
      const label = node("label", "", rule.kind);
      label.htmlFor = id;
      el.cfgKeywords.append(label, input);
    }
    el.cfgCues.replaceChildren();
    for (const key of CUE_LISTS) {
      const area = node("textarea");
      area.dir = "auto";
      area.spellcheck = false;
      area.dataset.key = key;
      area.value = (config[key] || []).join("\n");
      const label = node("label", "", key);
      label.append(area);
      el.cfgCues.append(label);
    }
  }

  function readConfig() {
    const config = JSON.parse(JSON.stringify(editing));
    const scorer = el.cfgScorer.value;
    if (scorer === "linear") config.scorer = { model: "linear" };
    else if (!editing.scorer || editing.scorer.model !== "tree") config.scorer = { model: "tree" };
    config.threshold = Number(el.cfgThreshold.value);
    for (const input of el.cfgWeights.querySelectorAll("input")) {
      config.weights[input.dataset.key] = Number(input.value);
    }
    for (const input of el.cfgScope.querySelectorAll("input")) {
      config.scope[input.dataset.key] = Math.round(Number(input.value));
    }
    config.keywords = Array.from(el.cfgKeywords.querySelectorAll("input")).map((input) => ({
      kind: input.dataset.kind,
      words: input.value.split(/[,،]/).map((w) => w.trim()).filter(Boolean),
    }));
    for (const area of el.cfgCues.querySelectorAll("textarea")) {
      config[area.dataset.key] = area.value.split("\n").map((w) => w.trim()).filter(Boolean);
    }
    return config;
  }

  function configFailed(error) {
    el.configError.textContent = String(error);
    el.configError.hidden = false;
  }

  async function showConfig() {
    try {
      fillConfig(await invoke("get_config"));
      if (!el.configDialog.open) el.configDialog.showModal();
    } catch (error) {
      setStatus(String(error), true);
    }
  }

  async function applyConfig() {
    if (!el.configForm.reportValidity()) return;
    try {
      await invoke("set_config", { config: readConfig() });
      el.configDialog.close();
      await refresh();
    } catch (error) {
      configFailed(error);
    }
  }

  async function configAction(command, done) {
    try {
      const result = await invoke(command);
      if (result) done(result);
    } catch (error) {
      configFailed(error);
    }
  }

  el.scoring.addEventListener("click", showConfig);
  el.configApply.addEventListener("click", applyConfig);
  el.configCancel.addEventListener("click", () => el.configDialog.close());
  el.configReset.addEventListener("click", () => configAction("reset_config", fillConfig));
  el.configLoad.addEventListener("click", () => configAction("load_config", fillConfig));
  el.configSave.addEventListener("click", async () => {
    // Save what is in the form, not only what was applied before.
    if (!el.configForm.reportValidity()) return;
    try {
      await invoke("set_config", { config: readConfig() });
      const dest = await invoke("save_config");
      if (dest) setStatus("Detection settings saved to " + dest);
      refresh(true);
    } catch (error) {
      configFailed(error);
    }
  });

  // ── Decision model ───────────────────────────────────────────────────────────

  const DM_PLACEHOLDERS = {
    "anthropic": { model: "e.g. claude-haiku-5-5", url: "https://api.anthropic.com" },
    "open-ai-compatible": { model: "e.g. gpt-4o-mini or llama3", url: "https://api.openai.com/v1" },
  };
  /** What the backend reports: settings, whether a key is stored. */
  let decision = null;
  /** The stored key is to be deleted when the dialog is saved. */
  let dmClearKey = false;
  let modelRunning = false;

  function updateDecisionRun() {
    el.decisionRun.disabled = !(analysis && decision && decision.ready) || modelRunning;
  }

  function dmMessage(text, isError) {
    el.decisionMessage.textContent = text;
    el.decisionMessage.classList.toggle("ok", !isError);
    el.decisionMessage.hidden = !text;
  }

  function dmPlaceholders() {
    const hints = DM_PLACEHOLDERS[el.dmProvider.value];
    el.dmModel.placeholder = hints.model;
    el.dmUrl.placeholder = hints.url;
  }

  function dmKeyChange() {
    const typed = el.dmKey.value.trim();
    if (typed) return { set: typed };
    return dmClearKey ? "clear" : "keep";
  }

  function dmSettings() {
    return {
      provider: el.dmProvider.value,
      baseUrl: el.dmUrl.value.trim(),
      model: el.dmModel.value.trim(),
    };
  }

  function fillDecision() {
    const d = decision;
    el.dmProvider.value = d.provider;
    el.dmModel.value = d.model;
    el.dmUrl.value = d.baseUrl;
    el.dmKey.value = "";
    dmClearKey = false;
    el.dmKey.placeholder = d.hasKey ? "Saved (" + d.keyHint + ") — type to replace" : "Paste your API key";
    el.dmKeyHint.textContent = d.provider === "open-ai-compatible"
      ? "A local server may not need one." : "";
    el.dmRemoveKey.hidden = !d.hasKey;
    dmPlaceholders();
    dmMessage("");
  }

  async function loadDecision() {
    try {
      decision = await invoke("get_decision_model");
    } catch (error) {
      setStatus(String(error), true);
    }
    updateDecisionRun();
  }

  async function showDecision() {
    await loadDecision();
    if (!decision) return;
    fillDecision();
    if (!el.decisionDialog.open) el.decisionDialog.showModal();
    el.dmModel.focus();
  }

  async function saveDecision() {
    if (!el.dmModel.reportValidity()) return;
    try {
      decision = await invoke("set_decision_model", { settings: dmSettings(), key: dmKeyChange() });
      updateDecisionRun();
      el.decisionDialog.close();
      setStatus(decision.ready ? "Decision model saved" : "Decision model saved, but it still needs an API key");
    } catch (error) {
      dmMessage(String(error), true);
    }
  }

  async function testDecision() {
    if (!el.dmModel.reportValidity()) return;
    el.dmTest.disabled = true;
    dmMessage("Connecting…");
    try {
      dmMessage(await invoke("test_decision_model", { settings: dmSettings(), key: dmKeyChange() }));
    } catch (error) {
      dmMessage(String(error), true);
    } finally {
      el.dmTest.disabled = false;
    }
  }

  async function runDecision() {
    if (!source || modelRunning) return;
    modelRunning = true;
    updateDecisionRun();
    setStatus("Asking the decision model…");
    try {
      const run = await invoke("run_decision_model", { source });
      overrides = Object.assign(emptyOverrides(), run.overrides);
      await pushOverrides();
      setStatus(
        "Decision model reviewed " + run.reviewed + " blocks and changed " + run.changed +
          (run.asked === 0 ? " (answers reused)" : "")
      );
    } catch (error) {
      setStatus(String(error), true);
    } finally {
      modelRunning = false;
      updateDecisionRun();
    }
  }

  el.decision.addEventListener("click", showDecision);
  el.decisionRun.addEventListener("click", runDecision);
  el.dmProvider.addEventListener("change", dmPlaceholders);
  el.dmTest.addEventListener("click", testDecision);
  el.dmSave.addEventListener("click", saveDecision);
  el.dmCancel.addEventListener("click", () => el.decisionDialog.close());
  el.dmRemoveKey.addEventListener("click", () => {
    dmClearKey = true;
    el.dmKey.value = "";
    el.dmKey.placeholder = "Key will be removed when you save";
    el.dmRemoveKey.hidden = true;
  });
  loadDecision();

  async function exportFolder() {
    try {
      setStatus("Choose a folder of Markdown files…");
      const result = await invoke("export_folder", { options: options() });
      if (result) setStatus("Exported " + result[0] + " file(s) to " + result[1]);
      else setStatus("");
    } catch (error) {
      setStatus(String(error), true);
    }
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
    setStatus("Prompt copied. Paste it into your vision model, then attach the pages.");
    const label = button.dataset.label || button.textContent;
    button.dataset.label = label;
    button.textContent = "Copied";
    clearTimeout(button.copyTimer);
    button.copyTimer = setTimeout(() => { button.textContent = label; }, 1600);
  }

  function setPreviewOpen(open) {
    previewOpen = Boolean(open);
    layout();
    el.previewToggle.setAttribute("aria-expanded", String(previewOpen));
    el.previewToggle.title = previewOpen ? "Hide the preview" : "Show the preview";
  }

  // ── Menus ─────────────────────────────────────────────────────────────

  const menus = [
    [el.optionsButton, el.optionsMenu], [el.exportMore, el.exportMenu],
    [el.recentButton, el.recentMenu],
  ];

  function closeMenus(except) {
    for (const [button, panel] of menus) {
      if (panel === except) continue;
      panel.hidden = true;
      button.setAttribute("aria-expanded", "false");
    }
  }

  for (const [button, panel] of menus) {
    button.addEventListener("click", () => {
      const opening = panel.hidden;
      closeMenus();
      panel.hidden = !opening;
      button.setAttribute("aria-expanded", String(opening));
    });
    panel.addEventListener("click", (event) => {
      if (event.target.closest(".menu-item")) closeMenus();
    });
  }
  document.addEventListener("click", (event) => {
    if (!event.target.closest(".menu-wrap")) closeMenus();
  });
  // A click in the preview frame never reaches this document.
  window.addEventListener("blur", () => closeMenus());

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
  el.openWelcome.addEventListener("click", open);
  el.detailClose.addEventListener("click", deselect);
  el.filter.addEventListener("input", renderBlocks);
  el.views.addEventListener("click", (event) => {
    const button = event.target.closest("button[data-view]");
    if (!button) return;
    view = button.dataset.view;
    renderBlocks();
  });
  // Up and down walk the list, so a document can be reviewed from the keyboard.
  el.blocks.addEventListener("keydown", (event) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    const buttons = Array.from(el.blocks.querySelectorAll(".block"));
    const at = buttons.indexOf(document.activeElement);
    const next = buttons[at + (event.key === "ArrowDown" ? 1 : -1)];
    if (!next) return;
    event.preventDefault();
    next.focus();
    select(next.dataset.id);
  });
  el.reload.addEventListener("click", () => refresh());
  el.export.addEventListener("click", exportHtml);
  el.previewToggle.addEventListener("click", () => setPreviewOpen(!previewOpen));
  el.ai.addEventListener("change", () => refresh());
  el.math.addEventListener("change", () => refresh());
  el.showText.addEventListener("change", renderBlocks);
  el.blocks.addEventListener("click", (event) => {
    const button = event.target.closest(".block");
    if (button) select(button.dataset.id);
  });
  el.fixKind.addEventListener("change", () => retype(el.fixKind.value));
  el.fixShorter.addEventListener("click", () => moveEnd(-1));
  el.fixLonger.addEventListener("click", () => moveEnd(1));
  el.fixReject.addEventListener("click", reject);
  el.fixReset.addEventListener("click", resetBlock);
  el.correctionsSave.addEventListener("click", saveCorrections);
  el.correctionsLoad.addEventListener("click", loadCorrections);
  el.correctionsClear.addEventListener("click", clearCorrections);
  el.exportFolder.addEventListener("click", exportFolder);
  el.exportPdf.addEventListener("click", exportPdf);

  window.addEventListener("keydown", (event) => {
    if (el.guideDialog.open || el.configDialog.open) return;
    if (event.key === "Escape") closeMenus();
    if (!(event.metaKey || event.ctrlKey) || event.altKey) return;
    const key = event.key.toLowerCase();
    if (key === "o") open();
    else if (key === "f" && analysis) el.filter.focus();
    else if (key === "e") exportHtml();
    else if (key === "p") exportPdf();
    else if (key === "r") refresh();
    else return;
    event.preventDefault();
  });

  // Pick up edits made in another program when the window comes back, and
  // while it is visible, whenever the file changes on disk.
  window.addEventListener("focus", () => refresh(true));
  setInterval(async () => {
    if (!source || busy || document.hidden) return;
    try {
      const now = await invoke("source_stamp", { source });
      if (stamp !== null && now !== stamp) await refresh(true);
      stamp = now;
    } catch (_) {
      // The file may be mid-save or gone; the next compile reports it.
    }
  }, WATCH_INTERVAL);

  el.recentClear.addEventListener("click", async () => {
    try {
      await invoke("clear_recent");
    } catch (error) {
      setStatus(String(error), true);
    }
    loadRecent();
  });

  layout();
  loadRecent();
  invoke("startup_file")
    .then((path) => { if (path) openPath(path); })
    .catch((error) => setStatus(String(error), true))
    .finally(loadRecent);

  listen("tauri://drag-enter", () => { el.drop.hidden = false; });
  listen("tauri://drag-leave", () => { el.drop.hidden = true; });
  listen("tauri://drag-drop", (event) => {
    el.drop.hidden = true;
    const path = (event.payload.paths || []).find((p) => MARKDOWN.test(p));
    if (path) openPath(path).then(loadRecent);
    else setStatus("Drop a Markdown file (.md, .markdown, .txt)", true);
  });
})();
