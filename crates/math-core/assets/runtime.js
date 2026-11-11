/* Mathia micro-runtime: math rendering, sidebar, and the block hook API that
   the interactive features (SVG explainers, per-block AI questions) build on. */
(function () {
  "use strict";

  function renderMath() {
    if (!window.katex) return;
    document.querySelectorAll(".math").forEach(function (el) {
      if (el.dataset.mxTex !== undefined) return;
      var tex = el.textContent;
      el.dataset.mxTex = tex;
      try {
        window.katex.render(tex, el, {
          displayMode: el.classList.contains("math-display"),
          throwOnError: false,
        });
      } catch (err) {
        el.classList.add("mx-math-error");
        el.title = String(err);
      }
    });
    decorateMath();
  }

  var LANG_FA = document.documentElement.lang === "fa";
  var T = {
    copy: LANG_FA ? "کپی" : "Copy",
    copied: LANG_FA ? "کپی شد" : "Copied",
    copyTex: LANG_FA ? "کپی LaTeX" : "Copy LaTeX",
    clickTex: LANG_FA ? "برای کپی LaTeX کلیک کنید" : "Click to copy LaTeX",
  };

  /* LaTeX source of a math element, as authored. */
  function texOf(m) {
    return m.dataset.mxTex !== undefined ? m.dataset.mxTex : m.textContent;
  }
  function wrapTex(m, tex) {
    return m.classList.contains("math-display") ? "$$" + tex + "$$" : "$" + tex + "$";
  }

  /* Copy text to the clipboard; works on file: pages without the async API. */
  function copyText(text) {
    function fallback() {
      var ta = document.createElement("textarea");
      ta.value = text;
      ta.setAttribute("readonly", "");
      ta.style.cssText = "position:fixed;top:0;left:0;opacity:0";
      document.body.appendChild(ta);
      ta.select();
      try { document.execCommand("copy"); } catch (err) { /* nothing more to try */ }
      document.body.removeChild(ta);
      return Promise.resolve();
    }
    if (navigator.clipboard && navigator.clipboard.writeText) {
      return navigator.clipboard.writeText(text).catch(fallback);
    }
    return fallback();
  }

  function flash(el, done) {
    el.classList.add("mx-copied");
    window.setTimeout(function () { el.classList.remove("mx-copied"); if (done) done(); }, 1200);
  }

  /* Every formula can be copied as LaTeX: display formulas get a button,
     inline ones copy on click (unless the reader is selecting text). */
  function decorateMath() {
    document.querySelectorAll(".math").forEach(function (m) {
      if (m.dataset.mxCopy) return;
      if (m.dataset.mxTex === undefined && !window.katex) m.dataset.mxTex = m.textContent;
      m.dataset.mxCopy = "1";
      m.title = T.clickTex;
      if (!m.classList.contains("math-display")) return;
      var b = document.createElement("button");
      b.type = "button";
      b.className = "mx-math-copy";
      b.textContent = T.copyTex;
      b.setAttribute("aria-label", T.copyTex);
      b.addEventListener("click", function (e) {
        e.stopPropagation();
        copyText(texOf(m)).then(function () {
          b.textContent = T.copied;
          flash(b, function () { b.textContent = T.copyTex; });
        });
      });
      m.appendChild(b);
    });
  }

  function setupCopy() {
    document.addEventListener("click", function (e) {
      var m = e.target.closest && e.target.closest(".math");
      if (!m || m.classList.contains("math-display") || e.target.closest("button")) return;
      var sel = window.getSelection();
      if (sel && String(sel).length) return;
      copyText(texOf(m)).then(function () { flash(m); });
    });
    /* Ctrl/Cmd+C over a selection yields LaTeX for formulas, not KaTeX's MathML. */
    document.addEventListener("copy", function (e) {
      var sel = window.getSelection();
      if (!sel || sel.isCollapsed || !sel.rangeCount || !e.clipboardData) return;
      var range = sel.getRangeAt(0);
      var holder = range.commonAncestorContainer;
      var host = (holder.nodeType === 1 ? holder : holder.parentElement);
      var inside = host && host.closest && host.closest(".math");
      var text;
      if (inside) {
        text = texOf(inside);
      } else {
        var box = document.createElement("div");
        box.appendChild(range.cloneContents());
        if (!box.querySelector(".math, .mx-math-copy")) return;
        box.querySelectorAll(".mx-math-copy").forEach(function (n) { n.remove(); });
        box.querySelectorAll(".math").forEach(function (n) {
          n.textContent = wrapTex(n, n.dataset.mxTex !== undefined ? n.dataset.mxTex : "");
        });
        text = box.textContent;
      }
      e.clipboardData.setData("text/plain", text);
      e.preventDefault();
    });
    /* A Copy button on every definition, theorem, proof, example, ... */
    window.Mathia.registerAction({
      id: "copy",
      label: T.copy,
      run: function (block, button) {
        copyText(blockText(block.element)).then(function () {
          if (!button) return;
          button.textContent = T.copied;
          flash(button, function () { button.textContent = T.copy; });
        });
      },
    });
  }

  /* A block as plain text: its label, then the body with formulas as LaTeX. */
  function blockText(el) {
    var badge = el.querySelector(".mx-badge");
    var clone = el.cloneNode(true);
    clone.querySelectorAll(".mx-block-head, .mx-math-copy, [class*='mx-ai']").forEach(function (n) { n.remove(); });
    clone.querySelectorAll(".math").forEach(function (m) {
      m.textContent = wrapTex(m, m.dataset.mxTex !== undefined ? m.dataset.mxTex : m.textContent);
    });
    var body = clone.textContent.trim();
    return (badge ? badge.textContent.trim() + "\n\n" : "") + body;
  }

  function setupSidebar() {
    var root = document.documentElement;
    var toggle = document.getElementById("mx-toc-toggle");
    var narrow = window.matchMedia("(max-width: 59.99rem)");
    function set(collapsed) {
      root.classList.toggle("mx-toc-collapsed", collapsed);
      if (toggle) toggle.setAttribute("aria-expanded", String(!collapsed));
    }
    set(narrow.matches);
    if (toggle) {
      toggle.addEventListener("click", function () {
        set(!root.classList.contains("mx-toc-collapsed"));
      });
    }
    document.querySelectorAll("#mx-toc a").forEach(function (link) {
      link.addEventListener("click", function () {
        if (narrow.matches) set(true);
      });
    });
  }

  /* Sections of the sidebar fold: a heading hides the entries nested under it. */
  function setupTocFolding() {
    var items = Array.prototype.slice.call(document.querySelectorAll(".mx-toc-item"));
    function depth(item) { return Number(item.dataset.depth) || 1; }
    function refresh() {
      var folded = []; /* depths of the collapsed headings we are inside */
      items.forEach(function (item) {
        var d = depth(item);
        while (folded.length && folded[folded.length - 1] >= d) folded.pop();
        item.hidden = folded.length > 0;
        if (item.dataset.mxFolded === "1") folded.push(d);
      });
    }
    items.forEach(function (item, index) {
      var next = items[index + 1];
      if (!item.classList.contains("mx-toc-heading") || !next || depth(next) <= depth(item)) return;
      var button = document.createElement("button");
      button.type = "button";
      button.className = "mx-toc-fold";
      button.textContent = "▾";
      button.setAttribute("aria-expanded", "true");
      button.setAttribute("aria-label", item.textContent.trim());
      button.addEventListener("click", function () {
        var fold = item.dataset.mxFolded !== "1";
        item.dataset.mxFolded = fold ? "1" : "0";
        button.setAttribute("aria-expanded", String(!fold));
        refresh();
      });
      item.insertBefore(button, item.firstChild);
    });
  }

  /* Light / dark switch. The choice is remembered per browser; without one the
     page follows the system setting. */
  function setupTheme() {
    var root = document.documentElement;
    var toggle = document.getElementById("mx-theme-toggle");
    var KEY = "mathia-theme";
    function stored() {
      try { return window.localStorage.getItem(KEY); } catch (err) { return null; }
    }
    function apply(theme) {
      if (theme === "light" || theme === "dark") root.dataset.mxTheme = theme;
      else delete root.dataset.mxTheme;
    }
    apply(stored());
    if (!toggle) return;
    toggle.hidden = false;
    toggle.addEventListener("click", function () {
      var dark = root.dataset.mxTheme
        ? root.dataset.mxTheme === "dark"
        : window.matchMedia("(prefers-color-scheme: dark)").matches;
      var next = dark ? "light" : "dark";
      apply(next);
      try { window.localStorage.setItem(KEY, next); } catch (err) { /* private mode */ }
    });
  }

  /* Full-text search over the document body. Matches are wrapped in <mark>;
     Persian/Arabic letter variants, diacritics and ZWNJ are folded together so
     OCR-typed and keyboard-typed text find each other. Math is matched against
     its LaTeX source and highlighted as a whole. */
  function setupSearch() {
    var box = document.getElementById("mx-search");
    var input = document.getElementById("mx-search-input");
    var main = document.getElementById("mx-main");
    if (!box || !input || !main) return;
    var count = document.getElementById("mx-search-count");
    var prev = document.getElementById("mx-search-prev");
    var next = document.getElementById("mx-search-next");
    box.hidden = false;

    var FOLD = { "ي": "ی", "ى": "ی", "ك": "ک", "ة": "ه", "أ": "ا", "إ": "ا", "آ": "ا" };
    var DROP = /[ـً-ٰٟ‌‍­]/;
    /* Folded string plus, for each folded char, its index in the original. */
    function fold(text) {
      var out = "", map = [];
      for (var i = 0; i < text.length; i++) {
        var c = text.charAt(i);
        if (DROP.test(c)) continue;
        c = (FOLD[c] || c).toLowerCase();
        for (var k = 0; k < c.length; k++) { out += c.charAt(k); map.push(i); }
      }
      return { text: out, map: map };
    }

    var hits = [], current = -1, timer = null;

    function clear() {
      main.querySelectorAll("mark.mx-hit").forEach(function (m) {
        var parent = m.parentNode;
        while (m.firstChild) parent.insertBefore(m.firstChild, m);
        parent.removeChild(m);
        parent.normalize();
      });
      main.querySelectorAll(".mx-hit-math").forEach(function (el) { el.classList.remove("mx-hit-math", "mx-hit-current"); });
      hits = []; current = -1;
    }

    function skip(node) {
      for (var el = node.parentElement; el && el !== main; el = el.parentElement) {
        if (el.classList.contains("math") || el.classList.contains("mx-block-head") || el.classList.contains("mx-actions")) return true;
        if (el.tagName === "SCRIPT" || el.tagName === "STYLE" || el.tagName === "SVG" || el.tagName === "svg") return true;
      }
      return false;
    }

    function run(query) {
      clear();
      var q = fold(query.trim()).text;
      if (!q) { update(); return; }
      var nodes = [], walker = document.createTreeWalker(main, NodeFilter.SHOW_TEXT);
      while (walker.nextNode()) if (!skip(walker.currentNode)) nodes.push(walker.currentNode);
      nodes.forEach(function (node) {
        var f = fold(node.nodeValue), ranges = [], at = 0, found;
        while ((found = f.text.indexOf(q, at)) !== -1) {
          var start = f.map[found];
          var end = f.map[found + q.length - 1] + 1;
          ranges.push([start, end]);
          at = found + q.length;
        }
        for (var r = ranges.length - 1; r >= 0; r--) {
          var tail = node.splitText(ranges[r][0]);
          tail.splitText(ranges[r][1] - ranges[r][0]);
          var mark = document.createElement("mark");
          mark.className = "mx-hit";
          tail.parentNode.insertBefore(mark, tail);
          mark.appendChild(tail);
          hits.unshift(mark);
        }
      });
      main.querySelectorAll(".math").forEach(function (el) {
        var tex = el.dataset.mxTex !== undefined ? el.dataset.mxTex : el.textContent;
        if (fold(tex).text.indexOf(q) !== -1) { el.classList.add("mx-hit-math"); hits.push(el); }
      });
      hits.sort(function (a, b) {
        return a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING ? -1 : 1;
      });
      if (hits.length) go(0);
      update(query);
    }

    function update(query) {
      var has = hits.length > 0;
      prev.disabled = next.disabled = !has;
      if (!query || !query.trim()) count.textContent = "";
      else count.textContent = has ? (current + 1) + " / " + hits.length : count.dataset.none;
      count.classList.toggle("mx-search-empty", !!query && query.trim() !== "" && !has);
    }

    function go(index) {
      if (!hits.length) return;
      if (current >= 0 && hits[current]) hits[current].classList.remove("mx-hit-current");
      current = (index + hits.length) % hits.length;
      var el = hits[current];
      el.classList.add("mx-hit-current");
      el.scrollIntoView({ block: "center", behavior: "auto" });
      update(input.value);
    }

    input.addEventListener("input", function () {
      window.clearTimeout(timer);
      timer = window.setTimeout(function () { timer = null; run(input.value); }, 120);
    });
    input.addEventListener("keydown", function (e) {
      if (e.key === "Enter") {
        e.preventDefault();
        if (timer) { window.clearTimeout(timer); timer = null; run(input.value); return; }
        go(current + (e.shiftKey ? -1 : 1));
      } else if (e.key === "Escape") {
        input.value = "";
        window.clearTimeout(timer);
        timer = null;
        run("");
        input.blur();
      }
    });
    prev.addEventListener("click", function () { go(current - 1); });
    next.addEventListener("click", function () { go(current + 1); });

    /* Ctrl/Cmd+F is left to the browser; Ctrl/Cmd+K and "/" jump to this box. */
    document.addEventListener("keydown", function (e) {
      var typing = /^(INPUT|TEXTAREA|SELECT)$/.test((e.target && e.target.tagName) || "") || (e.target && e.target.isContentEditable);
      if (((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") || (e.key === "/" && !typing && !e.ctrlKey && !e.metaKey && !e.altKey)) {
        e.preventDefault();
        document.documentElement.classList.remove("mx-toc-collapsed");
        var toggle = document.getElementById("mx-toc-toggle");
        if (toggle) toggle.setAttribute("aria-expanded", "true");
        input.focus();
        input.select();
      }
    });
    if (window.Mathia) window.Mathia.search = function (q) { input.value = q; run(q); return hits.length; };
  }

  function setupScrollSpy() {
    if (!("IntersectionObserver" in window)) return;
    var links = {};
    document.querySelectorAll("[data-toc-target]").forEach(function (a) {
      links[a.dataset.tocTarget] = a;
    });
    var current = null;
    var observer = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (entry) {
          var link = links[entry.target.id];
          if (!entry.isIntersecting || !link) return;
          if (current) current.classList.remove("mx-active");
          current = link;
          link.classList.add("mx-active");
        });
      },
      { rootMargin: "0px 0px -70% 0px" }
    );
    Object.keys(links).forEach(function (id) {
      var target = document.getElementById(id);
      if (target) observer.observe(target);
    });
  }

  function describe(el) {
    return {
      id: el.dataset.blockId,
      type: el.dataset.blockType,
      /* Id of the statement a proof proves / the exercise a solution answers. */
      proves: el.dataset.proves || null,
      element: el,
      /* LaTeX sources are restored so consumers get the text as authored. */
      get text() {
        var clone = el.cloneNode(true);
        clone.querySelectorAll(".mx-block-head, .mx-math-copy").forEach(function (n) { n.remove(); });
        clone.querySelectorAll(".math").forEach(function (m) {
          var tex = m.dataset.mxTex !== undefined ? m.dataset.mxTex : m.textContent;
          m.textContent = m.classList.contains("math-display") ? "$$" + tex + "$$" : "$" + tex + "$";
        });
        return clone.textContent.trim();
      },
    };
  }

  var actions = [];
  function mountAction(action) {
    document.querySelectorAll("[data-block-actions]").forEach(function (slot) {
      var el = document.getElementById(slot.dataset.blockActions);
      if (!el || (action.types && action.types.indexOf(el.dataset.blockType) === -1)) return;
      var button = document.createElement("button");
      button.type = "button";
      button.textContent = action.label;
      button.dataset.mxAction = action.id;
      button.addEventListener("click", function () { action.run(describe(el), button); });
      slot.appendChild(button);
    });
  }

  window.Mathia = {
    version: 1,
    blocks: function () {
      return Array.prototype.map.call(document.querySelectorAll("[data-block-id]"), describe);
    },
    block: function (id) {
      var el = document.querySelector('[data-block-id="' + String(id).replace(/"/g, "") + '"]');
      return el ? describe(el) : null;
    },
    /* action: { id, label, run(block), types?: ["theorem", ...] } */
    registerAction: function (action) {
      actions.push(action);
      mountAction(action);
    },
    renderMath: renderMath,
  };

  /* Inside a host frame (the Mathia desktop app), a click on a block tells the
     host which block it was. Only the block id is sent; nothing is received.
     Links, buttons, formulas (which copy themselves) and text selections are
     left alone. */
  function setupHostLink() {
    if (window.parent === window) return;
    document.addEventListener("click", function (e) {
      var target = e.target;
      if (!target || !target.closest || target.closest("a, button, input, textarea, select, .math, dialog")) return;
      var sel = window.getSelection();
      if (sel && String(sel).length) return;
      var block = target.closest("[data-block-id]");
      if (!block) return;
      window.parent.postMessage({ mathia: "select", id: block.dataset.blockId }, "*");
    });
  }

  /* Print button. In a host frame (the desktop app) the sandboxed frame may not
     open a print dialog itself, so it asks the host to; elsewhere it prints. */
  function setupPrint() {
    var button = document.getElementById("mx-print");
    if (!button) return;
    button.hidden = false;
    button.addEventListener("click", function () {
      if (window.parent !== window) window.parent.postMessage({ mathia: "print" }, "*");
      else window.print();
    });
  }

  function start() {
    setupTheme();
    setupPrint();
    setupHostLink();
    renderMath();
    setupCopy();
    setupSidebar();
    setupTocFolding();
    setupSearch();
    setupScrollSpy();
  }
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", start);
  } else {
    start();
  }
})();
