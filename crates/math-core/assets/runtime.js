/* Mathia micro-runtime: math rendering, sidebar, and the block hook API that
   the interactive features (SVG explainers, per-block AI questions) build on. */
(function () {
  "use strict";

  function cleanTex(raw) {
    if (!raw) return "";
    var tex = raw.trim();
    if (tex.startsWith("\\[") && tex.endsWith("\\]")) {
      tex = tex.slice(2, -2).trim();
    } else if (tex.startsWith("\\(") && tex.endsWith("\\)")) {
      tex = tex.slice(2, -2).trim();
    } else if (tex.startsWith("$$") && tex.endsWith("$$") && tex.length >= 4) {
      tex = tex.slice(2, -2).trim();
    } else if (tex.startsWith("$") && tex.endsWith("$") && tex.length >= 2) {
      tex = tex.slice(1, -1).trim();
    }
    return tex;
  }

  function autoRenderDelimiters(root) {
    if (!root) return;
    var SKIP_TAGS = { SCRIPT: 1, STYLE: 1, PRE: 1, CODE: 1, TEXTAREA: 1, KBD: 1, SAMP: 1, SVG: 1 };
    var walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, null, false);
    var textNodes = [];
    var node;
    while ((node = walker.nextNode())) {
      var parent = node.parentElement;
      if (!parent) continue;
      if (SKIP_TAGS[parent.tagName.toUpperCase()]) continue;
      if (parent.closest(".math, .katex, [data-mx-tex]")) continue;
      var val = node.nodeValue;
      if (val && (val.indexOf("$") !== -1 || val.indexOf("\\[") !== -1 || val.indexOf("\\(") !== -1 || val.indexOf("\\begin{") !== -1)) {
        textNodes.push(node);
      }
    }

    textNodes.forEach(function (tNode) {
      var text = tNode.nodeValue;
      if (!text) return;
      var parent = tNode.parentNode;
      if (!parent) return;

      var frag = document.createDocumentFragment();
      var len = text.length;
      var i = 0;
      var plainStart = 0;

      function flushPlain(end) {
        if (end > plainStart) {
          frag.appendChild(document.createTextNode(text.slice(plainStart, end)));
        }
      }

      while (i < len) {
        if (text.charAt(i) === "\\") {
          var next = text.charAt(i + 1);
          if (next === "[") {
            var closeB = text.indexOf("\\]", i + 2);
            if (closeB !== -1) {
              flushPlain(i);
              var div = document.createElement("div");
              div.className = "math math-display";
              div.textContent = text.slice(i + 2, closeB);
              frag.appendChild(div);
              i = closeB + 2;
              plainStart = i;
              continue;
            }
          } else if (next === "(") {
            var closeP = text.indexOf("\\)", i + 2);
            if (closeP !== -1) {
              flushPlain(i);
              var span = document.createElement("span");
              span.className = "math math-inline";
              span.textContent = text.slice(i + 2, closeP);
              frag.appendChild(span);
              i = closeP + 2;
              plainStart = i;
              continue;
            }
          } else if (text.slice(i, i + 7) === "\\begin{") {
            var endBrace = text.indexOf("}", i + 7);
            if (endBrace !== -1) {
              var envName = text.slice(i + 7, endBrace);
              var endTag = "\\end{" + envName + "}";
              var closeEnv = text.indexOf(endTag, endBrace + 1);
              if (closeEnv !== -1) {
                flushPlain(i);
                var divEnv = document.createElement("div");
                divEnv.className = "math math-display";
                divEnv.textContent = text.slice(i, closeEnv + endTag.length);
                frag.appendChild(divEnv);
                i = closeEnv + endTag.length;
                plainStart = i;
                continue;
              }
            }
          }
          i += 2;
          continue;
        }

        if (text.charAt(i) === "$" && text.charAt(i + 1) === "$") {
          var closeDd = text.indexOf("$$", i + 2);
          if (closeDd !== -1) {
            flushPlain(i);
            var mDiv = document.createElement("div");
            mDiv.className = "math math-display";
            mDiv.textContent = text.slice(i + 2, closeDd);
            frag.appendChild(mDiv);
            i = closeDd + 2;
            plainStart = i;
            continue;
          }
        }

        if (text.charAt(i) === "$") {
          var nextChar = text.charAt(i + 1);
          if (nextChar && nextChar !== " " && nextChar !== "\t" && nextChar !== "\n" && nextChar !== "\r" && nextChar !== "$") {
            var closeD = -1;
            for (var k = i + 1; k < len; k++) {
              var ch = text.charAt(k);
              if (ch === "\n") break;
              if (ch === "$" && text.charAt(k - 1) !== "\\") {
                if (text.charAt(k - 1) !== " " && text.charAt(k - 1) !== "\t") {
                  closeD = k;
                }
                break;
              }
            }
            if (closeD !== -1 && closeD > i + 1) {
              flushPlain(i);
              var mSpan = document.createElement("span");
              mSpan.className = "math math-inline";
              mSpan.textContent = text.slice(i + 1, closeD);
              frag.appendChild(mSpan);
              i = closeD + 1;
              plainStart = i;
              continue;
            }
          }
        }

        i++;
      }

      if (plainStart === 0 && frag.childNodes.length === 0) {
        return;
      }
      flushPlain(len);
      parent.replaceChild(frag, tNode);
    });
  }

  function renderMathIn(container) {
    if (!window.katex || !container) return;
    autoRenderDelimiters(container);
    container.querySelectorAll(".math, .arithmatex, .latex, .tex").forEach(function (el) {
      if (el.dataset.mxTex !== undefined) return;
      var raw = el.textContent;
      var tex = cleanTex(raw);
      el.dataset.mxTex = tex;
      var isDisplay = el.classList.contains("math-display") ||
                      el.classList.contains("display") ||
                      el.tagName === "DIV";
      if (isDisplay) {
        el.classList.add("math-display");
      } else {
        el.classList.add("math-inline");
      }
      el.classList.add("math");
      try {
        window.katex.render(tex, el, {
          displayMode: isDisplay,
          throwOnError: false,
        });
      } catch (err) {
        el.classList.add("mx-math-error");
        el.title = String(err);
      }
    });
  }

  function renderMath() {
    if (!window.katex) return;
    renderMathIn(document.getElementById("mx-main") || document.body);
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
      if (document.body.classList.contains("mx-editing")) return;
      var m = e.target.closest && e.target.closest(".math");
      if (!m || m.classList.contains("math-display") || e.target.closest("button")) return;
      var sel = window.getSelection();
      if (sel && String(sel).length) return;
      copyText(texOf(m)).then(function () { flash(m); });
    });
    /* Ctrl/Cmd+C over a selection yields LaTeX for formulas, not KaTeX's MathML. */
    document.addEventListener("copy", function (e) {
      var targetTag = (e.target && e.target.tagName) || "";
      if (/^(INPUT|TEXTAREA|SELECT)$/.test(targetTag)) return;
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
    document.querySelectorAll("#mx-toc .mx-toc-fold").forEach(function (btn) {
      btn.remove();
    });
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

  /* Keep native ranges for bidi selection and foreground color. Paint prose
     backgrounds by visual line so that inline math and text share a height. */
  function highlightBackgrounds(main) {
    var prose = "p, li, dt, dd, td, th, blockquote, h1, h2, h3, h4, h5, h6, .mx-block-title, .mx-block-body";
    var native = "code, pre, .math-display, mark.mx-hit, .mx-hit-math";
    var skip = ".katex-mathml, script, style, svg, button, .mx-actions";
    var painted = new Set();

    function eachText(root, range, visit) {
      if (root.nodeType === 3) { visit(root); return; }
      var walker = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT | NodeFilter.SHOW_TEXT, {
        acceptNode: function (node) {
          if (range && !range.intersectsNode(node)) return NodeFilter.FILTER_REJECT;
          if (node.nodeType === 3) return NodeFilter.FILTER_ACCEPT;
          return node.matches(skip) ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_SKIP;
        },
      });
      for (var node = walker.nextNode(); node; node = walker.nextNode()) visit(node);
    }

    function paint(block, selections) {
      var measure = document.createRange();
      var fragments = [];
      var selected = [];
      var mathRects = new Map();
      var seenMath = new Set();

      function textRects(node, start, end) {
        measure.setStart(node, start);
        measure.setEnd(node, end);
        return Array.from(measure.getClientRects()).filter(function (r) { return r.width > 0 && r.height > 0; });
      }

      eachText(block, null, function (node) {
        var el = node.parentElement;
        if (el.closest(prose) !== block || el.closest(".math-display, pre")) return;
        var length = node.length;
        var math = el.closest(".math-inline");
        var mathRect = null;
        if (math) {
          if (!mathRects.has(math)) {
            var rect = math.getBoundingClientRect();
            mathRects.set(math, rect);
            if (rect.width && rect.height) fragments.push(rect);
          }
          mathRect = mathRects.get(math);
        } else {
          fragments.push.apply(fragments, textRects(node, 0, length));
        }
        if (el.closest(native)) return;
        selections.forEach(function (range) {
          if (!range.intersectsNode(node)) return;
          if (math && range.comparePoint(math, 0) === 0 &&
              range.comparePoint(math, math.childNodes.length) === 0) {
            if (!seenMath.has(math)) selected.push({ rect: mathRect, anchor: mathRect });
            seenMath.add(math);
            return;
          }
          var start = range.startContainer === node ? range.startOffset : 0;
          var end = range.endContainer === node ? range.endOffset : length;
          textRects(node, start, end).forEach(function (rect) {
            selected.push({ rect: rect, anchor: mathRect || rect });
          });
        });
      });
      if (!selected.length) return null;

      var lines = [];
      function center(rect) { return (rect.top + rect.bottom) / 2; }
      function nearest(rect) {
        var match = null;
        var distance = Infinity;
        lines.forEach(function (line) {
          var delta = Math.abs(line.center - center(rect));
          if (delta < distance) { distance = delta; match = line; }
        });
        return match;
      }
      // Start with the shorter fragments so tall formulas can extend a line
      // without shifting the reference used to assign the other fragments.
      fragments.sort(function (a, b) { return a.height - b.height; }).forEach(function (rect) {
        var line = nearest(rect);
        if (!line || Math.abs(line.center - center(rect)) >= Math.max(line.height, rect.height) / 2) {
          lines.push({ center: center(rect), height: rect.height, top: rect.top, bottom: rect.bottom, spans: [] });
        } else {
          line.top = Math.min(line.top, rect.top);
          line.bottom = Math.max(line.bottom, rect.bottom);
        }
      });
      if (!lines.length) return null;

      var style = window.getComputedStyle(block);
      var height = parseFloat(style.lineHeight) || 0;
      lines.forEach(function (line) { height = Math.max(height, line.bottom - line.top); });
      selected.forEach(function (part) {
        if (part.rect.width) nearest(part.anchor).spans.push(part.rect);
      });
      var origin = block.getBoundingClientRect();
      var leftEdge = origin.left + parseFloat(style.borderLeftWidth);
      var topEdge = origin.top + parseFloat(style.borderTopWidth) - height;
      var bands = [];
      function px(n) { return n.toFixed(3) + "px"; }
      lines.forEach(function (line) {
        var top = (line.top + line.bottom - height) / 2 - topEdge;
        var left, right;
        function addLayer() {
          bands.push("linear-gradient(var(--mx-highlight-bg), var(--mx-highlight-bg)) " +
            px(left - leftEdge) + " " + px(top) + " / " + px(right - left) + " " + px(height) + " no-repeat");
        }
        line.spans.sort(function (a, b) { return a.left - b.left; }).forEach(function (span) {
          if (right !== undefined && span.left <= right + 0.5) {
            right = Math.max(right, span.right);
          } else {
            if (right !== undefined) addLayer();
            left = span.left;
            right = span.right;
          }
        });
        if (right !== undefined) addLayer();
      });
      return bands.length ? { background: bands.join(", "), inset: px(-height) } : null;
    }

    function clear() {
      painted.forEach(function (block) {
        block.removeAttribute("data-mx-highlight-painted");
        block.style.removeProperty("--mx-highlight-bands");
        block.style.removeProperty("--mx-highlight-inset");
        if (block.getAttribute("style") === "") block.removeAttribute("style");
      });
      painted.clear();
    }

    return {
      clear: clear,
      update: function (ranges) {
        var blocks = new Map();
        ranges.forEach(function (range) {
          var touched = new Set();
          eachText(range.commonAncestorContainer, range, function (node) {
            var el = node.parentElement;
            var block = el.closest(prose);
            if (block && main.contains(block) && !el.closest(native + ", " + skip)) touched.add(block);
          });
          touched.forEach(function (block) {
            if (!blocks.has(block)) blocks.set(block, []);
            blocks.get(block).push(range);
          });
        });
        var bands = new Map();
        blocks.forEach(function (selections, block) {
          var result = paint(block, selections);
          if (result) bands.set(block, result);
        });
        clear();
        bands.forEach(function (result, block) {
          block.style.setProperty("--mx-highlight-bands", result.background);
          block.style.setProperty("--mx-highlight-inset", result.inset);
          block.setAttribute("data-mx-highlight-painted", "");
          painted.add(block);
        });
      },
    };
  }

  function setupHighlights() {
    var main = document.getElementById("mx-main");
    var supported = !!(window.CSS && window.CSS.highlights && typeof window.Highlight === "function");
    var NAME = "mathia-highlight";
    var ranges = [];
    var highlight = supported ? new window.Highlight() : null;
    var registered = false;
    var pending = null;
    var toolbar = null;
    var button = null;
    var expandButton = null;
    var undoBar = null;
    var undoButton = null;
    var redoButton = null;
    /* Highlight states for undo/redo, oldest first. */
    var history = [];
    var future = [];
    var held = false;
    /* Inside the desktop app the host owns the undo order, so the frame only
       replays what it is told to. */
    var framed = window.parent !== window;
    var backgrounds = main && supported ? highlightBackgrounds(main) : null;
    var resizeObserver = null;
    var mutationObserver = null;
    var frame = 0;

    function repaintSoon() {
      if (!ranges.length || frame) return;
      frame = window.requestAnimationFrame(function () {
        frame = 0;
        backgrounds.update(ranges);
      });
    }

    function onResize() {
      hide();
      repaintSoon();
    }

    function hide() {
      if (toolbar) toolbar.hidden = true;
    }

    function reset() {
      if (registered && window.CSS && window.CSS.highlights) {
        try { window.CSS.highlights.delete(NAME); } catch (err) { /* already gone */ }
      }
      registered = false;
      ranges = [];
      if (frame) window.cancelAnimationFrame(frame);
      frame = 0;
      if (backgrounds) backgrounds.clear();
      if (supported) highlight = new window.Highlight();
      pending = null;
      hide();
    }

    function snapshot() {
      return ranges.slice();
    }

    function clear() {
      if (ranges.length) {
        history.push(snapshot());
        future = [];
      }
      reset();
      updateUndo();
    }

    function restore(list) {
      reset();
      list.forEach(function (range) {
        try {
          highlight.add(range);
          ranges.push(range);
        } catch (err) { /* the range no longer fits the document */ }
      });
      if (ranges.length && supported) {
        window.CSS.highlights.set(NAME, highlight);
        registered = true;
      }
      repaintSoon();
      updateUndo();
    }

    function undo() {
      if (!history.length) return;
      future.push(snapshot());
      restore(history.pop());
    }

    function redo() {
      if (!future.length) return;
      history.push(snapshot());
      restore(future.pop());
    }

    function updateUndo() {
      if (!undoBar) return;
      undoBar.hidden = !history.length && !future.length && !ranges.length;
      undoButton.disabled = !history.length;
      redoButton.disabled = !future.length;
    }

    function destroy() {
      clear();
      if (framed) window.removeEventListener("message", onHostMessage);
      if (toolbar) {
        document.removeEventListener("selectionchange", onSelectionChange);
        document.removeEventListener("keydown", onKeyDown, true);
        window.removeEventListener("scroll", hide, true);
        window.removeEventListener("resize", onResize);
        if (resizeObserver) resizeObserver.disconnect();
        if (mutationObserver) mutationObserver.disconnect();
        if (document.fonts) document.fonts.removeEventListener("loadingdone", repaintSoon);
        window.removeEventListener("pagehide", onPageHide);
        window.removeEventListener("beforeunload", destroy);
        if (toolbar.parentNode) toolbar.parentNode.removeChild(toolbar);
        toolbar = null;
        button = null;
        expandButton = null;
      }
      if (undoBar) {
        undoButton.removeEventListener("click", undo);
        redoButton.removeEventListener("click", redo);
        if (undoBar.parentNode) undoBar.parentNode.removeChild(undoBar);
        undoBar = null;
        undoButton = null;
        redoButton = null;
      }
    }

    function onPageHide(e) {
      if (e.persisted) return;
      destroy();
    }

    var api = {
      supported: supported,
      count: function () { return ranges.length; },
      clear: clear,
      undo: undo,
      redo: redo,
      destroy: destroy,
    };
    if (window.Mathia) window.Mathia.highlights = api;
    if (!main || !supported) return;

    var LANG_FA = document.documentElement.lang === "fa";
    var label = LANG_FA ? "برجسته‌سازی انتخاب" : "Highlight selection";
    var hint = LANG_FA ? "متن انتخاب‌شده را برجسته نگه می‌دارد" : "Keep the selected text highlighted";
    var expandLabel = LANG_FA ? "گسترش بلوک" : "Expand block";
    var expandHint = LANG_FA
      ? "بلوک آخر در انتخاب را تا این متن بزرگ می‌کند"
      : "Grow the last block in the selection to cover this text";

    toolbar = document.createElement("div");
    toolbar.id = "mx-highlight-toolbar";
    toolbar.className = "mx-highlight-toolbar";
    toolbar.setAttribute("role", "toolbar");
    toolbar.setAttribute("aria-label", label);
    toolbar.hidden = true;

    function addTool(action, text, title, shortcut) {
      var el = document.createElement("button");
      el.type = "button";
      el.id = "mx-" + action + "-button";
      el.dataset.mxAction = action;
      el.textContent = text;
      el.title = title;
      el.setAttribute("aria-label", text);
      if (shortcut) el.setAttribute("aria-keyshortcuts", shortcut);
      toolbar.appendChild(el);
      return el;
    }

    button = addTool("highlight", label, hint, "Control+Shift+H");
    /* Growing a block is a correction of the source, which only the Mathia
       app can apply; a standalone page offers the highlight alone. */
    if (framed) {
      expandButton = addTool("expand", expandLabel, expandHint);
    }
    document.body.appendChild(toolbar);

    /* A page opened on its own, as on a phone, has no toolbar of its own, so
       the highlight history gets floating buttons. */
    if (!framed) {
      undoBar = document.createElement("div");
      undoBar.id = "mx-highlight-history";
      undoBar.className = "mx-highlight-history";
      undoBar.setAttribute("role", "group");
      undoBar.setAttribute("aria-label", LANG_FA ? "واگرد برجسته‌سازی" : "Highlight history");
      undoBar.hidden = true;

      undoButton = document.createElement("button");
      undoButton.type = "button";
      undoButton.id = "mx-highlight-undo";
      undoButton.textContent = "↶";
      undoButton.title = LANG_FA ? "واگرد برجسته‌سازی (Ctrl+Z)" : "Undo highlight (Ctrl+Z)";
      undoButton.setAttribute("aria-label", LANG_FA ? "واگرد" : "Undo");
      undoButton.disabled = true;

      redoButton = document.createElement("button");
      redoButton.type = "button";
      redoButton.id = "mx-highlight-redo";
      redoButton.textContent = "↷";
      redoButton.title = LANG_FA ? "ازنو (Ctrl+Shift+Z)" : "Redo highlight (Ctrl+Shift+Z)";
      redoButton.setAttribute("aria-label", LANG_FA ? "ازنو" : "Redo");
      redoButton.disabled = true;

      undoBar.appendChild(undoButton);
      undoBar.appendChild(redoButton);
      document.body.appendChild(undoBar);
      undoButton.addEventListener("click", undo);
      redoButton.addEventListener("click", redo);
    }

    function elementOf(node) {
      return node && node.nodeType === 1 ? node : node && node.parentElement;
    }

    function isUi(node) {
      var el = elementOf(node);
      return !!(el && el.closest && el.closest(
        "button, input, textarea, select, dialog, .mx-actions, .mx-math-copy, " +
        ".mx-ai-panel, script, style, svg"
      ));
    }

    function inside(node) {
      return !!node && (node === main || main.contains(node));
    }

    function usable(range) {
      if (!range || range.collapsed) return false;
      if (!inside(range.startContainer) || !inside(range.endContainer)) return false;
      if (!range.startContainer.isConnected || !range.endContainer.isConnected) return false;
      if (isUi(range.startContainer) || isUi(range.endContainer)) return false;
      var text = range.toString();
      if (!text || !text.trim()) return false;
      var rect = range.getBoundingClientRect();
      return !!rect && (rect.width > 0 || rect.height > 0);
    }

    function show(range) {
      var rect = range.getBoundingClientRect();
      if (!rect) { hide(); return; }
      toolbar.hidden = false;
      var width = toolbar.offsetWidth || 160;
      var height = toolbar.offsetHeight || 32;
      var left = rect.left + (rect.width - width) / 2;
      left = Math.max(8, Math.min(left, window.innerWidth - width - 8));
      var top = rect.top - height - 8;
      if (top < 8) top = rect.bottom + 8;
      top = Math.max(8, Math.min(top, window.innerHeight - height - 8));
      toolbar.style.left = left + "px";
      toolbar.style.top = top + "px";
    }

    function apply() {
      var range = pending;
      var sel = window.getSelection();
      if (!range && sel && sel.rangeCount && !sel.isCollapsed) {
        range = sel.getRangeAt(0).cloneRange();
      }
      if (!usable(range)) { hide(); return false; }
      var before = snapshot();
      try {
        highlight.add(range);
        if (!registered) {
          window.CSS.highlights.set(NAME, highlight);
          registered = true;
        }
        ranges.push(range);
      } catch (err) {
        return false;
      }
      history.push(before);
      future = [];
      repaintSoon();
      pending = null;
      if (sel) sel.removeAllRanges();
      hide();
      updateUndo();
      if (framed) window.parent.postMessage({ mathia: "highlight-applied" }, "*");
      return true;
    }

    /* The last block the selection touches. A selection often runs past the
       end of a block into plain text; the block to grow is the one before it. */
    function lastBlock(node) {
      var el = elementOf(node);
      var block = el && el.closest ? el.closest("[data-block-id]") : null;
      var all = document.querySelectorAll("[data-block-id]");
      while (block) {
        var type = block.dataset.blockType;
        if (type && type !== "text" && type !== "heading") return block;
        var index = Array.prototype.indexOf.call(all, block);
        block = index > 0 ? all[index - 1] : null;
      }
      return null;
    }

    /* The element the compiled document made for one node of the source: a
       paragraph, a list item, a heading, a block quote. */
    function endElement(node) {
      var el = elementOf(node);
      while (el && el.parentElement) {
        if (el.tagName === "LI") return el;
        var parent = el.parentElement;
        if (parent.id === "mx-main" ||
            (parent.classList && parent.classList.contains("mx-block-body"))) return el;
        el = parent;
      }
      return null;
    }

    /* The node's prose without the rendered formulas, so the host can match it
       against the source nodes. */
    function proseOf(el) {
      if (!el) return "";
      var clone = el.cloneNode(true);
      clone.querySelectorAll(".mx-block-head, .mx-math-copy, .mx-actions, .math").forEach(function (n) {
        n.remove();
      });
      return clone.textContent.replace(/\s+/g, " ").trim();
    }

    /* Asks the host app to grow the last block of the selection. */
    function expand() {
      var range = pending;
      var sel = window.getSelection();
      if (!range && sel && sel.rangeCount && !sel.isCollapsed) {
        range = sel.getRangeAt(0).cloneRange();
      }
      if (!range) { hide(); return; }
      var block = lastBlock(range.endContainer) || lastBlock(range.startContainer);
      var last = endElement(range.endContainer);
      var owner = last && last.closest ? last.closest("[data-block-id]") : null;
      var text = proseOf(last);
      pending = null;
      if (sel) sel.removeAllRanges();
      hide();
      if (!block) return;
      window.parent.postMessage(
        { mathia: "expand", id: block.dataset.blockId, endId: owner ? owner.dataset.blockId : null, text: text },
        "*"
      );
    }

    function onSelectionChange() {
      if (held || document.body.classList.contains("mx-editing")) {
        pending = null;
        hide();
        return;
      }
      var sel = window.getSelection();
      if (!sel || !sel.rangeCount || sel.isCollapsed) { pending = null; hide(); return; }
      var range = sel.getRangeAt(0);
      if (!usable(range)) { pending = null; hide(); return; }
      pending = range.cloneRange();
      show(pending);
    }

    /* The desktop app decides what undo means across corrections and
       highlights, then asks the frame to replay its part. */
    function onHostMessage(e) {
      if (e.source !== window.parent || !e.data) return;
      if (e.data.mathia === "highlight-undo") undo();
      else if (e.data.mathia === "highlight-redo") redo();
    }

    function onKeyDown(e) {
      if (e.repeat) return;
      var typing = /^(INPUT|TEXTAREA|SELECT)$/.test((e.target && e.target.tagName) || "") ||
        (e.target && e.target.isContentEditable);
      var key = e.key ? e.key.toLowerCase() : "";
      if ((e.ctrlKey || e.metaKey) && !e.altKey && (key === "z" || key === "y") && !typing) {
        e.preventDefault();
        var action = key === "y" || e.shiftKey ? "redo" : "undo";
        if (framed) window.parent.postMessage({ mathia: action }, "*");
        else if (action === "redo") redo();
        else undo();
        return;
      }
      if (!(e.code === "KeyH" || key === "h")) return;
      if (!(e.ctrlKey || e.metaKey) || !e.shiftKey || e.altKey) return;
      var sel = window.getSelection();
      if (!sel || !sel.rangeCount || sel.isCollapsed) return;
      var range = sel.getRangeAt(0);
      if (!usable(range)) return;
      e.preventDefault();
      pending = range.cloneRange();
      apply();
    }

    function releaseSoon(delay) {
      window.setTimeout(function () { held = false; }, delay);
    }

    /* Capture the range before a toolbar interaction can clear the native
       selection. mousedown.preventDefault keeps the selection for mouse
       users; touch users rely on the range captured on selectionchange. */
    toolbar.addEventListener("mousedown", function (e) {
      held = true;
      e.preventDefault();
    });
    toolbar.addEventListener("touchstart", function () { held = true; }, { passive: true });
    toolbar.addEventListener("pointerdown", function () { held = true; });
    toolbar.addEventListener("pointerup", function (e) {
      if (e.pointerType !== "touch") releaseSoon(0);
    });
    toolbar.addEventListener("touchend", function () { releaseSoon(400); });
    toolbar.addEventListener("click", function (e) {
      var target = e.target && e.target.closest ? e.target.closest("button[data-mx-action]") : null;
      if (!target) return;
      e.preventDefault();
      if (target.dataset.mxAction === "expand") expand();
      else apply();
    });

    document.addEventListener("selectionchange", onSelectionChange);
    document.addEventListener("keydown", onKeyDown, true);
    if (framed) window.addEventListener("message", onHostMessage);
    window.addEventListener("scroll", hide, true);
    window.addEventListener("resize", onResize);
    if ("ResizeObserver" in window) {
      resizeObserver = new ResizeObserver(repaintSoon);
      resizeObserver.observe(main);
    }
    if ("MutationObserver" in window) {
      mutationObserver = new MutationObserver(repaintSoon);
      mutationObserver.observe(main, { subtree: true, childList: true, characterData: true });
    }
    if (document.fonts) document.fonts.addEventListener("loadingdone", repaintSoon);
    window.addEventListener("pagehide", onPageHide);
    window.addEventListener("beforeunload", destroy);
  }

  var scrollObserver = null;
  function setupScrollSpy() {
    if (!("IntersectionObserver" in window)) return;
    if (scrollObserver) {
      scrollObserver.disconnect();
      scrollObserver = null;
    }
    var links = {};
    document.querySelectorAll("[data-toc-target]").forEach(function (a) {
      links[a.dataset.tocTarget] = a;
    });
    var current = null;
    scrollObserver = new IntersectionObserver(
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
      if (target) scrollObserver.observe(target);
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

  function remountActions() {
    document.querySelectorAll("[data-block-actions]").forEach(function (slot) {
      slot.textContent = "";
    });
    actions.forEach(mountAction);
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
    remountActions: remountActions,
    refreshTocFolding: setupTocFolding,
    refreshScrollSpy: setupScrollSpy,
    renderMathIn: renderMathIn,
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
    setupHighlights();
    setupScrollSpy();
  }
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", start);
  } else {
    start();
  }
})();
