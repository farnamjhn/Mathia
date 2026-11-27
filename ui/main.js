// Mathia desktop frontend. No framework and no build step: this file is loaded
// as-is and talks to the Rust side through the global Tauri API
// (`app.withGlobalTauri`). All text is set with `textContent`; nothing here
// parses HTML. The compiled document lives in the sandboxed preview frame.
(function () {
  "use strict";

  const isMac = !window.location.search.includes("platform=windows") && (
    (typeof navigator !== "undefined" && (
      (navigator.userAgent && navigator.userAgent.includes("Mac")) ||
      (navigator.platform && navigator.platform.startsWith("Mac"))
    ))
  );
  if (isMac) {
    document.body.classList.add("platform-mac");
    document.body.classList.remove("platform-windows");
  } else {
    document.body.classList.add("platform-windows");
    document.body.classList.remove("platform-mac");
  }

  const { invoke, convertFileSrc } = window.__TAURI__.core;
  const { listen } = window.__TAURI__.event;

  const PREVIEW_SCHEME = "mathia-preview";
  const SUPPORTED_INPUT = /\.(md|markdown|mdown|txt|html|htm)$/i;
  const HTML_EXT = /\.(html|htm)$/i;
  const AI_PROVIDERS = ["anthropic", "openai", "gemini", "openai-compatible"];

  const $ = (id) => document.getElementById(id);
  const el = {
    home: $("home"), open: $("open"), reload: $("reload"), export: $("export"),
    undo: $("undo"), redo: $("redo"),
    ai: $("ai"), math: $("render-math"), showText: $("show-text"),
    optCrfModel: $("opt-crf-model"), winMenuModel: $("win-menu-model"),
    status: $("status"), blocks: $("blocks"), panes: $("panes"),
    preview: $("preview"), previewToggle: $("preview-toggle"),
    frame: $("frame"), empty: $("empty"), inspector: $("inspector"),
    docName: $("doc-name"), openWelcome: $("open-welcome"),
    openMulti: $("open-multi"), openMultiWelcome: $("open-multi-welcome"),
    orderDialog: $("order-dialog"), orderTitle: $("order-title"),
    orderAdd: $("order-add"), orderClear: $("order-clear"),
    orderCount: $("order-count"), orderList: $("order-list"),
    orderEmpty: $("order-empty"), orderTitleInput: $("order-title-input"),
    orderCancel: $("order-cancel"), orderCompile: $("order-compile"),
    filter: $("filter"), views: $("views"), blocksEmpty: $("blocks-empty"),
    typeFilter: $("block-type"),
    countAll: $("count-all"), countUnsure: $("count-unsure"), countChanged: $("count-changed"),
    optionsButton: $("options-button"), optionsMenu: $("options-menu"),
    exportMore: $("export-more"), exportMenu: $("export-menu"),
    detailClose: $("detail-close"), detailNote: $("detail-note"), fixLength: $("fix-length"),
    drop: $("drop"), detail: $("detail"), detailTitle: $("detail-title"),
    detailFacts: $("detail-facts"), detailScores: $("detail-scores"),
    detailFeatures: $("detail-features"),
    guide: $("guide"), guideDialog: $("guide-dialog"), guideClose: $("guide-close"),
    guideDesc: $("guide-desc"), promptTabInitial: $("prompt-tab-initial"),
    promptTabContinuation: $("prompt-tab-continuation"),
    copyContinuationPrompt: $("copy-continuation-prompt"),
    orderPromptBtn: $("order-prompt-btn"),
    viewPrompt: $("view-prompt"), promptText: $("prompt-text"),
    promptContinuation: $("prompt-continuation"),
    copyPrompt: $("copy-prompt"), copyPromptEmpty: $("copy-prompt-empty"),
    exportFolder: $("export-folder"),
    exportMarkdown: $("export-markdown"),
    fixMarkdown: $("fix-markdown"),
    exportPdf: $("export-pdf"),
    recent: $("recent"), recentList: $("recent-list"), recentClear: $("recent-clear"),
    recentWrap: $("recent-wrap"), recentButton: $("recent-button"), recentMenu: $("recent-menu"),
    winMenubar: $("win-menubar"),
    aboutDialog: $("about-dialog"), aboutClose: $("about-close"),
    aboutVersion: $("about-version"),
    updateDialog: $("update-dialog"), updateTitle: $("update-title"),
    updateSummary: $("update-summary"), updateNotes: $("update-notes"),
    updateProgressWrap: $("update-progress-wrap"),
    updateProgressTrack: $("update-progress-track"),
    updateProgressFill: $("update-progress-fill"),
    updateProgressText: $("update-progress-text"),
    updateError: $("update-error"),
    updateLater: $("update-later"), updateInstall: $("update-install"),
    winRecentMenu: $("win-recent-menu"), winRecentEmpty: $("win-recent-empty"),
    winRecentDivider: $("win-recent-divider"), winRecentClear: $("win-recent-clear"),
    corrections: $("corrections"), correctionsCount: $("corrections-count"),
    correctionsLoad: $("corrections-load"), correctionsSave: $("corrections-save"),
    correctionsClear: $("corrections-clear"),
    fixKind: $("fix-kind"), fixShorter: $("fix-shorter"), fixLonger: $("fix-longer"),
    fixReject: $("fix-reject"), fixReset: $("fix-reset"),
    scoring: $("scoring"), configDialog: $("config-dialog"), configForm: $("config-form"),
    cfgTagger: $("cfg-tagger"), cfgThreshold: $("cfg-threshold"), cfgWeights: $("cfg-weights"),
    cfgScope: $("cfg-scope"), cfgKeywords: $("cfg-keywords"), cfgCues: $("cfg-cues"),
    configError: $("config-error"), configLoad: $("config-load"), configSave: $("config-save"),
    decision: $("decision"), decisionRun: $("decision-run"), decisionDialog: $("decision-dialog"),
    dmProvider: $("dm-provider"), dmModel: $("dm-model"), dmUrl: $("dm-url"), dmKey: $("dm-key"),
    dmKeyHint: $("dm-key-hint"), dmRemoveKey: $("dm-remove-key"), dmTest: $("dm-test"),
    dmCancel: $("dm-cancel"), dmSave: $("dm-save"), decisionMessage: $("decision-message"),
    configReset: $("config-reset"), configCancel: $("config-cancel"), configApply: $("config-apply"),
    loadingScreen: $("loading-screen"), loadingTitle: $("loading-title"), loadingDesc: $("loading-desc"),
    loadingBar: $("loading-bar"), loadingFill: $("loading-fill"),
    loadingStage: $("loading-stage"), loadingPercent: $("loading-percent"),
  };

  const SEMANTIC = [
    "theorem", "lemma", "proposition", "corollary", "definition", "proof", "example",
    "remark", "exercise", "solution", "claim", "notation",
  ];
  const CUE_LISTS = [
    "logic_symbols", "qed_markers", "definitional_cues", "proof_cues",
    "statement_cues", "continuation_cues", "reference_cues",
  ];
  /** Last config fetched from the backend, used by the model toggle. */
  let currentConfig = null;
  /** How each block type is written in the list, as a textbook margin would. */
  const SHORT = {
    theorem: "Thm", lemma: "Lem", proposition: "Prop", corollary: "Cor", definition: "Def",
    proof: "Proof", example: "Ex", remark: "Rem", exercise: "Exer", solution: "Sol",
    claim: "Claim", notation: "Not", heading: "§", text: "¶",
  };
  /** The order the Type menu lists block types in. */
  const SEMANTIC_ORDER = [
    "theorem", "lemma", "proposition", "corollary", "definition", "proof",
    "example", "remark", "exercise", "solution", "claim", "notation",
  ];
  /** How often the open file is checked for changes made elsewhere (ms). */
  const WATCH_INTERVAL = 1000;

  /** @type {{path: string} | {multi_path: string[]} | null} */
  let source = null;
  let customTitle = null;
  let orderPaths = [];
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
  /** Every change that can be undone, oldest first: corrections before and
   *  after, and highlights the frame can replay. */
  let undoStack = [];
  let redoStack = [];
  /** The configuration shown in the detection settings dialog. */
  let editing = null;
  /** Which blocks the list shows: "all", "unsure" or "changed". */
  let view = "all";
  /** Which block type the list shows: "all", or a block kind such as "theorem". */
  let typeFilter = "all";
  /** The Type menu built last, so it is only rebuilt when the counts change. */
  let typeFilterKey = "";
  /** Whether the user wants the preview pane (it only shows with a document). */
  let previewOpen = true;
  let initialPromptText = el.promptText ? el.promptText.value : "";
  let continuationPromptText = el.promptContinuation ? el.promptContinuation.value : "";
  let promptMode = "initial";

  function emptyOverrides() {
    return { delta: {}, suppressed: [], retype: {}, end: {} };
  }

  function cloneOverrides(value) {
    return JSON.parse(JSON.stringify(value));
  }

  function sameOverrides(a, b) {
    return JSON.stringify(a) === JSON.stringify(b);
  }

  function updateHistory() {
    if (el.undo && el.redo) {
      el.undo.disabled = undoStack.length === 0;
      el.redo.disabled = redoStack.length === 0;
    }
    const winUndo = $("win-menu-undo");
    if (winUndo) winUndo.disabled = undoStack.length === 0;
    const winRedo = $("win-menu-redo");
    if (winRedo) winRedo.disabled = redoStack.length === 0;
  }

  function pushHistory(entry) {
    undoStack.push(entry);
    if (undoStack.length > 100) undoStack.shift();
    redoStack = [];
    updateHistory();
  }

  /** A recompile reloads the frame, which drops its highlights: their history
   *  entries would no longer mean anything. */
  function forgetHighlights() {
    undoStack = undoStack.filter((entry) => entry.kind !== "highlight");
    redoStack = redoStack.filter((entry) => entry.kind !== "highlight");
    updateHistory();
  }

  function resetHistory() {
    undoStack = [];
    redoStack = [];
    updateHistory();
  }

  function postToFrame(kind) {
    const frame = el.frame.contentWindow;
    if (frame) frame.postMessage({ mathia: kind }, "*");
  }

  async function replayHistory(entry, backwards) {
    if (entry.kind === "highlight") {
      postToFrame(backwards ? "highlight-undo" : "highlight-redo");
      return;
    }
    overrides = cloneOverrides(backwards ? entry.before : entry.after);
    try {
      await invoke("set_overrides", { overrides });
      await refresh(true);
      renderBlocks();
      setStatus(backwards ? "Undone" : "Redone");
    } catch (error) {
      setStatus(String(error), true);
    }
  }

  async function undoChange() {
    const entry = undoStack.pop();
    if (!entry) return;
    redoStack.push(entry);
    updateHistory();
    await replayHistory(entry, true);
  }

  async function redoChange() {
    const entry = redoStack.pop();
    if (!entry) return;
    undoStack.push(entry);
    updateHistory();
    await replayHistory(entry, false);
  }

  function setStatus(text, isError) {
    el.status.textContent = text;
    el.status.classList.toggle("error", Boolean(isError));
  }

  function options() {
    return {
      title: customTitle || undefined,
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

  function updateMenuToggles() {
    const isCrf = !!currentConfig && currentConfig.tagger === "crf";
    if (el.optCrfModel) el.optCrfModel.checked = isCrf;
    if (el.winMenuModel) {
      const check = el.winMenuModel.querySelector(".menu-check");
      if (check) check.style.visibility = isCrf ? "visible" : "hidden";
    }
    if (el.cfgTagger && !(el.configDialog && el.configDialog.open)) {
      el.cfgTagger.value = isCrf ? "crf" : "weighted_sum";
    }
    const mathBtn = $("win-menu-render-math");
    if (mathBtn) {
      const check = mathBtn.querySelector(".menu-check");
      if (check) check.style.visibility = el.math.checked ? "visible" : "hidden";
    }
    const aiBtn = $("win-menu-ai");
    if (aiBtn) {
      const check = aiBtn.querySelector(".menu-check");
      if (check) check.style.visibility = el.ai.checked ? "visible" : "hidden";
    }
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
    updateHistory();
    updateMenuToggles();
    el.reload.disabled = el.export.disabled = el.exportPdf.disabled = !has;
    el.exportMarkdown.disabled = !has;
    el.home.disabled = !has;
    const isHtml = Boolean(
      (source && source.path && HTML_EXT.test(source.path)) ||
      (source && source.multi_path && source.multi_path.some((p) => HTML_EXT.test(p)))
    );
    el.fixMarkdown.disabled = !has || isHtml;
    el.fixMarkdown.title = isHtml ? "Fix Markdown is for Markdown files." : "";

    const winHome = $("win-menu-home");
    if (winHome) winHome.disabled = !has;
    const winReload = $("win-menu-reload");
    if (winReload) winReload.disabled = !has;
    const winExportHtml = $("win-menu-export-html");
    if (winExportHtml) winExportHtml.disabled = !has;
    const winExportMd = $("win-menu-export-markdown");
    if (winExportMd) winExportMd.disabled = !has;
    const winExportPdf = $("win-menu-export-pdf");
    if (winExportPdf) winExportPdf.disabled = !has;
    const winFixMd = $("win-menu-fix-markdown");
    if (winFixMd) {
      winFixMd.disabled = !has || isHtml;
      winFixMd.title = isHtml ? "Fix Markdown is for Markdown files." : "";
    }
    const winPreview = $("win-menu-preview");
    if (winPreview) winPreview.disabled = !has;
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

  let loadingStartTime = 0;
  let loadingVisible = false;
  let currentProgress = 0;

  function setProgress(percent, stageText) {
    currentProgress = Math.max(0, Math.min(100, Math.round(percent)));
    if (el.loadingBar) {
      el.loadingBar.setAttribute("aria-valuenow", String(currentProgress));
    }
    if (el.loadingFill) {
      el.loadingFill.style.width = currentProgress + "%";
    }
    if (el.loadingPercent) {
      el.loadingPercent.textContent = currentProgress + "%";
    }
    if (stageText && el.loadingStage) {
      el.loadingStage.textContent = stageText;
    }
  }

  function showLoading(title, desc) {
    if (!el.loadingScreen) return;
    loadingStartTime = performance.now();
    loadingVisible = true;
    if (title && el.loadingTitle) el.loadingTitle.textContent = title;
    if (desc && el.loadingDesc) el.loadingDesc.textContent = desc;
    setProgress(5, "Preparing compilation…");
    el.loadingScreen.classList.remove("loading-fade-out");
    el.loadingScreen.hidden = false;
  }

  async function finishLoading() {
    if (!loadingVisible || !el.loadingScreen) return;
    setProgress(100, "Complete");
    const elapsed = performance.now() - loadingStartTime;
    const minDisplay = 260;
    const remaining = Math.max(0, minDisplay - elapsed);
    if (remaining > 0) {
      await new Promise((resolve) => setTimeout(resolve, remaining));
    }
    el.loadingScreen.classList.add("loading-fade-out");
    await new Promise((resolve) => setTimeout(resolve, 220));
    el.loadingScreen.hidden = true;
    el.loadingScreen.classList.remove("loading-fade-out");
    loadingVisible = false;
  }

  function hideLoading() {
    if (!el.loadingScreen) return;
    el.loadingScreen.hidden = true;
    el.loadingScreen.classList.remove("loading-fade-out");
    loadingVisible = false;
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
    if (!quiet) {
      setStatus("Compiling…");
      const isHtml = Boolean(
        (source.path && HTML_EXT.test(source.path)) ||
        (source.multi_path && source.multi_path.some((p) => HTML_EXT.test(p)))
      );
      const title = "Compiling HTML…";
      let desc = "Parsing markdown and rendering mathematics";
      if (source.multi_path && source.multi_path.length > 1) {
        desc = "Combining " + source.multi_path.length + " files into a standalone HTML document";
      } else if (isHtml) {
        desc = "Extracting structure, formulas and cleaning bloat";
      }
      showLoading(title, desc);
    }
    try {
      const compiled = await invoke("compile", { source, options: options() });
      if (compiled.changed || compiled.revision !== revision || !analysis) {
        revision = compiled.revision;
        if (!quiet) setProgress(88, "Analyzing blocks…");
        analysis = await invoke("analyze", { source });
        if (!quiet) setProgress(94, "Rendering preview…");
        renderBlocks();

        let frameLoadedResolve;
        const frameLoadedPromise = new Promise((resolve) => {
          frameLoadedResolve = resolve;
        });
        const onFrameLoad = () => {
          el.frame.removeEventListener("load", onFrameLoad);
          frameLoadedResolve();
        };
        el.frame.addEventListener("load", onFrameLoad);
        const timeoutId = setTimeout(onFrameLoad, 700);

        el.frame.src = previewUrl(selected);

        if (!quiet) {
          await frameLoadedPromise;
          clearTimeout(timeoutId);
        }
        forgetHighlights();
      }
      layout();
      let name = "Mathia";
      let tooltip = "";
      if (source.multi_path) {
        const count = source.multi_path.length;
        name = customTitle || (count === 1 ? fileName(source.multi_path[0]) : count + " files combined");
        tooltip = source.multi_path.map(fileName).join(", ");
      } else if (source.path) {
        name = customTitle || fileName(source.path);
        tooltip = source.path;
      }
      document.title = name + " — Mathia";
      el.docName.textContent = name;
      el.docName.title = tooltip;
      const found = compiled.semantic_blocks;
      const blocksText = found === 1 ? "1 block found" : found + " blocks found";
      const sizeText = Math.round(compiled.bytes / 1024) + " kB of HTML";
      if (source.multi_path && source.multi_path.length > 1) {
        setStatus(blocksText + " across " + source.multi_path.length + " files, " + sizeText);
      } else {
        setStatus(blocksText + ", " + sizeText);
      }
    } catch (error) {
      layout();
      setStatus(String(error), true);
    } finally {
      if (!quiet) {
        await finishLoading();
      } else {
        hideLoading();
      }
      busy = false;
      if (again) {
        again = false;
        refresh(true);
      }
    }
  }

  async function goHome() {
    source = null;
    customTitle = null;
    selected = null;
    selectedStart = null;
    analysis = null;
    stamp = null;
    overrides = emptyOverrides();
    resetHistory();
    view = "all";
    typeFilter = "all";
    typeFilterKey = "";
    if (el.filter) el.filter.value = "";
    if (el.frame) el.frame.src = "about:blank";
    el.blocks.innerHTML = "";
    document.title = "Mathia";
    el.docName.textContent = "Mathia";
    el.docName.title = "";
    setStatus("");
    try {
      await invoke("set_overrides", { overrides });
    } catch (_) {}
    layout();
    loadRecent();
  }

  async function openPath(path) {
    source = { path };
    customTitle = null;
    selected = null;
    selectedStart = null;
    analysis = null;
    stamp = null;
    overrides = emptyOverrides();
    resetHistory();
    view = "all";
    typeFilter = "all";
    typeFilterKey = "";
    el.filter.value = "";
    try {
      await invoke("set_overrides", { overrides });
    } catch (error) {
      setStatus(String(error), true);
      return;
    }
    await refresh();
  }

  async function openMultiPath(paths, title) {
    if (!paths || paths.length === 0) return;
    try {
      await invoke("grant_paths", { paths });
    } catch (_) {}
    source = paths.length === 1 && !title ? { path: paths[0] } : { multi_path: paths };
    customTitle = title || null;
    selected = null;
    selectedStart = null;
    analysis = null;
    stamp = null;
    overrides = emptyOverrides();
    resetHistory();
    view = "all";
    typeFilter = "all";
    typeFilterKey = "";
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

    if (el.winRecentMenu) {
      const existingItems = el.winRecentMenu.querySelectorAll(".win-recent-item");
      existingItems.forEach((it) => it.remove());

      if (paths.length === 0) {
        if (el.winRecentEmpty) el.winRecentEmpty.hidden = false;
        if (el.winRecentDivider) el.winRecentDivider.hidden = true;
        if (el.winRecentClear) el.winRecentClear.hidden = true;
      } else {
        if (el.winRecentEmpty) el.winRecentEmpty.hidden = true;
        if (el.winRecentDivider) el.winRecentDivider.hidden = false;
        if (el.winRecentClear) el.winRecentClear.hidden = false;

        for (const path of paths) {
          const btn = document.createElement("button");
          btn.type = "button";
          btn.className = "menu-action win-recent-item";
          btn.dataset.path = path;
          btn.title = path;
          const nameSpan = document.createElement("span");
          nameSpan.className = "recent-name";
          nameSpan.textContent = fileName(path);
          const dirSpan = document.createElement("span");
          dirSpan.className = "recent-dir";
          dirSpan.textContent = parentDir(path);
          btn.appendChild(nameSpan);
          btn.appendChild(dirSpan);
          el.winRecentMenu.insertBefore(btn, el.winRecentDivider);
        }
      }
    }
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
      const paths = await invoke("pick_markdown");
      if (!paths || paths.length === 0) return;
      if (paths.length === 1) {
        await openPath(paths[0]);
      } else {
        openOrderDialog(paths);
      }
    } catch (error) {
      setStatus(String(error), true);
    }
    loadRecent();
  }

  async function combineMarkdowns() {
    try {
      const paths = await invoke("pick_markdown");
      if (paths && paths.length > 0) {
        openOrderDialog(paths);
      }
    } catch (error) {
      setStatus(String(error), true);
    }
  }

  async function exportHtml() {
    if (!source) return;
    showLoading("Compiling HTML…", "Preparing standalone HTML file for export");
    try {
      const dest = await invoke("export", { source, options: options() });
      if (dest) setStatus("Exported to " + dest);
    } catch (error) {
      setStatus(String(error), true);
    } finally {
      await finishLoading();
    }
  }

  // The preview is always compiled from the repaired text; this writes that
  // repair back to a Markdown file and switches to it.
  async function fixMarkdown() {
    if (!source) return;
    try {
      const fixed = await invoke("fix_markdown", { source });
      if (!fixed.changed) {
        setStatus("Nothing to fix: the Markdown is already well formed.");
        return;
      }
      if (!fixed.path) return;
      const wrapped = fixed.formulas === 1 ? "1 formula" : fixed.formulas + " formulas";
      const saved = "Saved the repaired Markdown to " + fixed.path +
        (fixed.formulas ? " (" + wrapped + " given their $ $)" : "");
      // Same blocks in the same order, so the corrections stay valid.
      if (!source.path || fixed.path !== source.path) {
        source = { path: fixed.path };
        customTitle = null;
        stamp = null;
        loadRecent();
      }
      await refresh(true);
      setStatus(saved);
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

  // The compiled document as Markdown: the same text, with every block
  // labelled the way the inspector shows it.
  async function exportMarkdown() {
    if (!source) return;
    try {
      const dest = await invoke("export_markdown", { source });
      if (dest) setStatus("Exported Markdown to " + dest);
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
      block.score < analysis.confidence_threshold;
  }

  function matches(block, query) {
    if (!query) return true;
    const text = (block.title || "") + " " + block.kind + " " + block.id;
    return text.toLowerCase().includes(query);
  }

  function listed(block, query) {
    if (!matches(block, query)) return false;
    if (typeFilter !== "all" && block.kind !== typeFilter) return false;
    if (view === "unsure") return unsure(block);
    if (view === "changed") return corrected(block.span.start);
    if (block.kind === "heading") return !query;
    // A paragraph the user demoted stays listed, so the change can be undone.
    if (block.kind === "text") return el.showText.checked || corrected(block.span.start);
    return true;
  }

  // The Type menu lists the block types this document has, with their counts,
  // so "every theorem" is one click away. It is rebuilt only when the counts
  // change, so an open menu is not closed by a keystroke in the filter.
  function renderTypeFilter(blocks) {
    const counts = new Map();
    let total = 0;
    for (const block of blocks) {
      if (!isSemantic(block)) continue;
      counts.set(block.kind, (counts.get(block.kind) || 0) + 1);
      total += 1;
    }
    const kinds = SEMANTIC_ORDER.filter((kind) => counts.has(kind));
    for (const kind of counts.keys()) {
      if (!kinds.includes(kind)) kinds.push(kind);
    }
    // A type that is gone (retyped, merged away) falls back to every type.
    if (typeFilter !== "all" && !counts.has(typeFilter)) typeFilter = "all";
    const key = kinds.map((kind) => kind + ":" + counts.get(kind)).join(",");
    if (key !== typeFilterKey) {
      typeFilterKey = key;
      const options = [new Option("All types (" + total + ")", "all")];
      for (const kind of kinds) {
        options.push(new Option(kindLabel(kind) + " (" + counts.get(kind) + ")", kind));
      }
      el.typeFilter.replaceChildren(...options);
    }
    el.typeFilter.value = typeFilter;
  }

  function renderBlocks() {
    el.blocks.replaceChildren();
    if (!analysis) return;
    const blocks = analysis.document.blocks;
    renderTypeFilter(blocks);
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
      : typeFilter !== "all" ? "No " + kindLabel(typeFilter).toLowerCase() + " blocks here."
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
    fact("Confidence", block.score > 0 ? block.score.toFixed(2) : null);
    fact("Number", block.number);
    fact("Name", block.name);
    fact("Proves", block.proves);
    fact("Ended by", block.end ? block.end.replace(/_/g, " ") : null);
    fact("Parts", block.span.start + "–" + (block.span.end - 1));
    fact("Direction", block.direction);
    fact("Id", block.id);

    el.detailScores.replaceChildren();
    const opener = analysis.tags[block.span.start];
    if (opener && opener.label !== "O") {
      const row = node("li");
      const meter = node("meter");
      const scale = analysis.confidence_threshold > 0 ? analysis.confidence_threshold : 1;
      meter.min = 0;
      meter.max = scale;
      meter.value = Math.min(opener.confidence, scale);
      row.append(
        node("span", "", opener.label),
        meter,
        node("span", "num", opener.confidence.toFixed(2)),
      );
      el.detailScores.append(row);
    } else {
      el.detailScores.append(node("li", "none", "Nothing here looked like the start of a block."));
    }
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
      block.span.end >= scopeUnits();
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
    const before = cloneOverrides(overrides);
    change(block, block.span.start);
    if (sameOverrides(before, overrides)) return;
    selectedStart = block.span.start;
    pushHistory({ kind: "overrides", before, after: cloneOverrides(overrides) });
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
      const end = Math.min(scopeUnits(), Math.max(start + 1, block.span.end + by));
      delete overrides.delta[String(start)];
      overrides.end[String(start)] = end;
    }, by < 0 ? "Block made shorter" : "Block made longer");
  }

  /** How far a block's scope may reach: source nodes for Markdown, the
   *  highest block span for HTML, which has no source nodes. */
  function scopeUnits() {
    const doc = analysis && analysis.document;
    if (!doc) return 0;
    const nodes = doc.nodes ? doc.nodes.length : 0;
    const spans = doc.blocks.reduce((max, b) => Math.max(max, b.span.end), 0);
    return Math.max(nodes, spans);
  }

  /** The source node whose prose matches the text the drag ended in. The
   *  runtime sends that prose and the block it sits in, which narrows the
   *  search to one block's nodes. */
  function nodeIndexFor(text, from) {
    const nodes = analysis && analysis.document.nodes;
    const target = String(text || "").replace(/\s+/g, " ").trim();
    if (!nodes || !target) return -1;
    let loose = -1;
    for (let i = Math.max(0, from); i < nodes.length; i += 1) {
      const candidate = nodes[i].text.replace(/\s+/g, " ").trim();
      if (!candidate) continue;
      if (candidate === target) return i;
      if (loose < 0 && (candidate.startsWith(target) || target.startsWith(candidate))) loose = i;
    }
    return loose;
  }

  /** Grows the last block of a drag over the text it covers. */
  async function expandBlock(id, endId, text) {
    const block = analysis && analysis.document.blocks.find((b) => b.id === id);
    if (!block) return;
    if (block.end === "environment") {
      setStatus("A LaTeX environment cannot be expanded.", true);
      return;
    }
    const owner = analysis.document.blocks.find((b) => b.id === endId);
    let index = nodeIndexFor(text, owner ? owner.span.start : block.span.start);
    if (index < 0) index = nodeIndexFor(text, block.span.start);
    // A node of nothing but a formula has no prose to match; its block still
    // tells us where the drag ended.
    if (index < 0 && owner) index = owner.span.end - 1;
    if (index < 0) {
      setStatus("Could not find where the selection ends.", true);
      return;
    }
    if (index + 1 <= block.span.end) {
      setStatus("The block already covers this text.");
      return;
    }
    const before = cloneOverrides(overrides);
    delete overrides.delta[String(block.span.start)];
    overrides.end[String(block.span.start)] = index + 1;
    pushHistory({ kind: "overrides", before, after: cloneOverrides(overrides) });
    selected = block.id;
    selectedStart = block.span.start;
    await pushOverrides("Block expanded");
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
      const before = cloneOverrides(overrides);
      overrides = Object.assign(emptyOverrides(), loaded);
      if (!sameOverrides(before, overrides)) {
        pushHistory({ kind: "overrides", before, after: cloneOverrides(overrides) });
      }
      await pushOverrides("Corrections loaded");
    } catch (error) {
      setStatus(String(error), true);
    }
  }

  function clearCorrections() {
    const before = cloneOverrides(overrides);
    overrides = emptyOverrides();
    if (sameOverrides(before, overrides)) return Promise.resolve();
    pushHistory({ kind: "overrides", before, after: cloneOverrides(overrides) });
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
    if (data && data.mathia === "expand" && typeof data.id === "string") {
      expandBlock(data.id, data.endId, data.text);
      return;
    }
    if (data && data.mathia === "highlight-applied") {
      pushHistory({ kind: "highlight" });
      return;
    }
    if (data && data.mathia === "undo") {
      undoChange();
      return;
    }
    if (data && data.mathia === "redo") {
      redoChange();
      return;
    }
    if (data && data.mathia === "mindmap-export" && ["svg", "png"].includes(data.extension)
        && typeof data.data === "string"
        && data.data.startsWith(data.extension === "svg" ? "data:image/svg+xml;base64," : "data:image/png;base64,")) {
      if (data.data.length > 64 * 1024 * 1024) {
        setStatus("Mind map export is too large.", true);
        return;
      }
      try {
        const bytes = Array.from(atob(data.data.slice(data.data.indexOf(",") + 1)), (c) => c.charCodeAt(0));
        invoke("export_mindmap", { extension: data.extension, bytes })
          .then((dest) => { if (dest) setStatus("Exported mind map to " + dest); })
          .catch((error) => setStatus(String(error), true));
      } catch (error) {
        setStatus(String(error), true);
      }
      return;
    }
    if (!data || data.mathia !== "select" || typeof data.id !== "string") return;
    if (analysis && analysis.document.blocks.some((b) => b.id === data.id)) select(data.id, true);
  });

  // ── Detection settings dialog ────────────────────────────────────────────────────

  async function loadConfig() {
    try {
      currentConfig = await invoke("get_config");
      updateMenuToggles();
    } catch (_) {}
  }

  async function setDetectionModel(model) {
    const nextModel = model === "weighted_sum" ? "weighted_sum" : "crf";
    if (!currentConfig) {
      try {
        currentConfig = await invoke("get_config");
      } catch (_) {
        currentConfig = {};
      }
    }
    if (currentConfig.tagger === nextModel) {
      updateMenuToggles();
      return;
    }
    currentConfig.tagger = nextModel;
    updateMenuToggles();
    try {
      await invoke("set_config", { config: currentConfig });
      if (source) {
        revision = 0;
        analysis = null;
        await refresh();
      }
      setStatus(
        nextModel === "crf"
          ? "Switched block detection to the CRF model"
          : "Switched block detection to the linear weighted sum"
      );
    } catch (err) {
      setStatus("Failed to change detection model: " + err, true);
    }
  }

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
    currentConfig = config;
    el.configError.hidden = true;
    updateMenuToggles();
    if (el.cfgTagger) {
      el.cfgTagger.value = config.tagger === "weighted_sum" ? "weighted_sum" : "crf";
    }
    if (el.cfgThreshold) el.cfgThreshold.value = String(config.threshold ?? 0.5);
    el.cfgWeights.replaceChildren();
    for (const [key, value] of Object.entries(config.weights || {})) {
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
    if (el.cfgTagger) {
      config.tagger = el.cfgTagger.value === "weighted_sum" ? "weighted_sum" : "crf";
    }
    if (el.cfgThreshold) config.threshold = Number(el.cfgThreshold.value);
    config.weights = config.weights || {};
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
      const cfg = readConfig();
      await invoke("set_config", { config: cfg });
      currentConfig = cfg;
      updateMenuToggles();
      el.configDialog.close();
      if (source) {
        revision = 0;
        analysis = null;
        await refresh();
      }
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
      const cfg = readConfig();
      await invoke("set_config", { config: cfg });
      currentConfig = cfg;
      updateMenuToggles();
      const dest = await invoke("save_config");
      if (dest) setStatus("Detection settings saved to " + dest);
      refresh(true);
    } catch (error) {
      configFailed(error);
    }
  });

  // ── Decision model ───────────────────────────────────────────────────────────

  const DM_PLACEHOLDERS = {
    "typesafe": { model: "e.g. jev-latest", url: "https://api.typesafe.ai" },
    "anthropic": { model: "e.g. claude-haiku-5-5", url: "https://api.anthropic.com" },
    "open-ai-compatible": { model: "e.g. gpt-4o-mini or llama3", url: "https://api.openai.com/v1" },
  };
  /** What the backend reports: settings, whether a key is stored. */
  let decision = null;
  /** The stored key is to be deleted when the dialog is saved. */
  let dmClearKey = false;
  let modelRunning = false;

  function updateDecisionRun() {
    const disabled = !(analysis && decision && decision.ready) || modelRunning;
    if (el.decisionRun) el.decisionRun.disabled = disabled;
    const winDecisionRun = $("win-menu-decision-run");
    if (winDecisionRun) winDecisionRun.disabled = disabled;
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
    el.dmKeyHint.textContent = d.provider === "anthropic"
      ? "" : "A server at your own address may not need one.";
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
      const before = cloneOverrides(overrides);
      overrides = Object.assign(emptyOverrides(), run.overrides);
      if (!sameOverrides(before, overrides)) {
        pushHistory({ kind: "overrides", before, after: cloneOverrides(overrides) });
      }
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
      showLoading("Compiling HTML files…", "Exporting folder to HTML files");
      const result = await invoke("export_folder", { options: options() });
      if (result) setStatus("Exported " + result[0] + " file(s) to " + result[1]);
      else setStatus("");
    } catch (error) {
      setStatus(String(error), true);
    } finally {
      await finishLoading();
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

  function setPromptMode(mode) {
    promptMode = mode === "continuation" ? "continuation" : "initial";
    const isCont = promptMode === "continuation";
    if (el.promptTabInitial) el.promptTabInitial.setAttribute("aria-pressed", String(!isCont));
    if (el.promptTabContinuation) el.promptTabContinuation.setAttribute("aria-pressed", String(isCont));
    if (el.promptText) el.promptText.value = isCont ? continuationPromptText : initialPromptText;
    if (el.guideDesc) {
      el.guideDesc.innerHTML = isCont
        ? "Paste this into your vision model for <strong>subsequent batches</strong> when transcribing a multi-part document, so split sentences, continuing proofs, and lists concatenate seamlessly."
        : "Paste this into a vision model as the first message or system prompt for the <strong>first batch</strong> (or a single document), then attach photos of the pages. Copy its answer with the code block's own copy button, so no <code>$</code> is lost, and save it as a <code>.md</code> file.";
    }
    if (el.promptText) el.promptText.scrollTop = 0;
  }

  async function copyPrompt(button, customText) {
    const text = typeof customText === "string" ? customText : el.promptText.value;
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
    const isCont = customText ? (customText === continuationPromptText) : (promptMode === "continuation");
    setStatus(isCont
      ? "Continuation prompt copied. Paste it into your vision model for the next batch."
      : "Prompt copied. Paste it into your vision model, then attach the pages.");
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

  let activeWinMenuBtn = null;
  let isWinMenuOpen = false;

  function closeWinMenus() {
    isWinMenuOpen = false;
    activeWinMenuBtn = null;
    if (el.winMenubar) {
      el.winMenubar.querySelectorAll(".menubar-dropdown").forEach((menu) => {
        menu.hidden = true;
      });
      el.winMenubar.querySelectorAll(".menubar-btn").forEach((btn) => {
        btn.setAttribute("aria-expanded", "false");
      });
    }
  }

  function openWinMenu(btn) {
    closeWinMenus();
    const dropdown = btn.parentElement ? btn.parentElement.querySelector(".menubar-dropdown") : null;
    if (!dropdown) return;
    dropdown.hidden = false;
    btn.setAttribute("aria-expanded", "true");
    activeWinMenuBtn = btn;
    isWinMenuOpen = true;
  }

  if (el.winMenubar) {
    el.winMenubar.querySelectorAll(".menubar-btn").forEach((btn) => {
      btn.addEventListener("click", (e) => {
        e.stopPropagation();
        if (isWinMenuOpen && activeWinMenuBtn === btn) {
          closeWinMenus();
        } else {
          openWinMenu(btn);
        }
      });

      btn.addEventListener("mouseenter", () => {
        if (isWinMenuOpen && activeWinMenuBtn !== btn) {
          openWinMenu(btn);
        }
      });
    });

    const recentParent = $("win-recent-parent");
    if (recentParent) {
      recentParent.addEventListener("mouseenter", () => {
        if (el.winRecentMenu) el.winRecentMenu.hidden = false;
      });
      recentParent.addEventListener("mouseleave", () => {
        if (el.winRecentMenu) el.winRecentMenu.hidden = true;
      });
    }

    el.winMenubar.addEventListener("click", (e) => {
      const actionBtn = e.target.closest("button.menu-action");
      if (!actionBtn || actionBtn.disabled) return;
      if (actionBtn.classList.contains("submenu-trigger")) return;
      const action = actionBtn.dataset.action;
      const path = actionBtn.dataset.path;
      closeWinMenus();
      if (path) {
        openRecent(path);
      } else if (action) {
        handleMenuAction(action);
      }
    });
  }

  let updateInstalling = false;

  function formatBytes(bytes) {
    if (!bytes || bytes < 0) return "0 B";
    const units = ["B", "KB", "MB", "GB"];
    let value = bytes;
    let unit = 0;
    while (value >= 1024 && unit < units.length - 1) {
      value /= 1024;
      unit += 1;
    }
    return value.toFixed(unit === 0 ? 0 : 1) + " " + units[unit];
  }

  function showUpdateDialog(info) {
    if (el.updateTitle) el.updateTitle.textContent = "Update available";
    if (el.updateSummary) {
      el.updateSummary.textContent = "Mathia " + info.version + " is available. You are running " + info.currentVersion + ".";
    }
    if (el.updateNotes) {
      const notes = (info.notes || "").trim();
      el.updateNotes.textContent = notes;
      el.updateNotes.hidden = notes.length === 0;
    }
    if (el.updateError) {
      el.updateError.textContent = "";
      el.updateError.hidden = true;
    }
    if (el.updateProgressWrap) el.updateProgressWrap.hidden = true;
    if (el.updateProgressFill) el.updateProgressFill.style.width = "0%";
    if (el.updateInstall) {
      el.updateInstall.disabled = false;
      el.updateInstall.textContent = "Install and Restart";
    }
    if (el.updateLater) el.updateLater.disabled = false;
    updateInstalling = false;
    if (el.updateDialog && !el.updateDialog.open) el.updateDialog.showModal();
  }

  async function checkForUpdate(manual) {
    if (updateInstalling) return;
    if (manual) setStatus("Checking for updates…");
    try {
      const info = await invoke("check_for_update");
      if (info) {
        if (manual) setStatus("");
        showUpdateDialog(info);
      } else if (manual) {
        setStatus("Mathia is up to date.");
      }
    } catch (error) {
      if (manual) setStatus("Update check failed: " + error, true);
    }
  }

  async function installUpdate() {
    if (updateInstalling) return;
    updateInstalling = true;
    if (el.updateError) {
      el.updateError.textContent = "";
      el.updateError.hidden = true;
    }
    if (el.updateInstall) {
      el.updateInstall.disabled = true;
      el.updateInstall.textContent = "Installing…";
    }
    if (el.updateLater) el.updateLater.disabled = true;
    if (el.updateProgressWrap) el.updateProgressWrap.hidden = false;
    if (el.updateProgressText) el.updateProgressText.textContent = "Downloading…";
    try {
      await invoke("install_update");
      // A successful install restarts the app before this resolves.
    } catch (error) {
      updateInstalling = false;
      if (el.updateInstall) {
        el.updateInstall.disabled = false;
        el.updateInstall.textContent = "Install and Restart";
      }
      if (el.updateLater) el.updateLater.disabled = false;
      if (el.updateError) {
        el.updateError.textContent = String(error);
        el.updateError.hidden = false;
      }
    }
  }

  function handleMenuAction(action) {
    switch (action) {
      case "open":
        open();
        break;
      case "combine":
        combineMarkdowns();
        break;
      case "clear_recent":
        invoke("clear_recent")
          .then(loadRecent)
          .catch((err) => setStatus(String(err), true));
        break;
      case "home":
        if (analysis) goHome();
        break;
      case "reload":
        if (analysis) refresh();
        break;
      case "export_html":
        if (analysis) exportHtml();
        break;
      case "export_markdown":
        if (analysis) exportMarkdown();
        break;
      case "export_pdf":
        if (analysis) exportPdf();
        break;
      case "export_folder":
        exportFolder();
        break;
      case "close_window":
        if (window.__TAURI__?.window?.getCurrentWindow) {
          window.__TAURI__.window.getCurrentWindow().close();
        } else {
          window.close();
        }
        break;
      case "undo": {
        const active = document.activeElement;
        const typing = active && (/^(INPUT|TEXTAREA|SELECT)$/.test(active.tagName || "") || active.isContentEditable);
        if (typing) {
          document.execCommand("undo");
        } else {
          undoChange();
        }
        break;
      }
      case "redo": {
        const active = document.activeElement;
        const typing = active && (/^(INPUT|TEXTAREA|SELECT)$/.test(active.tagName || "") || active.isContentEditable);
        if (typing) {
          document.execCommand("redo");
        } else {
          redoChange();
        }
        break;
      }
      case "fix_markdown":
        if (analysis) fixMarkdown();
        break;
      case "preview_toggle":
        if (analysis) setPreviewOpen(!previewOpen);
        break;
      case "theme_toggle": {
        const themeBtn = document.getElementById("theme-toggle");
        if (themeBtn) themeBtn.click();
        break;
      }
      case "guide":
        showGuide("initial");
        break;
      case "decision":
        showDecision();
        break;
      case "decision_run":
        if (analysis && decision && decision.ready && !modelRunning) {
          runDecision();
        }
        break;
      case "scoring":
        showConfig();
        break;
      case "toggle_model":
        setDetectionModel(
          currentConfig && currentConfig.tagger === "crf" ? "weighted_sum" : "crf"
        );
        break;
      case "toggle_render_math":
        el.math.checked = !el.math.checked;
        updateMenuToggles();
        refresh();
        break;
      case "toggle_ai":
        el.ai.checked = !el.ai.checked;
        updateMenuToggles();
        refresh();
        break;
      case "about":
        if (el.aboutDialog && !el.aboutDialog.open) {
          el.aboutDialog.showModal();
        }
        break;
      case "check_update":
        checkForUpdate(true);
        break;
    }
  }

  document.addEventListener("click", (event) => {
    if (!event.target.closest(".menu-wrap")) closeMenus();
    if (isWinMenuOpen && el.winMenubar && !el.winMenubar.contains(event.target)) {
      closeWinMenus();
    }
  });
  // A click in the preview frame never reaches this document.
  window.addEventListener("blur", () => {
    closeMenus();
    closeWinMenus();
  });

  function showGuide(mode) {
    if (mode) setPromptMode(mode);
    if (!el.guideDialog.open) el.guideDialog.showModal();
    if (el.promptText) el.promptText.scrollTop = 0;
    el.copyPrompt.focus();
  }

  if (el.promptTabInitial) el.promptTabInitial.addEventListener("click", () => setPromptMode("initial"));
  if (el.promptTabContinuation) el.promptTabContinuation.addEventListener("click", () => setPromptMode("continuation"));
  el.guide.addEventListener("click", () => showGuide("initial"));
  el.viewPrompt.addEventListener("click", () => showGuide("initial"));
  if (el.orderPromptBtn) el.orderPromptBtn.addEventListener("click", () => showGuide("continuation"));
  el.guideClose.addEventListener("click", () => el.guideDialog.close());
  el.copyPrompt.addEventListener("click", () => copyPrompt(el.copyPrompt));
  el.copyPromptEmpty.addEventListener("click", () => copyPrompt(el.copyPromptEmpty, initialPromptText));
  if (el.copyContinuationPrompt) {
    el.copyContinuationPrompt.addEventListener("click", () => copyPrompt(el.copyContinuationPrompt, continuationPromptText));
  }
  // A click on the backdrop (outside the dialog box) closes it.
  el.guideDialog.addEventListener("click", (event) => {
    if (event.target === el.guideDialog) el.guideDialog.close();
  });

  // ── Multi-markdown ordering ───────────────────────────────────────────

  // Reorder list items via pointer events. Standard HTML5 drag-and-drop is
  // intercepted by Tauri's window-level file drop handler on macOS/Windows,
  // so pointer events provide smooth, reliable in-app reordering.
  el.orderList.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    if (e.target.closest("button")) return;
    const item = e.target.closest(".order-item");
    if (!item) return;

    const startIndex = Number(item.dataset.index);
    if (Number.isNaN(startIndex) || orderPaths.length <= 1) return;

    const pointerId = e.pointerId;
    const startY = e.clientY;
    const startX = e.clientX;
    const initialRect = item.getBoundingClientRect();
    const grabOffsetY = e.clientY - initialRect.top;

    let isDragging = false;
    let targetIndex = startIndex;
    let ghost = null;

    function onPointerMove(moveEvent) {
      if (moveEvent.pointerId !== pointerId) return;

      const dy = moveEvent.clientY - startY;
      const dx = moveEvent.clientX - startX;

      if (!isDragging) {
        if (Math.hypot(dx, dy) < 4) return;
        isDragging = true;
        item.classList.add("dragging");

        ghost = item.cloneNode(true);
        ghost.classList.add("order-item-ghost");
        ghost.style.position = "fixed";
        ghost.style.left = initialRect.left + "px";
        ghost.style.top = (moveEvent.clientY - grabOffsetY) + "px";
        ghost.style.width = initialRect.width + "px";
        ghost.style.height = initialRect.height + "px";
        ghost.style.margin = "0";
        ghost.style.zIndex = "1000";
        ghost.style.pointerEvents = "none";
        ghost.style.boxShadow = "0 8px 24px rgba(0, 0, 0, 0.22)";
        ghost.style.transform = "scale(1.02)";
        ghost.style.transition = "none";
        ghost.style.opacity = "0.95";
        el.orderDialog.appendChild(ghost);
        document.body.style.cursor = "grabbing";
      }

      moveEvent.preventDefault();

      if (ghost) {
        ghost.style.top = (moveEvent.clientY - grabOffsetY) + "px";
      }

      const scrollContainer = el.orderList.parentElement;
      if (scrollContainer) {
        const containerRect = scrollContainer.getBoundingClientRect();
        if (moveEvent.clientY < containerRect.top + 32) {
          scrollContainer.scrollTop -= 6;
        } else if (moveEvent.clientY > containerRect.bottom - 32) {
          scrollContainer.scrollTop += 6;
        }
      }

      const allItems = Array.from(el.orderList.querySelectorAll(".order-item"));
      let newTargetIndex = startIndex;

      for (let i = 0; i < allItems.length; i++) {
        const r = allItems[i].getBoundingClientRect();
        if (moveEvent.clientY >= r.top && moveEvent.clientY <= r.bottom) {
          newTargetIndex = i;
          break;
        }
      }

      if (allItems.length > 0) {
        if (moveEvent.clientY < allItems[0].getBoundingClientRect().top) {
          newTargetIndex = 0;
        } else if (moveEvent.clientY > allItems[allItems.length - 1].getBoundingClientRect().bottom) {
          newTargetIndex = allItems.length - 1;
        }
      }

      targetIndex = newTargetIndex;

      allItems.forEach((it, i) => {
        if (isDragging && i === targetIndex && i !== startIndex) {
          it.classList.add("drag-over");
        } else {
          it.classList.remove("drag-over");
        }
      });
    }

    function cleanup() {
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", onPointerUp);
      window.removeEventListener("pointercancel", onPointerCancel);
      window.removeEventListener("keydown", onKeyDown);
      document.body.style.cursor = "";

      if (ghost) {
        ghost.remove();
        ghost = null;
      }

      item.classList.remove("dragging");
      const allItems = el.orderList.querySelectorAll(".order-item");
      allItems.forEach((it) => it.classList.remove("drag-over"));
    }

    function onPointerUp(upEvent) {
      if (upEvent.pointerId !== pointerId) return;
      const wasDragging = isDragging;
      const finalTarget = targetIndex;
      cleanup();

      if (wasDragging && finalTarget !== null && finalTarget !== startIndex) {
        const [moved] = orderPaths.splice(startIndex, 1);
        orderPaths.splice(finalTarget, 0, moved);
        renderOrderList();
      }
    }

    function onPointerCancel(cancelEvent) {
      if (cancelEvent.pointerId !== pointerId) return;
      cleanup();
    }

    function onKeyDown(keyEvent) {
      if (keyEvent.key === "Escape") {
        cleanup();
      }
    }

    window.addEventListener("pointermove", onPointerMove, { passive: false });
    window.addEventListener("pointerup", onPointerUp);
    window.addEventListener("pointercancel", onPointerCancel);
    window.addEventListener("keydown", onKeyDown);
  });

  function openOrderDialog(paths) {
    orderPaths = [];
    if (paths && paths.length > 0) {
      for (const p of paths) {
        if (!orderPaths.includes(p)) orderPaths.push(p);
      }
    }
    el.orderTitleInput.value = customTitle || "";
    renderOrderList();
    if (!el.orderDialog.open) el.orderDialog.showModal();
    el.orderCompile.focus();
  }

  function renderOrderList() {
    const total = orderPaths.length;
    el.orderCount.textContent = total === 1 ? "1 file" : total + " files";
    el.orderEmpty.hidden = total > 0;
    el.orderCompile.disabled = total === 0;

    el.orderList.replaceChildren(...orderPaths.map((path, index) => {
      const item = node("li", "order-item");
      item.dataset.index = String(index);

      // Drag handle icon
      const handle = node("span", "order-drag-handle");
      handle.innerHTML = '<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"><circle cx="5" cy="3" r="1.5"/><circle cx="11" cy="3" r="1.5"/><circle cx="5" cy="8" r="1.5"/><circle cx="11" cy="8" r="1.5"/><circle cx="5" cy="13" r="1.5"/><circle cx="11" cy="13" r="1.5"/></svg>';
      handle.title = "Drag to reorder";

      // Order number badge
      const badge = node("span", "order-index", String(index + 1));

      // Info: file name and directory
      const info = node("div", "order-info");
      info.append(node("span", "order-name", fileName(path)));
      info.append(node("span", "order-dir", parentDir(path)));

      // Actions: Move up, Move down, Remove
      const actions = node("div", "order-actions");

      const upBtn = node("button", "btn small", "▲");
      upBtn.type = "button";
      upBtn.title = "Move up";
      upBtn.disabled = index === 0;
      upBtn.addEventListener("click", () => {
        if (index > 0) {
          const temp = orderPaths[index];
          orderPaths[index] = orderPaths[index - 1];
          orderPaths[index - 1] = temp;
          renderOrderList();
          const items = el.orderList.querySelectorAll(".order-item");
          if (items[index - 1]) {
            const btn = items[index - 1].querySelector('button[title="Move up"]');
            if (btn) btn.focus();
          }
        }
      });

      const downBtn = node("button", "btn small", "▼");
      downBtn.type = "button";
      downBtn.title = "Move down";
      downBtn.disabled = index === total - 1;
      downBtn.addEventListener("click", () => {
        if (index < total - 1) {
          const temp = orderPaths[index];
          orderPaths[index] = orderPaths[index + 1];
          orderPaths[index + 1] = temp;
          renderOrderList();
          const items = el.orderList.querySelectorAll(".order-item");
          if (items[index + 1]) {
            const btn = items[index + 1].querySelector('button[title="Move down"]');
            if (btn) btn.focus();
          }
        }
      });

      const removeBtn = node("button", "btn small", "✕");
      removeBtn.type = "button";
      removeBtn.title = "Remove";
      removeBtn.addEventListener("click", () => {
        orderPaths.splice(index, 1);
        renderOrderList();
      });

      actions.append(upBtn, downBtn, removeBtn);
      item.append(handle, badge, info, actions);
      return item;
    }));
  }

  el.orderAdd.addEventListener("click", async () => {
    try {
      const paths = await invoke("pick_markdown");
      if (paths && paths.length > 0) {
        for (const p of paths) {
          if (!orderPaths.includes(p)) orderPaths.push(p);
        }
        renderOrderList();
      }
    } catch (error) {
      setStatus(String(error), true);
    }
  });

  el.orderClear.addEventListener("click", () => {
    orderPaths = [];
    renderOrderList();
  });

  el.orderCancel.addEventListener("click", () => {
    el.orderDialog.close();
  });

  el.orderDialog.addEventListener("click", (event) => {
    if (event.target === el.orderDialog) el.orderDialog.close();
  });

  el.orderCompile.addEventListener("click", () => {
    if (orderPaths.length === 0) return;
    const title = el.orderTitleInput.value.trim();
    el.orderDialog.close();
    openMultiPath(orderPaths, title).then(loadRecent);
  });

  el.open.addEventListener("click", open);
  el.openWelcome.addEventListener("click", open);
  el.openMulti.addEventListener("click", combineMarkdowns);
  el.openMultiWelcome.addEventListener("click", combineMarkdowns);
  el.detailClose.addEventListener("click", deselect);
  el.filter.addEventListener("input", renderBlocks);
  el.typeFilter.addEventListener("change", () => {
    typeFilter = el.typeFilter.value;
    renderBlocks();
  });
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
  el.home.addEventListener("click", goHome);
  el.reload.addEventListener("click", () => refresh());
  el.undo.addEventListener("click", () => undoChange());
  el.redo.addEventListener("click", () => redoChange());
  el.export.addEventListener("click", exportHtml);
  el.previewToggle.addEventListener("click", () => setPreviewOpen(!previewOpen));
  el.ai.addEventListener("change", () => {
    updateMenuToggles();
    refresh();
  });
  if (el.optCrfModel) {
    el.optCrfModel.addEventListener("change", () => {
      setDetectionModel(el.optCrfModel.checked ? "crf" : "weighted_sum");
    });
  }
  el.math.addEventListener("change", () => {
    updateMenuToggles();
    refresh();
  });
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
  el.exportMarkdown.addEventListener("click", exportMarkdown);
  el.exportPdf.addEventListener("click", exportPdf);
  el.fixMarkdown.addEventListener("click", fixMarkdown);

  if (el.aboutClose && el.aboutDialog) {
    el.aboutClose.addEventListener("click", () => el.aboutDialog.close());
  }
  if (el.updateLater && el.updateDialog) {
    el.updateLater.addEventListener("click", () => el.updateDialog.close());
  }
  if (el.updateInstall) {
    el.updateInstall.addEventListener("click", installUpdate);
  }
  if (el.updateDialog) {
    el.updateDialog.addEventListener("click", (event) => {
      if (event.target === el.updateDialog && !updateInstalling) el.updateDialog.close();
    });
    el.updateDialog.addEventListener("cancel", (event) => {
      if (updateInstalling) event.preventDefault();
    });
  }
  invoke("app_version")
    .then((version) => {
      if (el.aboutVersion) el.aboutVersion.textContent = "Version " + version;
    })
    .catch(() => {});

  window.addEventListener("keydown", (event) => {
    if (el.guideDialog.open || el.configDialog.open || (el.aboutDialog && el.aboutDialog.open)) return;
    if (event.key === "Escape") {
      closeMenus();
      closeWinMenus();
    }
    if (!(event.metaKey || event.ctrlKey) || event.altKey) return;
    const key = event.key.toLowerCase();
    // Text fields keep their own undo while the user is typing in them.
    const typing = /^(INPUT|TEXTAREA|SELECT)$/.test(event.target.tagName || "") ||
      event.target.isContentEditable;
    if (key === "z" && !typing) {
      if (event.shiftKey) redoChange();
      else undoChange();
    } else if (key === "y" && !typing) redoChange();
    else if (key === "o") open();
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
  loadConfig();
  loadRecent();
  invoke("startup_file")
    .then((paths) => {
      if (!paths || paths.length === 0) return;
      if (paths.length === 1) openPath(paths[0]);
      else openOrderDialog(paths);
    })
    .catch((error) => setStatus(String(error), true))
    .finally(loadRecent);

  listen("compile-progress", (event) => {
    if (!loadingVisible) return;
    const data = event.payload;
    if (data && typeof data.progress === "number") {
      const pct = Math.round(data.progress * 100);
      if (pct >= currentProgress) {
        setProgress(pct, data.stage);
      } else if (data.stage && el.loadingStage) {
        el.loadingStage.textContent = data.stage;
      }
    }
  });

  listen("menu-action", (event) => {
    handleMenuAction(event.payload);
  });
  listen("menu-open-recent", (event) => {
    if (event.payload) openRecent(event.payload);
  });
  listen("update-available", (event) => {
    if (!event.payload) return;
    if (el.updateDialog && el.updateDialog.open) return;
    showUpdateDialog(event.payload);
  });
  listen("update-progress", (event) => {
    const data = event.payload;
    if (!data) return;
    if (el.updateProgressWrap) el.updateProgressWrap.hidden = false;
    const percent = Math.round((data.percent || 0) * 100);
    if (el.updateProgressFill) el.updateProgressFill.style.width = percent + "%";
    if (el.updateProgressTrack) el.updateProgressTrack.setAttribute("aria-valuenow", String(percent));
    if (el.updateProgressText) {
      const total = data.total ? " of " + formatBytes(data.total) : "";
      el.updateProgressText.textContent = "Downloading… " + percent + "% (" + formatBytes(data.downloaded) + total + ")";
    }
  });

  listen("tauri://drag-enter", () => { el.drop.hidden = false; });
  listen("tauri://drag-leave", () => { el.drop.hidden = true; });
  listen("tauri://drag-drop", (event) => {
    el.drop.hidden = true;
    const paths = (event.payload.paths || []).filter((p) => SUPPORTED_INPUT.test(p));
    if (paths.length === 0) {
      setStatus("Drop Markdown or HTML files (.md, .html, .txt)", true);
      return;
    }
    if (el.orderDialog.open) {
      for (const p of paths) {
        if (!orderPaths.includes(p)) orderPaths.push(p);
      }
      renderOrderList();
    } else if (paths.length === 1) {
      openPath(paths[0]).then(loadRecent);
    } else {
      openOrderDialog(paths);
    }
  });
})();
