/* Mathia Obsidian-style Live Preview editor & standalone browser persistence.
   Runs offline inside both the desktop preview frame and standalone exported HTML.
   Builds all DOM nodes explicitly (never innerHTML on user text) to uphold CSP. */
(function () {
  "use strict";

  var SEMANTIC_KEYWORDS = [
    ["theorem", "Theorem", "قضیه", ["theorem", "thm", "قضیه"]],
    ["lemma", "Lemma", "لم", ["lemma", "lem", "لم"]],
    ["proposition", "Proposition", "گزاره", ["proposition", "prop", "گزاره"]],
    ["corollary", "Corollary", "نتیجه", ["corollary", "cor", "نتیجه"]],
    ["definition", "Definition", "تعریف", ["definition", "defn", "def", "تعریف"]],
    ["proof", "Proof", "برهان", ["proof", "برهان", "اثبات"]],
    ["example", "Example", "مثال", ["example", "exmp", "ex", "مثال"]],
    ["remark", "Remark", "تذکر", ["remark", "rem", "note", "تذکر", "نکته", "توجه"]],
    ["exercise", "Exercise", "تمرین", ["exercise", "problem", "تمرین", "مسئله", "مساله"]],
    ["solution", "Solution", "حل", ["solution", "sol", "حل", "پاسخ"]],
    ["claim", "Claim", "ادعا", ["claim", "ادعا"]],
    ["notation", "Notation", "نمادگذاری", ["notation", "نماد", "نمادگذاری"]]
  ];

  var MATH_ENVS = {
    equation: 1, align: 1, alignat: 1, gather: 1, multline: 1, flalign: 1,
    matrix: 1, pmatrix: 1, bmatrix: 1, Bmatrix: 1, vmatrix: 1, Vmatrix: 1,
    cases: 1, dcases: 1, rcases: 1, drcases: 1, split: 1, array: 1
  };

  function safeUrl(url, isImage) {
    if (!url || typeof url !== "string") return isImage ? "" : "#";
    var compact = url.replace(/[\s\u0000-\u001f\u007f]/g, "");
    var colon = compact.indexOf(":");
    if (colon === -1) return url;
    var scheme = compact.slice(0, colon);
    if (!/^[a-zA-Z][a-zA-Z0-9+.-]*$/.test(scheme)) return url;
    var lower = scheme.toLowerCase();
    if (lower === "http" || lower === "https") return url;
    if (lower === "mailto" && !isImage) return url;
    if (lower === "data" && isImage && /^data:image\//i.test(compact)) return url;
    return isImage ? "" : "#";
  }

  function detectDir(text) {
    if (!text) return null;
    for (var i = 0; i < text.length; i++) {
      var code = text.charCodeAt(i);
      if ((code >= 0x0590 && code <= 0x08ff) || (code >= 0xfb1d && code <= 0xfdff) || (code >= 0xfe70 && code <= 0xfeff)) {
        return "rtl";
      }
      if ((code >= 65 && code <= 90) || (code >= 97 && code <= 122)) {
        return "ltr";
      }
    }
    return null;
  }

  function cloneData(v) {
    return JSON.parse(JSON.stringify(v));
  }

  function isSameBlocks(a, b) {
    if (!a || !b || a.length !== b.length) return false;
    for (var i = 0; i < a.length; i++) {
      if (a[i].md !== b[i].md || a[i].id !== b[i].id || a[i].kind !== b[i].kind) return false;
    }
    return true;
  }

  /* ── Markdown unit splitter & joiner ───────────────────────────────────── */

  function isHrLine(line) {
    var t = line.trim();
    if (t.length < 3) return false;
    return /^(-{3,}|\*{3,}|_{3,})$/.test(t.replace(/\s+/g, ""));
  }

  function matchListStart(line) {
    var m = line.match(/^(\s{0,3})([-*+]|\d{1,4}[.)])\s+(.*)$/);
    if (!m) return null;
    var marker = m[2];
    return {
      ordered: /\d/.test(marker.charAt(0)),
      marker: marker,
      text: m[3]
    };
  }

  function isTableRow(line) {
    var t = line.trim();
    return t.indexOf("|") !== -1 && (t.charAt(0) === "|" || /\S\s*\|\s*\S/.test(t));
  }

  function isTableSep(line) {
    var t = line.trim().replace(/^\|/, "").replace(/\|$/, "");
    if (!t || line.indexOf("-") === -1) return false;
    var cells = t.split("|");
    for (var i = 0; i < cells.length; i++) {
      if (!/^\s*:?-{2,}:?\s*$/.test(cells[i])) return false;
    }
    return cells.length > 0;
  }

  function splitMarkdownUnits(md) {
    var src = String(md || "").replace(/\r\n/g, "\n").replace(/\r/g, "\n");
    var trimmedSrc = src.trim();
    if (!trimmedSrc) return [];

    // Keep LaTeX theorem-like environments as a cohesive editable unit.
    var envMatch = trimmedSrc.match(/^\\begin\{([a-zA-Z*]+)\}(?:\[([^\]]*)\])?([\s\S]*?)\\end\{\1\}$/);
    if (envMatch && !MATH_ENVS[envMatch[1].replace(/\*$/, "")]) {
      return [{ kind: "env", text: trimmedSrc, envName: envMatch[1], envTitle: envMatch[2] || "", body: envMatch[3].trim() }];
    }

    var lines = src.split("\n");
    var units = [];
    var i = 0;
    var n = lines.length;

    while (i < n) {
      var line = lines[i];
      if (!line.trim()) { i++; continue; }

      // Fenced code block (``` or ~~~)
      var fenceMatch = line.match(/^\s{0,3}(`{3,}|~{3,})(.*)$/);
      if (fenceMatch) {
        var fenceChar = fenceMatch[1].charAt(0);
        var fenceLen = fenceMatch[1].length;
        var closeRe = new RegExp("^\\s{0,3}" + fenceChar + "{" + fenceLen + ",}\\s*$");
        var codeBuf = [line];
        i++;
        while (i < n) {
          codeBuf.push(lines[i]);
          if (closeRe.test(lines[i])) { i++; break; }
          i++;
        }
        units.push({ kind: "fence", text: codeBuf.join("\n") });
        continue;
      }

      // Display math: $$...$$
      var t = line.trim();
      if (t.indexOf("$$") === 0) {
        if (t.length > 4 && t.slice(-2) === "$$") {
          units.push({ kind: "math", text: line });
          i++;
          continue;
        }
        var mBuf = [line];
        i++;
        while (i < n) {
          mBuf.push(lines[i]);
          if (lines[i].trim().indexOf("$$") !== -1) { i++; break; }
          i++;
        }
        units.push({ kind: "math", text: mBuf.join("\n") });
        continue;
      }

      // Display math: \[...\]
      if (t.indexOf("\\[") === 0) {
        if (t.length > 4 && t.slice(-2) === "\\]") {
          units.push({ kind: "math", text: line });
          i++;
          continue;
        }
        var bBuf = [line];
        i++;
        while (i < n) {
          bBuf.push(lines[i]);
          if (lines[i].trim().indexOf("\\]") !== -1) { i++; break; }
          i++;
        }
        units.push({ kind: "math", text: bBuf.join("\n") });
        continue;
      }

      // Math environment: \begin{align}...\end{align}
      var mEnv = t.match(/^\\begin\{([a-zA-Z*]+)\}/);
      if (mEnv && MATH_ENVS[mEnv[1].replace(/\*$/, "")]) {
        var endTag = "\\end{" + mEnv[1] + "}";
        var eBuf = [line];
        if (t.indexOf(endTag) !== -1 && t.indexOf(endTag) > 0) {
          units.push({ kind: "math", text: line });
          i++;
          continue;
        }
        i++;
        while (i < n) {
          eBuf.push(lines[i]);
          if (lines[i].indexOf(endTag) !== -1) { i++; break; }
          i++;
        }
        units.push({ kind: "math", text: eBuf.join("\n") });
        continue;
      }

      // Heading
      var hMatch = line.match(/^\s{0,3}(#{1,6})(?:\s+(.*))?$/);
      if (hMatch) {
        units.push({ kind: "heading", headingLevel: hMatch[1].length, text: line });
        i++;
        continue;
      }

      // Horizontal rule
      if (isHrLine(line)) {
        units.push({ kind: "hr", text: line });
        i++;
        continue;
      }

      // Table
      if (i + 1 < n && isTableRow(line) && isTableSep(lines[i + 1])) {
        var tblBuf = [line, lines[i + 1]];
        i += 2;
        while (i < n && lines[i].trim() && isTableRow(lines[i])) {
          tblBuf.push(lines[i]);
          i++;
        }
        units.push({ kind: "table", text: tblBuf.join("\n") });
        continue;
      }

      // Blockquote
      if (/^\s{0,3}>/.test(line)) {
        var qBuf = [];
        while (i < n && /^\s{0,3}>/.test(lines[i])) {
          qBuf.push(lines[i]);
          i++;
        }
        units.push({ kind: "blockquote", text: qBuf.join("\n") });
        continue;
      }

      // List item
      var li = matchListStart(line);
      if (li) {
        var liBuf = [line];
        i++;
        while (i < n) {
          var nextLine = lines[i];
          if (!nextLine.trim()) break;
          if (matchListStart(nextLine)) break;
          if (/^\s{2,}/.test(nextLine) || !/^\s{0,3}(#{1,6}\s|>|```|~~~|\$\$|\\\[)/.test(nextLine)) {
            if (!/^\s{2,}/.test(nextLine) && isHrLine(nextLine)) break;
            liBuf.push(nextLine);
            i++;
          } else {
            break;
          }
        }
        units.push({ kind: "list-item", ordered: li.ordered, text: liBuf.join("\n") });
        continue;
      }

      // Paragraph
      var pBuf = [];
      while (i < n) {
        var cur = lines[i];
        if (!cur.trim()) break;
        if (
          /^\s{0,3}(`{3,}|~{3,})/.test(cur) ||
          /^\s{0,3}#{1,6}(\s|$)/.test(cur) ||
          /^\s{0,3}>/.test(cur) ||
          isHrLine(cur) ||
          matchListStart(cur) ||
          cur.trim().indexOf("$$") === 0 ||
          cur.trim().indexOf("\\[") === 0 ||
          (i + 1 < n && isTableRow(cur) && isTableSep(lines[i + 1]))
        ) {
          break;
        }
        pBuf.push(cur);
        i++;
      }
      if (pBuf.length) {
        units.push({ kind: "paragraph", text: pBuf.join("\n") });
      }
    }

    return units;
  }

  function joinMarkdownUnits(units) {
    var out = "";
    for (var i = 0; i < units.length; i++) {
      var u = units[i];
      var text = String(u.text || "").trim();
      if (!text) continue;
      if (out) {
        var prev = units[i - 1];
        if (prev && prev.kind === "list-item" && u.kind === "list-item" && prev.ordered === u.ordered) {
          out += "\n";
        } else {
          out += "\n\n";
        }
      }
      out += text;
    }
    return out;
  }

  /* ── XSS-safe Markdown + KaTeX DOM renderer ────────────────────────────── */

  function parseInline(text, parent) {
    if (!text) return;
    var len = text.length;
    var i = 0;
    var plainStart = 0;

    function flushPlain(end) {
      if (end > plainStart) {
        parent.appendChild(document.createTextNode(text.slice(plainStart, end)));
      }
    }

    while (i < len) {
      var c = text.charAt(i);

      // Escaped char: \$, \*, \\, etc.
      if (c === "\\") {
        var next = text.charAt(i + 1);
        if (next === "(") {
          var closeP = text.indexOf("\\)", i + 2);
          if (closeP !== -1) {
            flushPlain(i);
            var spanP = document.createElement("span");
            spanP.className = "math math-inline";
            spanP.textContent = text.slice(i + 2, closeP).trim();
            parent.appendChild(spanP);
            i = closeP + 2;
            plainStart = i;
            continue;
          }
        } else if (next === "[") {
          var closeB = text.indexOf("\\]", i + 2);
          if (closeB !== -1) {
            flushPlain(i);
            var divB = document.createElement("div");
            divB.className = "math math-display";
            divB.textContent = text.slice(i + 2, closeB).trim();
            parent.appendChild(divB);
            i = closeB + 2;
            plainStart = i;
            continue;
          }
        } else if (next && "\\`*_{}[]()#+-.!$|~".indexOf(next) !== -1) {
          flushPlain(i);
          parent.appendChild(document.createTextNode(next));
          i += 2;
          plainStart = i;
          continue;
        }
        i++;
        continue;
      }

      // Inline/Display $$...$$ or $...$
      if (c === "$") {
        if (text.charAt(i + 1) === "$") {
          var closeDd = text.indexOf("$$", i + 2);
          if (closeDd !== -1) {
            flushPlain(i);
            var mDiv = document.createElement("div");
            mDiv.className = "math math-display";
            mDiv.textContent = text.slice(i + 2, closeDd).trim();
            parent.appendChild(mDiv);
            i = closeDd + 2;
            plainStart = i;
            continue;
          }
        }
        var chAfter = text.charAt(i + 1);
        if (chAfter && !/\s/.test(chAfter) && chAfter !== "$") {
          var closeD = -1;
          for (var k = i + 1; k < len; k++) {
            var ck = text.charAt(k);
            if (ck === "\n") break;
            if (ck === "$" && text.charAt(k - 1) !== "\\") {
              if (!/\s/.test(text.charAt(k - 1))) closeD = k;
              break;
            }
          }
          if (closeD > i + 1) {
            flushPlain(i);
            var mSpan = document.createElement("span");
            mSpan.className = "math math-inline";
            mSpan.textContent = text.slice(i + 1, closeD);
            parent.appendChild(mSpan);
            i = closeD + 1;
            plainStart = i;
            continue;
          }
        }
      }

      // Inline code (`...` or ``...``)
      if (c === "`") {
        var tickCount = 1;
        while (text.charAt(i + tickCount) === "`") tickCount++;
        var delim = text.slice(i, i + tickCount);
        var closeCode = text.indexOf(delim, i + tickCount);
        if (closeCode !== -1) {
          flushPlain(i);
          var codeEl = document.createElement("code");
          codeEl.textContent = text.slice(i + tickCount, closeCode).trim();
          parent.appendChild(codeEl);
          i = closeCode + tickCount;
          plainStart = i;
          continue;
        }
      }

      // Strong (**...** or __...__)
      if ((c === "*" && text.charAt(i + 1) === "*") || (c === "_" && text.charAt(i + 1) === "_")) {
        var dbl = c + c;
        var closeStrong = text.indexOf(dbl, i + 2);
        if (closeStrong > i + 2) {
          flushPlain(i);
          var strong = document.createElement("strong");
          parseInline(text.slice(i + 2, closeStrong), strong);
          parent.appendChild(strong);
          i = closeStrong + 2;
          plainStart = i;
          continue;
        }
      }

      // Strikethrough (~~...~~)
      if (c === "~" && text.charAt(i + 1) === "~") {
        var closeDel = text.indexOf("~~", i + 2);
        if (closeDel > i + 2) {
          flushPlain(i);
          var del = document.createElement("del");
          parseInline(text.slice(i + 2, closeDel), del);
          parent.appendChild(del);
          i = closeDel + 2;
          plainStart = i;
          continue;
        }
      }

      // Emphasis (*...* or _..._)
      if (c === "*" || c === "_") {
        var prevWord = c === "_" && i > 0 && /[a-zA-Z0-9]/.test(text.charAt(i - 1));
        if (!prevWord) {
          var closeEm = -1;
          for (var emIdx = i + 1; emIdx < len; emIdx++) {
            var ec = text.charAt(emIdx);
            if (ec === "\n") break;
            if (ec === c && text.charAt(emIdx - 1) !== "\\" && text.charAt(emIdx + 1) !== c) {
              if (c === "_" && /[a-zA-Z0-9]/.test(text.charAt(emIdx + 1))) continue;
              if (!/\s/.test(text.charAt(emIdx - 1))) {
                closeEm = emIdx;
                break;
              }
            }
          }
          if (closeEm > i + 1) {
            flushPlain(i);
            var em = document.createElement("em");
            parseInline(text.slice(i + 1, closeEm), em);
            parent.appendChild(em);
            i = closeEm + 1;
            plainStart = i;
            continue;
          }
        }
      }

      // Image ![alt](url)
      if (c === "!" && text.charAt(i + 1) === "[") {
        var closeAlt = text.indexOf("]", i + 2);
        if (closeAlt !== -1 && text.charAt(closeAlt + 1) === "(") {
          var closeImg = text.indexOf(")", closeAlt + 2);
          if (closeImg !== -1) {
            flushPlain(i);
            var imgUrl = safeUrl(text.slice(closeAlt + 2, closeImg).trim(), true);
            var img = document.createElement("img");
            img.src = imgUrl;
            img.alt = text.slice(i + 2, closeAlt);
            parent.appendChild(img);
            i = closeImg + 1;
            plainStart = i;
            continue;
          }
        }
      }

      // Link [label](url)
      if (c === "[") {
        var closeLbl = text.indexOf("]", i + 1);
        if (closeLbl !== -1 && text.charAt(closeLbl + 1) === "(") {
          var closeHref = text.indexOf(")", closeLbl + 2);
          if (closeHref !== -1) {
            flushPlain(i);
            var a = document.createElement("a");
            var href = safeUrl(text.slice(closeLbl + 2, closeHref).trim(), false);
            a.setAttribute("href", href);
            if (/^https?:\/\//i.test(href)) {
              a.setAttribute("rel", "noopener noreferrer");
            }
            parseInline(text.slice(i + 1, closeLbl), a);
            parent.appendChild(a);
            i = closeHref + 1;
            plainStart = i;
            continue;
          }
        }
      }

      i++;
    }
    flushPlain(len);
  }

  function renderUnitDom(unit, options) {
    options = options || {};
    var frag = document.createDocumentFragment();
    var text = unit.text || "";

    if (unit.kind === "env") {
      if (unit.envTitle) {
        var titleP = document.createElement("p");
        titleP.setAttribute("dir", "auto");
        var strongTitle = document.createElement("strong");
        strongTitle.textContent = "(" + unit.envTitle + ")";
        titleP.appendChild(strongTitle);
        frag.appendChild(titleP);
      }
      var innerUnits = splitMarkdownUnits(unit.body || "");
      for (var eu = 0; eu < innerUnits.length; eu++) {
        frag.appendChild(renderUnitDom(innerUnits[eu]));
      }
      return frag;
    }

    if (unit.kind === "fence") {
      var lines = text.split("\n");
      var firstLine = lines[0] || "";
      var fenceMatch = firstLine.match(/^\s{0,3}(?:`{3,}|~{3,})(.*)$/);
      var lang = fenceMatch ? fenceMatch[1].trim().split(/\s+/)[0] : "";
      var bodyEnd = lines.length > 1 && /^\s{0,3}(?:`{3,}|~{3,})\s*$/.test(lines[lines.length - 1])
        ? lines.length - 1
        : lines.length;
      var pre = document.createElement("pre");
      var code = document.createElement("code");
      if (lang) code.className = "language-" + lang.replace(/[^a-zA-Z0-9_-]/g, "");
      code.textContent = lines.slice(1, bodyEnd).join("\n");
      pre.appendChild(code);
      frag.appendChild(pre);
      return frag;
    }

    if (unit.kind === "math") {
      var trimmed = text.trim();
      var tex = trimmed;
      if (trimmed.indexOf("$$") === 0 && trimmed.slice(-2) === "$$" && trimmed.length >= 4) {
        tex = trimmed.slice(2, -2).trim();
      } else if (trimmed.indexOf("\\[") === 0 && trimmed.slice(-2) === "\\]" && trimmed.length >= 4) {
        tex = trimmed.slice(2, -2).trim();
      }
      var mDiv = document.createElement("div");
      mDiv.className = "math math-display";
      mDiv.textContent = tex;
      frag.appendChild(mDiv);
      return frag;
    }

    if (unit.kind === "heading") {
      var hm = text.match(/^\s{0,3}(#{1,6})(?:\s+(.*))?$/);
      var content = hm ? (hm[2] || "") : text;
      if (options.inlineHeading) {
        parseInline(content, frag);
        return frag;
      }
      var lvl = hm ? hm[1].length : 2;
      var h = document.createElement("h" + lvl);
      h.setAttribute("dir", "auto");
      parseInline(content, h);
      frag.appendChild(h);
      return frag;
    }

    if (unit.kind === "hr") {
      frag.appendChild(document.createElement("hr"));
      return frag;
    }

    if (unit.kind === "blockquote") {
      var bq = document.createElement("blockquote");
      var innerText = text.split("\n").map(function (l) {
        return l.replace(/^\s{0,3}>\s?/, "");
      }).join("\n");
      var bqUnits = splitMarkdownUnits(innerText);
      for (var bi = 0; bi < bqUnits.length; bi++) {
        bq.appendChild(renderUnitDom(bqUnits[bi]));
      }
      frag.appendChild(bq);
      return frag;
    }

    if (unit.kind === "table") {
      var tLines = text.split("\n");
      var splitRow = function (row) {
        var r = row.trim().replace(/^\|/, "").replace(/\|$/, "");
        return r.split("|").map(function (c) { return c.trim(); });
      };
      var table = document.createElement("table");
      var thead = document.createElement("thead");
      var htr = document.createElement("tr");
      var headers = splitRow(tLines[0] || "");
      for (var hi = 0; hi < headers.length; hi++) {
        var th = document.createElement("th");
        parseInline(headers[hi], th);
        htr.appendChild(th);
      }
      thead.appendChild(htr);
      table.appendChild(thead);
      if (tLines.length > 2) {
        var tbody = document.createElement("tbody");
        for (var ri = 2; ri < tLines.length; ri++) {
          var tr = document.createElement("tr");
          var cells = splitRow(tLines[ri]);
          for (var ci = 0; ci < headers.length; ci++) {
            var td = document.createElement("td");
            parseInline(cells[ci] || "", td);
            tr.appendChild(td);
          }
          tbody.appendChild(tr);
        }
        table.appendChild(tbody);
      }
      frag.appendChild(table);
      return frag;
    }

    if (unit.kind === "list-item") {
      var listTag = unit.ordered ? "ol" : "ul";
      var list = document.createElement(listTag);
      list.className = "mx-lp-list";
      var liEl = document.createElement("li");
      var liLines = text.split("\n");
      var firstLi = matchListStart(liLines[0]);
      var itemText = (firstLi ? firstLi.text : liLines[0]);
      if (liLines.length > 1) {
        itemText += " " + liLines.slice(1).map(function (l) { return l.trim(); }).join(" ");
      }
      parseInline(itemText, liEl);
      list.appendChild(liEl);
      frag.appendChild(list);
      return frag;
    }

    var p = document.createElement("p");
    p.setAttribute("dir", "auto");
    parseInline(text.replace(/\n/g, " "), p);
    frag.appendChild(p);
    return frag;
  }

  function renderMathInContainer(container) {
    if (!container) return;
    if (window.Mathia && typeof window.Mathia.renderMathIn === "function") {
      window.Mathia.renderMathIn(container);
      return;
    }
    if (!window.katex) return;
    container.querySelectorAll(".math").forEach(function (el) {
      if (el.dataset.mxTex !== undefined) return;
      var tex = el.textContent.trim();
      el.dataset.mxTex = tex;
      var isDisplay = el.classList.contains("math-display") || el.tagName === "DIV";
      try {
        window.katex.render(tex, el, { displayMode: isDisplay, throwOnError: false });
      } catch (err) {
        el.classList.add("mx-math-error");
        el.title = String(err);
      }
    });
  }

  function appendUnitsMerged(units, parent) {
    var i = 0;
    while (i < units.length) {
      var u = units[i];
      if (u.kind === "list-item") {
        var list = document.createElement(u.ordered ? "ol" : "ul");
        var ordered = u.ordered;
        while (i < units.length && units[i].kind === "list-item" && units[i].ordered === ordered) {
          var dom = renderUnitDom(units[i], {});
          var li = dom.querySelector("li");
          if (li) list.appendChild(li);
          i++;
        }
        parent.appendChild(list);
      } else {
        parent.appendChild(renderUnitDom(u, {}));
        i++;
      }
    }
  }

  /* ── Client-side block classification (for standalone browser edits) ───── */

  function stripMarkdownInline(s) {
    return String(s || "")
      .replace(/^\s*#{1,6}\s+/, "")
      .replace(/\*\*([^*]+)\*\*/g, "$1")
      .replace(/__([^_]+)__/g, "$1")
      .replace(/\*([^*]+)\*/g, "$1")
      .replace(/`([^`]+)`/g, "$1")
      .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
      .trim();
  }

  function classifyClientBlock(md, existing) {
    var trimmed = String(md || "").trim();
    var dir = detectDir(trimmed) || (existing && existing.dir) || document.documentElement.dir || "ltr";
    var firstLine = trimmed.split("\n")[0] || "";

    var hMatch = firstLine.match(/^\s{0,3}(#{1,6})\s+(.*)$/);
    if (hMatch && trimmed.indexOf("\n") === -1) {
      var level = hMatch[1].length;
      var title = stripMarkdownInline(hMatch[2]) || "Section";
      return {
        kind: "heading",
        headingLevel: level,
        dir: dir,
        label: dir === "rtl" ? "بخش" : "Section",
        title: title
      };
    }

    var envMatch = trimmed.match(/^\\begin\{([a-zA-Z*]+)\}(?:\[([^\]]*)\])?/);
    if (envMatch && !MATH_ENVS[envMatch[1].replace(/\*$/, "")]) {
      var envSlug = envMatch[1].replace(/\*$/, "").toLowerCase();
      for (var eIdx = 0; eIdx < SEMANTIC_KEYWORDS.length; eIdx++) {
        var row = SEMANTIC_KEYWORDS[eIdx];
        if (row[3].indexOf(envSlug) !== -1) {
          var lbl = dir === "rtl" ? row[2] : row[1];
          return {
            kind: row[0],
            headingLevel: 0,
            dir: dir,
            label: lbl,
            title: envMatch[2] ? lbl + " (" + envMatch[2] + ")" : lbl
          };
        }
      }
    }

    var cleanFirst = firstLine.replace(/^\s*(?:[-*+]|\d+\.)\s+/, "");
    var boldOpener = cleanFirst.match(/^(?:\*\*|__|\*|_)\s*([^*\n_]+?)(?:\*\*|__|\*|_)/);
    var probe = (boldOpener ? boldOpener[1] : cleanFirst).trim();
    for (var kIdx = 0; kIdx < SEMANTIC_KEYWORDS.length; kIdx++) {
      var kw = SEMANTIC_KEYWORDS[kIdx];
      var words = kw[3];
      for (var wIdx = 0; wIdx < words.length; wIdx++) {
        var word = words[wIdx];
        var lowerProbe = probe.toLowerCase();
        if (lowerProbe === word || lowerProbe.indexOf(word + " ") === 0 || lowerProbe.indexOf(word + ".") === 0 || lowerProbe.indexOf(word + ":") === 0 || lowerProbe.indexOf(word + "\u200c") === 0) {
          var badgeLbl = dir === "rtl" ? kw[2] : kw[1];
          var shortTitle = probe.replace(/[.:：]\s*$/, "").trim();
          if (!shortTitle || shortTitle.length > 48) shortTitle = badgeLbl;
          return {
            kind: kw[0],
            headingLevel: 0,
            dir: dir,
            label: badgeLbl,
            title: shortTitle
          };
        }
      }
    }

    return {
      kind: "text",
      headingLevel: 0,
      dir: dir,
      label: dir === "rtl" ? "متن" : "Text",
      title: "Content"
    };
  }

  function splitIntoTopLevelBlocks(md, templateBlock) {
    var trimmed = String(md || "").trim();
    if (!trimmed) return [];
    var units = splitMarkdownUnits(trimmed);
    var groups = [];
    var current = [];
    for (var i = 0; i < units.length; i++) {
      var u = units[i];
      if (u.kind === "heading" && current.length > 0) {
        groups.push(joinMarkdownUnits(current));
        current = [u];
      } else {
        current.push(u);
        if (u.kind === "heading" && i + 1 < units.length) {
          groups.push(joinMarkdownUnits(current));
          current = [];
        }
      }
    }
    if (current.length > 0) groups.push(joinMarkdownUnits(current));
    return groups.map(function (groupMd) {
      var meta = classifyClientBlock(groupMd, templateBlock);
      return {
        id: (templateBlock && templateBlock.id) || "",
        kind: meta.kind,
        headingLevel: meta.headingLevel,
        dir: meta.dir,
        label: meta.label,
        title: meta.title,
        proves: (templateBlock && templateBlock.proves) || "",
        provesTitle: (templateBlock && templateBlock.provesTitle) || "",
        hasFigure: false,
        edited: true,
        md: groupMd,
        fileIndex: (templateBlock && typeof templateBlock.fileIndex === "number") ? templateBlock.fileIndex : 0
      };
    });
  }

  function reassignIdsAndToc(blocks) {
    var counters = {};
    var secCounter = 0;
    var used = {};
    var toc = [];
    var depth = 0;

    for (var i = 0; i < blocks.length; i++) {
      var b = blocks[i];
      if (b.kind === "heading") {
        secCounter++;
        var base = (b.id && !used[b.id] && !/^block-/.test(b.id)) ? b.id : ("sec-" + secCounter);
        while (used[base]) base += "-x";
        used[base] = 1;
        b.id = base;
        depth = Math.max(1, Math.min(6, b.headingLevel || 2));
        toc.push({ id: b.id, title: b.title, depth: depth, kind: "heading" });
      } else if (b.kind !== "text") {
        counters[b.kind] = (counters[b.kind] || 0) + 1;
        var expectedPrefix = "block-" + b.kind + "-";
        var bid = (b.id && !used[b.id] && (!/^block-/.test(b.id) || b.id.indexOf(expectedPrefix) === 0))
          ? b.id
          : (expectedPrefix + counters[b.kind]);
        while (used[bid]) bid += "-x";
        used[bid] = 1;
        b.id = bid;
        toc.push({ id: b.id, title: b.title, depth: depth + 1, kind: b.kind });
      } else {
        counters.text = (counters.text || 0) + 1;
        var tid = (b.id && !used[b.id] && !/^sec-/.test(b.id)) ? b.id : ("block-text-" + counters.text);
        while (used[tid]) tid += "-x";
        used[tid] = 1;
        b.id = tid;
      }
    }
    return toc;
  }

  /* ── Caret line-boundary detection using a hidden mirror ───────────────── */

  var mirrorEl = null;
  function caretBoundary(textarea) {
    if (!mirrorEl) {
      mirrorEl = document.createElement("div");
      mirrorEl.setAttribute("aria-hidden", "true");
      mirrorEl.style.cssText = "position:fixed;top:-9999px;left:-9999px;visibility:hidden;pointer-events:none;white-space:pre-wrap;word-wrap:break-word;overflow-wrap:break-word;";
      document.body.appendChild(mirrorEl);
    }
    var cs = window.getComputedStyle(textarea);
    mirrorEl.style.width = cs.width;
    mirrorEl.style.fontFamily = cs.fontFamily;
    mirrorEl.style.fontSize = cs.fontSize;
    mirrorEl.style.fontWeight = cs.fontWeight;
    mirrorEl.style.lineHeight = cs.lineHeight;
    mirrorEl.style.letterSpacing = cs.letterSpacing;
    mirrorEl.style.direction = cs.direction;

    var pos = textarea.selectionStart;
    var val = textarea.value;
    var before = val.slice(0, pos);
    var after = val.slice(pos) || "\u200b";

    mirrorEl.textContent = "";
    mirrorEl.appendChild(document.createTextNode(before));
    var marker = document.createElement("span");
    marker.textContent = "\u200b";
    mirrorEl.appendChild(marker);
    mirrorEl.appendChild(document.createTextNode(after));

    var mRect = mirrorEl.getBoundingClientRect();
    var cRect = marker.getBoundingClientRect();
    var lineH = parseFloat(cs.lineHeight) || (parseFloat(cs.fontSize) * 1.6) || 24;
    return {
      atFirstLine: (cRect.top - mRect.top) < lineH * 0.65,
      atLastLine: (mRect.bottom - cRect.bottom) < lineH * 0.65
    };
  }

  function estimateClickOffset(unitEl, text, clientX, clientY) {
    var range = null;
    if (document.caretRangeFromPoint) {
      range = document.caretRangeFromPoint(clientX, clientY);
    } else if (document.caretPositionFromPoint) {
      var pos = document.caretPositionFromPoint(clientX, clientY);
      if (pos) {
        range = document.createRange();
        range.setStart(pos.offsetNode, pos.offset);
      }
    }
    if (!range || !unitEl.contains(range.startContainer)) return text.length;
    try {
      var preRange = document.createRange();
      preRange.setStart(unitEl, 0);
      preRange.setEnd(range.startContainer, range.startOffset);
      var prefixLen = preRange.toString().length;
      var renderedTotal = Math.max(1, unitEl.textContent.length);
      return Math.max(0, Math.min(text.length, Math.round((prefixLen / renderedTotal) * text.length)));
    } catch (err) {
      return text.length;
    }
  }

  /* ── Clean DOM copy helper for preserving untouched SVGs/cross-refs ────── */

  function captureCleanBlockDom(el) {
    var clone = el.cloneNode(true);
    clone.querySelectorAll(".mx-math-copy").forEach(function (n) { n.remove(); });
    clone.querySelectorAll(".mx-actions button").forEach(function (n) { n.remove(); });
    clone.querySelectorAll(".math").forEach(function (m) {
      if (m.dataset.mxTex !== undefined) {
        var tex = m.dataset.mxTex;
        delete m.dataset.mxTex;
        delete m.dataset.mxCopy;
        m.removeAttribute("title");
        m.textContent = tex;
      }
    });
    return clone;
  }

  /* ── Main Editor Controller ────────────────────────────────────────────── */

  function start() {
    var sourceEl = document.getElementById("mx-source-data");
    var main = document.getElementById("mx-main");
    if (!sourceEl || !main) return;

    var sourceData;
    try {
      sourceData = JSON.parse(sourceEl.textContent);
    } catch (err) {
      return;
    }
    if (!sourceData || !Array.isArray(sourceData.blocks)) return;

    var framed = window.parent !== window;
    var rtl = document.documentElement.dir === "rtl";
    var say = function (en, fa) { return rtl ? fa : en; };
    var docId = document.body.getAttribute("data-mx-doc-id") || "default";
    var draftKey = "mathia-edit-draft:" + docId;

    var baselineBlocks = cloneData(sourceData.blocks);
    var blocks = cloneData(sourceData.blocks);
    var editMode = false;
    var dirty = false;
    var active = null; // { blockIdx, unitIdx, textarea, previewEl, unitEl, units, initialText }
    var fileHandle = null;
    var commitTimer = null;
    var draftTimer = null;
    var undoStack = [];
    var redoStack = [];
    var pendingSyncAfterBlur = null;
    var pointerDownInMain = false;
    var blockUnitsCache = [];
    var cleanBlockCache = Object.create(null);

    // Capture initial server-rendered block elements (preserving SVGs, cross-refs, footnotes).
    Array.prototype.forEach.call(main.children, function (child) {
      var bid = child.getAttribute && child.getAttribute("data-block-id");
      if (bid) cleanBlockCache[bid] = captureCleanBlockDom(child);
    });
    blocks.forEach(function (b) { if (b.id) b._origId = b.id; });
    baselineBlocks.forEach(function (b) { if (b.id) b._origId = b.id; });

    function ensureUnitsFor(idx) {
      if (!blocks[idx]) return [];
      if (!blockUnitsCache[idx]) {
        blockUnitsCache[idx] = splitMarkdownUnits(blocks[idx].md);
        if (blockUnitsCache[idx].length === 0) {
          blockUnitsCache[idx] = [{ kind: "paragraph", text: "" }];
        }
      }
      return blockUnitsCache[idx];
    }

    function rebuildAllUnits() {
      blockUnitsCache = blocks.map(function (b) {
        var u = splitMarkdownUnits(b.md);
        return u.length ? u : [{ kind: "paragraph", text: "" }];
      });
    }
    rebuildAllUnits();

    /* ── UI Mounting: Toggle, Action Bar, Draft Banner ── */

    var editToggle = document.createElement("button");
    editToggle.id = "mx-edit-toggle";
    editToggle.type = "button";
    editToggle.className = "mx-edit-toggle";
    editToggle.textContent = say("✎ Edit", "✎ ویرایش");
    editToggle.title = say("Live Preview edit mode (Ctrl/Cmd+Shift+E)", "حالت ویرایش زنده (Ctrl/Cmd+Shift+E)");
    editToggle.setAttribute("aria-pressed", "false");

    var mapSwitch = document.querySelector(".mx-map-switch");
    if (mapSwitch) {
      var sep = document.createElement("span");
      sep.className = "mx-edit-sep";
      sep.setAttribute("aria-hidden", "true");
      mapSwitch.appendChild(sep);
      mapSwitch.appendChild(editToggle);
    } else {
      editToggle.classList.add("mx-edit-toggle-standalone");
      document.body.appendChild(editToggle);
    }

    var editBar = document.createElement("div");
    editBar.id = "mx-edit-bar";
    editBar.className = "mx-edit-bar";
    editBar.setAttribute("role", "toolbar");
    editBar.setAttribute("aria-label", say("Document editor controls", "ابزار ویرایش سند"));
    editBar.hidden = true;

    function barBtn(id, label, title, onClick, isPrimary) {
      var btn = document.createElement("button");
      if (id) btn.id = id;
      btn.type = "button";
      btn.textContent = label;
      if (title) btn.title = title;
      if (isPrimary) btn.className = "mx-edit-primary";
      btn.addEventListener("click", onClick);
      editBar.appendChild(btn);
      return btn;
    }

    var undoBtn = barBtn("mx-edit-undo", "↶", say("Undo (Ctrl/Cmd+Z)", "واگرد"), function () { undo(); });
    var redoBtn = barBtn("mx-edit-redo", "↷", say("Redo (Ctrl/Cmd+Shift+Z)", "ازنو"), function () { redo(); });
    var sep1 = document.createElement("span");
    sep1.className = "mx-edit-bar-sep";
    editBar.appendChild(sep1);

    barBtn("mx-edit-save", say("Save", "ذخیره"), say("Save changes (Ctrl/Cmd+S)", "ذخیره تغییرات"), function () {
      flushCommit(true);
    }, true);

    var revertBtn = null;
    if (!framed) {
      barBtn("mx-edit-download-html", say("Download HTML", "دانلود HTML"), say("Download updated standalone HTML", "دانلود فایل HTML به‌روزشده"), function () {
        commitActiveUnit(true);
        downloadStandaloneHtml();
      });
      barBtn("mx-edit-export-md", say("Export .md", "خروجی .md"), say("Download as Markdown", "خروجی مارک‌داون"), function () {
        commitActiveUnit(true);
        exportMarkdownFile();
      });
      revertBtn = barBtn("mx-edit-revert", say("Revert", "بازگردانی"), say("Discard unsaved browser edits", "لغو تغییرات ذخیره‌نشده"), function () {
        revertBrowserDraft();
      });
    }

    var statusSpan = document.createElement("span");
    statusSpan.id = "mx-edit-status";
    statusSpan.className = "mx-edit-status";
    statusSpan.setAttribute("aria-live", "polite");
    editBar.appendChild(statusSpan);

    barBtn("mx-edit-done", say("Done", "پایان"), say("Return to Reading mode", "بازگشت به حالت مطالعه"), function () {
      setEditMode(false);
    });
    document.body.appendChild(editBar);

    var banner = document.createElement("div");
    banner.id = "mx-edit-banner";
    banner.className = "mx-edit-banner";
    banner.hidden = true;
    var bannerMsg = document.createElement("span");
    bannerMsg.textContent = say(
      "Restored unsaved edits from this browser's storage.",
      "ویرایش‌های ذخیره‌نشده از حافظه مرورگر بازیابی شد."
    );
    var bannerActions = document.createElement("div");
    bannerActions.className = "mx-edit-banner-actions";
    var bannerSave = document.createElement("button");
    bannerSave.type = "button";
    bannerSave.textContent = say("Download HTML", "دانلود HTML");
    bannerSave.addEventListener("click", function () { downloadStandaloneHtml(); });
    var bannerRevert = document.createElement("button");
    bannerRevert.type = "button";
    bannerRevert.textContent = say("Discard draft", "حذف پیش‌نویس");
    bannerRevert.addEventListener("click", function () { revertBrowserDraft(); });
    var bannerClose = document.createElement("button");
    bannerClose.type = "button";
    bannerClose.textContent = "✕";
    bannerClose.setAttribute("aria-label", say("Dismiss", "بستن"));
    bannerClose.addEventListener("click", function () { banner.hidden = true; });
    bannerActions.append(bannerSave, bannerRevert, bannerClose);
    banner.append(bannerMsg, bannerActions);

    var addRow = document.createElement("div");
    addRow.className = "mx-edit-add-row";
    addRow.hidden = true;
    var addBtn = document.createElement("button");
    addBtn.type = "button";
    addBtn.className = "mx-edit-add-btn";
    addBtn.textContent = say("+ Add paragraph", "+ افزودن بند جدید");
    addBtn.addEventListener("click", function () {
      appendEmptyBlock();
    });
    addRow.appendChild(addBtn);

    function setStatus(text, isDirty) {
      statusSpan.textContent = text || "";
      statusSpan.classList.toggle("mx-edit-dirty", Boolean(isDirty));
    }

    function updateBar() {
      editBar.hidden = !editMode && (framed || !dirty);
      if (framed) {
        undoBtn.disabled = false;
        redoBtn.disabled = false;
      } else {
        undoBtn.disabled = undoStack.length === 0;
        redoBtn.disabled = redoStack.length === 0;
        if (revertBtn) revertBtn.disabled = !dirty;
      }
      addRow.hidden = !editMode;
    }

    /* ── Rendering blocks & TOC ────────────────────────────────────────────── */

    function renderTocList(tocEntries) {
      var list = document.querySelector("#mx-toc .mx-toc-list");
      if (!list) return;
      list.textContent = "";
      for (var i = 0; i < tocEntries.length; i++) {
        var entry = tocEntries[i];
        var li = document.createElement("li");
        li.className = "mx-toc-item mx-toc-" + entry.kind;
        li.style.setProperty("--mx-depth", String(entry.depth));
        li.dataset.depth = String(entry.depth);
        var a = document.createElement("a");
        a.setAttribute("href", "#" + entry.id);
        a.dataset.tocTarget = entry.id;
        a.textContent = entry.title;
        li.appendChild(a);
        list.appendChild(li);
      }
      if (window.Mathia && typeof window.Mathia.refreshTocFolding === "function") {
        window.Mathia.refreshTocFolding();
      }
    }

    function renderTocFromBlocks() {
      var toc = reassignIdsAndToc(blocks);
      renderTocList(toc);
    }

    function syncSourceDataElement() {
      sourceData.blocks = cloneData(blocks);
      sourceEl.textContent = JSON.stringify(sourceData);
    }

    function createUnitWrapper(blockIdx, unitIdx, unit, renderOpts) {
      var wrap = document.createElement("div");
      wrap.className = "mx-lp-unit";
      wrap.dataset.blockIndex = String(blockIdx);
      wrap.dataset.unitIndex = String(unitIdx);
      wrap.appendChild(renderUnitDom(unit, renderOpts));
      renderMathInContainer(wrap);
      return wrap;
    }

    function cachedDomFor(b) {
      if (!b) return null;
      return (b.id && cleanBlockCache[b.id]) || (b._origId && cleanBlockCache[b._origId]) || null;
    }

    function appendPreservedFigures(b, container) {
      if (!b.hasFigure) return;
      var cached = cachedDomFor(b);
      if (!cached) return;
      var figs = cached.querySelectorAll("figure, svg");
      for (var i = 0; i < figs.length; i++) {
        var el = figs[i];
        if (el.tagName && el.tagName.toLowerCase() === "svg" && el.closest && el.closest("figure")) {
          continue;
        }
        container.appendChild(el.cloneNode(true));
      }
    }

    function renderBlockElement(blockIdx, forEdit) {
      var b = blocks[blockIdx];
      var cached = cachedDomFor(b);
      // Preserve pristine server-rendered DOM (SVGs, cross-references) when untouched in Reading mode.
      if (!forEdit && !b.edited && cached) {
        var restored = cached.cloneNode(true);
        restored.dataset.blockIndex = String(blockIdx);
        return restored;
      }

      var units = ensureUnitsFor(blockIdx);

      if (b.kind === "heading") {
        var lvl = Math.max(1, Math.min(6, b.headingLevel || 2));
        var h = document.createElement("h" + lvl);
        h.id = b.id;
        h.className = "mx-heading";
        h.dataset.blockId = b.id;
        h.dataset.blockType = "heading";
        h.dataset.blockIndex = String(blockIdx);
        h.setAttribute("dir", b.dir || "auto");
        if (forEdit) {
          h.appendChild(createUnitWrapper(blockIdx, 0, units[0], { inlineHeading: true }));
        } else {
          h.appendChild(renderUnitDom(units[0], { inlineHeading: true }));
        }
        return h;
      }

      var sec = document.createElement("section");
      sec.id = b.id;
      sec.className = "mx-block mx-" + b.kind;
      sec.dataset.blockId = b.id;
      sec.dataset.blockType = b.kind;
      sec.dataset.blockIndex = String(blockIdx);
      sec.setAttribute("dir", b.dir || "auto");
      if (b.proves) sec.dataset.proves = b.proves;
      if (b.kind !== "text") sec.setAttribute("aria-label", b.title || b.label || b.kind);
      if (b.hasFigure) sec.dataset.mxFigure = "true";

      if (b.kind !== "text") {
        var head = document.createElement("header");
        head.className = "mx-block-head";
        var badge = document.createElement("span");
        badge.className = "mx-badge";
        badge.textContent = b.label || b.kind;
        head.appendChild(badge);
        if (b.proves && b.provesTitle) {
          var prov = document.createElement("a");
          prov.className = "mx-proves";
          prov.setAttribute("href", "#" + b.proves);
          prov.textContent = b.provesTitle;
          head.appendChild(prov);
        }
        var actionsSlot = document.createElement("span");
        actionsSlot.className = "mx-actions";
        actionsSlot.dataset.blockActions = b.id;
        head.appendChild(actionsSlot);
        sec.appendChild(head);
      }

      var body = document.createElement("div");
      body.className = "mx-block-body";
      if (forEdit) {
        for (var uIdx = 0; uIdx < units.length; uIdx++) {
          body.appendChild(createUnitWrapper(blockIdx, uIdx, units[uIdx], {}));
        }
        appendPreservedFigures(b, body);
      } else {
        appendUnitsMerged(units, body);
        if (b.edited) appendPreservedFigures(b, body);
      }
      sec.appendChild(body);
      return sec;
    }

    function renderAllBlocks(forEdit) {
      active = null;
      main.textContent = "";
      if (!banner.hidden) main.appendChild(banner);
      for (var i = 0; i < blocks.length; i++) {
        main.appendChild(renderBlockElement(i, forEdit));
      }
      if (forEdit) main.appendChild(addRow);
      if (window.Mathia && typeof window.Mathia.renderMath === "function") {
        window.Mathia.renderMath();
      } else {
        renderMathInContainer(main);
      }
      if (!forEdit && window.Mathia && typeof window.Mathia.remountActions === "function") {
        window.Mathia.remountActions();
      }
      if (window.Mathia && typeof window.Mathia.refreshScrollSpy === "function") {
        window.Mathia.refreshScrollSpy();
      }
    }

    function updateSingleBlockShell(blockIdx) {
      var el = main.querySelector('[data-block-index="' + blockIdx + '"]');
      if (!el) return;
      var b = blocks[blockIdx];
      if (!framed) {
        var meta = classifyClientBlock(b.md, b);
        b.kind = meta.kind;
        b.headingLevel = meta.headingLevel;
        b.dir = meta.dir;
        b.label = meta.label;
        b.title = meta.title;
      }
      if (b.kind === "heading") {
        var wantTag = "H" + Math.max(1, Math.min(6, b.headingLevel || 2));
        if (el.tagName !== wantTag) {
          var replacement = renderBlockElement(blockIdx, editMode);
          el.parentNode.replaceChild(replacement, el);
        }
      } else if (el.tagName !== "SECTION" || el.dataset.blockType !== b.kind) {
        var sec = renderBlockElement(blockIdx, editMode);
        el.parentNode.replaceChild(sec, el);
      } else {
        var badge = el.querySelector(".mx-badge");
        if (badge) badge.textContent = b.label || b.kind;
        el.setAttribute("dir", b.dir || "auto");
      }
    }

    /* ── Obsidian Unit Activation & Keyboard Navigation ────────────────────── */

    function resizeTextarea(ta) {
      ta.style.height = "auto";
      ta.style.height = Math.max(24, ta.scrollHeight) + "px";
    }

    function updateMathPreview(ta, previewEl) {
      if (!previewEl || !active) return;
      var val = ta.value;
      var hasMath = val.indexOf("$") !== -1 || val.indexOf("\\(") !== -1 || val.indexOf("\\[") !== -1 || val.indexOf("\\begin{") !== -1;
      if (!hasMath) {
        previewEl.textContent = "";
        previewEl.hidden = true;
        return;
      }
      previewEl.hidden = false;
      previewEl.textContent = "";
      var lbl = document.createElement("span");
      lbl.className = "mx-lp-math-preview-label";
      lbl.textContent = say("Preview", "پیش‌نمایش");
      previewEl.appendChild(lbl);
      var parsed = splitMarkdownUnits(val);
      var isHeading = blocks[active.blockIdx] && blocks[active.blockIdx].kind === "heading";
      for (var i = 0; i < parsed.length; i++) {
        previewEl.appendChild(renderUnitDom(parsed[i], { inlineHeading: isHeading }));
      }
      renderMathInContainer(previewEl);
    }

    function activateUnit(blockIdx, unitIdx, caretPos) {
      if (!editMode) return;
      if (active && active.blockIdx === blockIdx && active.unitIdx === unitIdx) {
        if (typeof caretPos === "number") {
          active.textarea.setSelectionRange(caretPos, caretPos);
        }
        active.textarea.focus();
        return;
      }
      var targetUnitRef = ensureUnitsFor(blockIdx)[unitIdx];
      var targetBlockRef = blocks[blockIdx];
      commitActiveUnit(false);

      // Re-resolve indices in case committing the previous unit merged/split/removed a unit or block.
      if (targetBlockRef) {
        var resolvedB = blocks.indexOf(targetBlockRef);
        if (resolvedB !== -1) blockIdx = resolvedB;
      }
      var units = ensureUnitsFor(blockIdx);
      if (targetUnitRef) {
        var resolvedU = units.indexOf(targetUnitRef);
        if (resolvedU !== -1) unitIdx = resolvedU;
      }
      unitIdx = Math.max(0, Math.min(unitIdx, units.length - 1));

      var blockEl = main.querySelector('[data-block-index="' + blockIdx + '"]');
      if (!blockEl) return;
      var unitEl = blockEl.querySelector('.mx-lp-unit[data-unit-index="' + unitIdx + '"]');
      if (!unitEl) return;

      var unit = units[unitIdx];
      if (!unit) return;

      unitEl.classList.add("mx-lp-active");
      unitEl.textContent = "";

      var ta = document.createElement("textarea");
      ta.className = "mx-lp-input";
      if (unit.kind === "fence" || unit.kind === "math" || unit.kind === "env" || unit.kind === "table") {
        ta.classList.add("mx-lp-input-code");
      } else if (unit.kind === "heading") {
        ta.classList.add("mx-lp-input-heading");
      }
      ta.setAttribute("dir", "auto");
      ta.setAttribute("spellcheck", "false");
      ta.value = unit.text;

      var previewEl = document.createElement("div");
      previewEl.className = "mx-lp-math-preview";
      previewEl.hidden = true;

      unitEl.append(ta, previewEl);
      active = {
        blockIdx: blockIdx,
        unitIdx: unitIdx,
        textarea: ta,
        previewEl: previewEl,
        unitEl: unitEl,
        units: units,
        initialText: unit.text,
        preEditBlocks: !framed ? snapshotBlocks() : null
      };

      resizeTextarea(ta);
      updateMathPreview(ta, previewEl);
      ta.focus();
      var pos = caretPos === "start" ? 0
        : caretPos === "end" ? ta.value.length
        : typeof caretPos === "number" ? Math.max(0, Math.min(ta.value.length, caretPos))
        : ta.value.length;
      ta.setSelectionRange(pos, pos);

      if (framed && blocks[blockIdx] && blocks[blockIdx].id) {
        window.parent.postMessage({ mathia: "select", id: blocks[blockIdx].id }, "*");
      }

      ta.addEventListener("input", onTextareaInput);
      ta.addEventListener("keydown", onTextareaKeyDown);
      ta.addEventListener("blur", onTextareaBlur);
    }

    function snapshotBlocks() {
      return cloneData(blocks);
    }

    function pushBrowserUndo(before) {
      if (framed || isSameBlocks(before, blocks)) return;
      undoStack.push(before);
      if (undoStack.length > 80) undoStack.shift();
      redoStack = [];
      updateBar();
    }

    function commitActiveUnit(forceSync) {
      if (!active) return;
      var cur = active;
      active = null;

      var ta = cur.textarea;
      ta.removeEventListener("input", onTextareaInput);
      ta.removeEventListener("keydown", onTextareaKeyDown);
      ta.removeEventListener("blur", onTextareaBlur);

      var newText = ta.value.replace(/\r\n/g, "\n").replace(/\r/g, "\n").trimEnd();
      var bIdx = cur.blockIdx;
      var uIdx = cur.unitIdx;
      var units = ensureUnitsFor(bIdx);
      var changed = newText !== cur.initialText;
      var before = !framed && changed ? (cur.preEditBlocks || snapshotBlocks()) : null;

      if (!newText.trim()) {
        units.splice(uIdx, 1);
      } else {
        var reparsed = splitMarkdownUnits(newText);
        if (reparsed.length === 0) reparsed = [{ kind: "paragraph", text: newText }];
        Array.prototype.splice.apply(units, [uIdx, 1].concat(reparsed));
      }

      if (units.length === 0) {
        blocks.splice(bIdx, 1);
        blockUnitsCache.splice(bIdx, 1);
        if (blocks.length === 0) {
          blocks.push({ id: "block-text-1", kind: "text", headingLevel: 0, dir: rtl ? "rtl" : "ltr", label: say("Text", "متن"), title: "Content", proves: "", provesTitle: "", hasFigure: false, edited: true, md: "", fileIndex: 0 });
          blockUnitsCache = [[{ kind: "paragraph", text: "" }]];
        }
        renderAllBlocks(editMode);
        if (!framed) renderTocFromBlocks();
      } else {
        var nextMd = joinMarkdownUnits(units);
        blocks[bIdx].md = nextMd;
        if (changed) blocks[bIdx].edited = true;
        if (!framed && blocks[bIdx].kind !== "heading" && splitIntoTopLevelBlocks(nextMd, blocks[bIdx]).length > 1) {
          var splitBlocks = splitIntoTopLevelBlocks(nextMd, blocks[bIdx]);
          Array.prototype.splice.apply(blocks, [bIdx, 1].concat(splitBlocks));
          rebuildAllUnits();
          renderTocFromBlocks();
          renderAllBlocks(editMode);
        } else {
          var blockEl = main.querySelector('[data-block-index="' + bIdx + '"]');
          if (blockEl) {
            var replacement = renderBlockElement(bIdx, editMode);
            blockEl.parentNode.replaceChild(replacement, blockEl);
          }
          if (!framed) {
            updateSingleBlockShell(bIdx);
            renderTocFromBlocks();
          }
        }
      }

      if (changed) {
        dirty = true;
        if (before) pushBrowserUndo(before);
        syncSourceDataElement();
        scheduleDraftSave();
        scheduleCommit(framed || forceSync);
      } else if (pendingSyncAfterBlur) {
        var pending = pendingSyncAfterBlur;
        pendingSyncAfterBlur = null;
        applyHostSync(pending);
      }
      updateBar();
    }

    function onTextareaInput() {
      if (!active) return;
      var ta = active.textarea;
      resizeTextarea(ta);
      updateMathPreview(ta, active.previewEl);

      var val = ta.value;
      var unit = active.units[active.unitIdx];
      // Split into two units immediately if the user types a blank line (\n\n) inside normal prose.
      if (unit && unit.kind !== "fence" && unit.kind !== "math" && unit.kind !== "env") {
        var blankAt = val.indexOf("\n\n");
        if (blankAt !== -1) {
          var bIdx = active.blockIdx;
          var uIdx = active.unitIdx;
          var firstPart = val.slice(0, blankAt).trimEnd();
          var secondPart = val.slice(blankAt + 2);
          if (firstPart) {
            var before = active.preEditBlocks || (!framed ? snapshotBlocks() : null);
            var reparsedFirst = splitMarkdownUnits(firstPart);
            var secondUnits = secondPart.trim() ? splitMarkdownUnits(secondPart) : [{ kind: "paragraph", text: secondPart }];
            Array.prototype.splice.apply(active.units, [uIdx, 1].concat(reparsedFirst, secondUnits));
            blocks[bIdx].md = joinMarkdownUnits(active.units);
            blocks[bIdx].edited = true;
            dirty = true;
            if (before) pushBrowserUndo(before);
            syncSourceDataElement();
            active = null;
            var blockEl = main.querySelector('[data-block-index="' + bIdx + '"]');
            if (blockEl) blockEl.parentNode.replaceChild(renderBlockElement(bIdx, true), blockEl);
            activateUnit(bIdx, uIdx + reparsedFirst.length, 0);
            scheduleDraftSave();
            scheduleCommit(false);
            return;
          }
        }
      }

      if (blocks[active.blockIdx]) {
        active.units[active.unitIdx].text = val;
        blocks[active.blockIdx].md = joinMarkdownUnits(active.units);
        blocks[active.blockIdx].edited = true;
        dirty = true;
        setStatus(say("Unsaved changes", "تغییرات ذخیره‌نشده"), true);
        syncSourceDataElement();
        scheduleDraftSave();
        scheduleCommit(false);
      }
    }

    function moveUnit(delta, caretPos) {
      if (!active) return;
      var bIdx = active.blockIdx;
      var uIdx = active.unitIdx;
      var nextU = uIdx + delta;
      var units = ensureUnitsFor(bIdx);

      if (nextU >= 0 && nextU < units.length) {
        activateUnit(bIdx, nextU, caretPos);
        return;
      }
      var nextB = bIdx + delta;
      while (nextB >= 0 && nextB < blocks.length) {
        var targetUnits = ensureUnitsFor(nextB);
        if (targetUnits.length > 0) {
          activateUnit(nextB, delta > 0 ? 0 : targetUnits.length - 1, caretPos);
          return;
        }
        nextB += delta;
      }
    }

    function onTextareaKeyDown(e) {
      if (!active) return;
      var ta = active.textarea;
      var key = e.key;

      if ((e.ctrlKey || e.metaKey) && !e.altKey && key.toLowerCase() === "s") {
        e.preventDefault();
        flushCommit(true);
        return;
      }

      if (key === "Escape") {
        e.preventDefault();
        commitActiveUnit(false);
        return;
      }

      if (key === "ArrowUp" && !e.shiftKey && !e.altKey && !e.ctrlKey && !e.metaKey) {
        if (ta.selectionStart === ta.selectionEnd && caretBoundary(ta).atFirstLine) {
          e.preventDefault();
          moveUnit(-1, "end");
          return;
        }
      }

      if (key === "ArrowDown" && !e.shiftKey && !e.altKey && !e.ctrlKey && !e.metaKey) {
        if (ta.selectionStart === ta.selectionEnd && caretBoundary(ta).atLastLine) {
          e.preventDefault();
          moveUnit(1, "start");
          return;
        }
      }

      if (key === "Enter" && !e.shiftKey && !e.ctrlKey && !e.metaKey && !e.altKey) {
        var unit = active.units[active.unitIdx];
        if (unit && unit.kind === "heading") {
          e.preventDefault();
          var pos = ta.selectionStart;
          var headPart = ta.value.slice(0, pos).trimEnd();
          var tailPart = ta.value.slice(ta.selectionEnd).trimStart();
          var bIdx = active.blockIdx;
          var uIdx = active.unitIdx;
          var before = active.preEditBlocks || (!framed ? snapshotBlocks() : null);
          active.units[uIdx].text = headPart;
          blocks[bIdx].md = joinMarkdownUnits(active.units);
          blocks[bIdx].edited = true;
          blocks.splice(bIdx + 1, 0, {
            id: "",
            kind: "text",
            headingLevel: 0,
            dir: blocks[bIdx].dir,
            label: say("Text", "متن"),
            title: "Content",
            proves: "",
            provesTitle: "",
            hasFigure: false,
            edited: true,
            md: tailPart,
            fileIndex: blocks[bIdx].fileIndex
          });
          dirty = true;
          if (before) pushBrowserUndo(before);
          rebuildAllUnits();
          if (!framed) renderTocFromBlocks();
          syncSourceDataElement();
          renderAllBlocks(true);
          activateUnit(bIdx + 1, 0, 0);
          scheduleDraftSave();
          scheduleCommit(false);
          return;
        }

        if (unit && unit.kind === "list-item") {
          var liMatch = matchListStart(ta.value);
          if (liMatch) {
            e.preventDefault();
            var lPos = ta.selectionStart;
            var curB = active.blockIdx;
            var curU = active.unitIdx;
            if (!liMatch.text.trim()) {
              active.units[curU] = { kind: "paragraph", text: "" };
              blocks[curB].md = joinMarkdownUnits(active.units);
              blocks[curB].edited = true;
              active = null;
              var blkEl = main.querySelector('[data-block-index="' + curB + '"]');
              if (blkEl) blkEl.parentNode.replaceChild(renderBlockElement(curB, true), blkEl);
              activateUnit(curB, curU, 0);
              return;
            }
            var beforeLi = active.preEditBlocks || (!framed ? snapshotBlocks() : null);
            var leftLi = ta.value.slice(0, lPos);
            var rightLi = ta.value.slice(ta.selectionEnd);
            var nextPrefix = liMatch.ordered ? (parseInt(liMatch.marker, 10) + 1) + ". " : liMatch.marker + " ";
            active.units[curU].text = leftLi;
            active.units.splice(curU + 1, 0, { kind: "list-item", ordered: liMatch.ordered, text: nextPrefix + rightLi });
            blocks[curB].md = joinMarkdownUnits(active.units);
            blocks[curB].edited = true;
            dirty = true;
            if (beforeLi) pushBrowserUndo(beforeLi);
            syncSourceDataElement();
            active = null;
            var blkEl2 = main.querySelector('[data-block-index="' + curB + '"]');
            if (blkEl2) blkEl2.parentNode.replaceChild(renderBlockElement(curB, true), blkEl2);
            activateUnit(curB, curU + 1, nextPrefix.length);
            scheduleDraftSave();
            scheduleCommit(false);
            return;
          }
        }
      }

      if (key === "Backspace" && ta.selectionStart === 0 && ta.selectionEnd === 0) {
        var bIdx2 = active.blockIdx;
        var uIdx2 = active.unitIdx;
        var units2 = active.units;
        if (uIdx2 > 0) {
          var prevUnit = units2[uIdx2 - 1];
          if (prevUnit && prevUnit.kind !== "fence" && prevUnit.kind !== "math" && prevUnit.kind !== "hr" && prevUnit.kind !== "env") {
            e.preventDefault();
            var beforeMerge = active.preEditBlocks || (!framed ? snapshotBlocks() : null);
            var joinAt = prevUnit.text.length;
            var curText = ta.value;
            var curLi = matchListStart(curText);
            if (curLi && units2[uIdx2].kind === "list-item") curText = curLi.text;
            prevUnit.text = prevUnit.text + curText;
            units2.splice(uIdx2, 1);
            blocks[bIdx2].md = joinMarkdownUnits(units2);
            blocks[bIdx2].edited = true;
            dirty = true;
            if (beforeMerge) pushBrowserUndo(beforeMerge);
            syncSourceDataElement();
            active = null;
            var blockNode = main.querySelector('[data-block-index="' + bIdx2 + '"]');
            if (blockNode) blockNode.parentNode.replaceChild(renderBlockElement(bIdx2, true), blockNode);
            activateUnit(bIdx2, uIdx2 - 1, joinAt);
            scheduleDraftSave();
            scheduleCommit(false);
          }
        } else if (bIdx2 > 0 && !ta.value.trim() && blocks.length > 1) {
          e.preventDefault();
          var beforeDel = active.preEditBlocks || (!framed ? snapshotBlocks() : null);
          blocks.splice(bIdx2, 1);
          blockUnitsCache.splice(bIdx2, 1);
          dirty = true;
          if (beforeDel) pushBrowserUndo(beforeDel);
          if (!framed) renderTocFromBlocks();
          syncSourceDataElement();
          renderAllBlocks(true);
          var prevUnits = ensureUnitsFor(bIdx2 - 1);
          activateUnit(bIdx2 - 1, Math.max(0, prevUnits.length - 1), "end");
          scheduleDraftSave();
          scheduleCommit(false);
        }
      }
    }

    function onTextareaBlur(e) {
      if (!active || pointerDownInMain) return;
      var related = e.relatedTarget;
      if (related && related.closest && related.closest(".mx-lp-unit") === active.unitEl) return;
      var blurred = active;
      window.setTimeout(function () {
        if (active === blurred && !pointerDownInMain) commitActiveUnit(false);
      }, 20);
    }

    function appendEmptyBlock() {
      commitActiveUnit(false);
      var before = !framed ? snapshotBlocks() : null;
      var last = blocks[blocks.length - 1];
      blocks.push({
        id: "block-text-" + (blocks.length + 1),
        kind: "text",
        headingLevel: 0,
        dir: (last && last.dir) || (rtl ? "rtl" : "ltr"),
        label: say("Text", "متن"),
        title: "Content",
        proves: "",
        provesTitle: "",
        hasFigure: false,
        edited: true,
        md: "",
        fileIndex: last ? last.fileIndex : 0
      });
      if (before) pushBrowserUndo(before);
      rebuildAllUnits();
      renderAllBlocks(true);
      activateUnit(blocks.length - 1, 0, 0);
    }

    main.addEventListener("mousedown", function () {
      if (editMode) pointerDownInMain = true;
    });
    window.addEventListener("mouseup", function () {
      if (!pointerDownInMain) return;
      pointerDownInMain = false;
      window.setTimeout(function () {
        if (active && document.activeElement !== active.textarea) {
          commitActiveUnit(false);
        }
      }, 30);
    });

    main.addEventListener("click", function (e) {
      if (!editMode) return;
      if (e.target.closest && e.target.closest("button, .mx-actions, .mx-lp-input, .mx-lp-math-preview")) return;
      var unitEl = e.target.closest && e.target.closest(".mx-lp-unit");
      if (!unitEl) return;
      if (unitEl.classList.contains("mx-lp-active")) return;
      var sel = window.getSelection();
      if (sel && !sel.isCollapsed && String(sel).length > 0) return;
      var bIdx = Number(unitEl.dataset.blockIndex);
      var uIdx = Number(unitEl.dataset.unitIndex);
      var units = ensureUnitsFor(bIdx);
      if (!units[uIdx]) return;
      e.preventDefault();
      var offset = estimateClickOffset(unitEl, units[uIdx].text, e.clientX, e.clientY);
      activateUnit(bIdx, uIdx, offset);
    });

    /* ── Persistence: Desktop Host IPC & Browser LocalStorage/File Export ──── */

    function scheduleDraftSave() {
      if (framed) return;
      window.clearTimeout(draftTimer);
      draftTimer = window.setTimeout(function () {
        draftTimer = null;
        try {
          if (isSameBlocks(blocks, baselineBlocks)) {
            window.localStorage.removeItem(draftKey);
          } else {
            window.localStorage.setItem(draftKey, JSON.stringify({
              savedAt: Date.now(),
              blocks: blocks
            }));
          }
        } catch (err) { /* storage disabled or full */ }
      }, 300);
    }

    function scheduleCommit(immediate) {
      window.clearTimeout(commitTimer);
      if (!framed) {
        if (immediate) saveStandaloneBrowser();
        return;
      }
      if (immediate) {
        flushCommit(true);
        return;
      }
      commitTimer = window.setTimeout(function () {
        commitTimer = null;
        flushCommit(false);
      }, 550);
    }

    function flushCommit(explicit) {
      if (explicit) commitActiveUnit(false);
      if (!framed) {
        if (explicit) saveStandaloneBrowser();
        return;
      }
      setStatus(say("Saving…", "در حال ذخیره…"), true);
      window.parent.postMessage({
        mathia: "edit-commit",
        explicit: Boolean(explicit),
        edits: blocks.map(function (b) {
          return {
            id: b.id,
            md: b.md,
            fileIndex: b.fileIndex || 0,
            edited: Boolean(b.edited)
          };
        })
      }, "*");
    }

    function updateCacheFromRenderedPatch(blocksHtml) {
      if (!Array.isArray(blocksHtml)) return;
      var docParser = new DOMParser();
      for (var i = 0; i < blocksHtml.length; i++) {
        var item = blocksHtml[i];
        if (!item || !item.id || !item.outerHtml) continue;
        var parsed = docParser.parseFromString(item.outerHtml, "text/html");
        var node = parsed.body.firstElementChild;
        if (node) cleanBlockCache[item.id] = node;
      }
    }

    function applyHostSync(payload) {
      if (!payload || !payload.sourceData) return;
      if (active) {
        pendingSyncAfterBlur = payload;
        setStatus(say("Saved", "ذخیره شد"), false);
        dirty = false;
        return;
      }
      blocks = cloneData(payload.sourceData.blocks || []);
      sourceData = cloneData(payload.sourceData);
      syncSourceDataElement();
      updateCacheFromRenderedPatch(payload.blocksPatch);
      rebuildAllUnits();
      if (payload.mindmapJson) {
        var mm = document.getElementById("mx-mindmap-data");
        if (mm) mm.textContent = payload.mindmapJson;
      }
      if (Array.isArray(payload.toc)) {
        renderTocList(payload.toc);
      }
      renderAllBlocks(editMode);
      dirty = false;
      setStatus(say("Saved", "ذخیره شد"), false);
      updateBar();
    }

    function serializeStandaloneHtml() {
      var docClone = document.documentElement.cloneNode(true);
      docClone.querySelectorAll(
        ".mx-map-switch, .mx-map-panel, .mx-graph-panel, " +
        "#mx-highlight-toolbar, #mx-highlight-history, " +
        ".mx-ai-toggle, #mx-ai-dialog, #mx-ai-guide-toggle, #mx-ai-guide-dialog, " +
        ".mx-edit-bar, .mx-edit-banner, .mx-edit-add-row, #mx-edit-toggle, .mx-edit-sep, " +
        ".mx-toc-fold"
      ).forEach(function (n) { n.remove(); });

      docClone.classList.remove("mx-toc-collapsed");
      delete docClone.dataset.mxTheme;
      var bodyClone = docClone.querySelector("body");
      if (bodyClone) {
        bodyClone.classList.remove("mx-editing", "mx-map-has-ai", "mx-ai-offline");
        delete bodyClone.dataset.mxView;
      }

      ["mx-theme-toggle", "mx-print", "mx-search"].forEach(function (id) {
        var ctrl = docClone.querySelector("#" + id);
        if (ctrl) ctrl.setAttribute("hidden", "");
      });
      var searchIn = docClone.querySelector("#mx-search-input");
      if (searchIn) searchIn.value = "";

      var mainClone = docClone.querySelector("#mx-main");
      if (mainClone) {
        mainClone.textContent = "";
        for (var i = 0; i < blocks.length; i++) {
          var b = blocks[i];
          var cached = cachedDomFor(b);
          if (!b.edited && cached) {
            mainClone.appendChild(cached.cloneNode(true));
            continue;
          }
          var units = splitMarkdownUnits(b.md);
          if (b.kind === "heading") {
            var lvl = Math.max(1, Math.min(6, b.headingLevel || 2));
            var h = document.createElement("h" + lvl);
            h.id = b.id;
            h.className = "mx-heading";
            h.dataset.blockId = b.id;
            h.dataset.blockType = "heading";
            h.setAttribute("dir", b.dir || "ltr");
            if (units[0]) h.appendChild(renderUnitDom(units[0], { inlineHeading: true }));
            mainClone.appendChild(h);
          } else {
            var sec = document.createElement("section");
            sec.id = b.id;
            sec.className = "mx-block mx-" + b.kind;
            sec.dataset.blockId = b.id;
            sec.dataset.blockType = b.kind;
            sec.setAttribute("dir", b.dir || "ltr");
            if (b.proves) sec.dataset.proves = b.proves;
            if (b.kind !== "text") sec.setAttribute("aria-label", b.title || b.label || b.kind);
            if (b.kind !== "text") {
              var head = document.createElement("header");
              head.className = "mx-block-head";
              var badge = document.createElement("span");
              badge.className = "mx-badge";
              badge.textContent = b.label || b.kind;
              head.appendChild(badge);
              if (b.proves && b.provesTitle) {
                var prov = document.createElement("a");
                prov.className = "mx-proves";
                prov.setAttribute("href", "#" + b.proves);
                prov.textContent = b.provesTitle;
                head.appendChild(prov);
              }
              var slot = document.createElement("span");
              slot.className = "mx-actions";
              slot.dataset.blockActions = b.id;
              head.appendChild(slot);
              sec.appendChild(head);
            }
            var bodyDiv = document.createElement("div");
            bodyDiv.className = "mx-block-body";
            appendUnitsMerged(units, bodyDiv);
            appendPreservedFigures(b, bodyDiv);
            sec.appendChild(bodyDiv);
            mainClone.appendChild(sec);
          }
        }
      }

      var tocListClone = docClone.querySelector("#mx-toc .mx-toc-list");
      if (tocListClone) {
        var liveToc = document.querySelector("#mx-toc .mx-toc-list");
        if (liveToc) {
          tocListClone.textContent = "";
          liveToc.querySelectorAll(".mx-toc-item").forEach(function (item) {
            var cItem = item.cloneNode(true);
            cItem.querySelectorAll(".mx-toc-fold").forEach(function (f) { f.remove(); });
            cItem.removeAttribute("hidden");
            delete cItem.dataset.mxFolded;
            tocListClone.appendChild(cItem);
          });
        }
      }

      var srcClone = docClone.querySelector("#mx-source-data");
      if (srcClone) {
        var nextSource = cloneData(sourceData);
        nextSource.blocks = blocks.map(function (b) {
          var copy = cloneData(b);
          delete copy.edited;
          delete copy._origId;
          return copy;
        });
        srcClone.textContent = JSON.stringify(nextSource);
      }

      return "<!DOCTYPE html>\n" + docClone.outerHTML;
    }

    function suggestedName(ext) {
      var title = (document.title || "document").replace(/[\\/:*?"<>|]/g, "_").trim() || "document";
      return title + "." + ext;
    }

    function triggerDownload(filename, content, mime) {
      var blob = new Blob([content], { type: mime });
      var url = URL.createObjectURL(blob);
      var a = document.createElement("a");
      a.href = url;
      a.download = filename;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      window.setTimeout(function () { URL.revokeObjectURL(url); }, 1000);
    }

    function downloadStandaloneHtml() {
      var html = serializeStandaloneHtml();
      triggerDownload(suggestedName("html"), html, "text/html;charset=utf-8");
      baselineBlocks = cloneData(blocks);
      dirty = false;
      banner.hidden = true;
      try { window.localStorage.removeItem(draftKey); } catch (err) { /* ignore */ }
      setStatus(say("Downloaded HTML", "فایل HTML دانلود شد"), false);
      updateBar();
    }

    function exportMarkdownFile() {
      var md = blocks.map(function (b) { return b.md.trim(); }).filter(Boolean).join("\n\n") + "\n";
      triggerDownload(suggestedName("md"), md, "text/markdown;charset=utf-8");
      setStatus(say("Exported .md", "فایل Markdown ذخیره شد"), false);
    }

    async function saveStandaloneBrowser() {
      var html = serializeStandaloneHtml();
      if (typeof window.showSaveFilePicker === "function") {
        try {
          if (!fileHandle) {
            fileHandle = await window.showSaveFilePicker({
              suggestedName: suggestedName("html"),
              types: [{ description: "HTML Document", accept: { "text/html": [".html", ".htm"] } }]
            });
          }
          var writable = await fileHandle.createWritable();
          await writable.write(html);
          await writable.close();
          baselineBlocks = cloneData(blocks);
          dirty = false;
          banner.hidden = true;
          try { window.localStorage.removeItem(draftKey); } catch (err) { /* ignore */ }
          setStatus(say("Saved to file", "در فایل ذخیره شد"), false);
          updateBar();
          return;
        } catch (err) {
          if (err && err.name === "AbortError") return;
          fileHandle = null;
        }
      }
      downloadStandaloneHtml();
    }

    function revertBrowserDraft() {
      commitActiveUnit(false);
      blocks = cloneData(baselineBlocks);
      undoStack = [];
      redoStack = [];
      dirty = false;
      banner.hidden = true;
      try { window.localStorage.removeItem(draftKey); } catch (err) { /* ignore */ }
      rebuildAllUnits();
      syncSourceDataElement();
      renderTocFromBlocks();
      renderAllBlocks(editMode);
      setStatus(say("Reverted", "بازگردانی شد"), false);
      updateBar();
    }

    function undo() {
      if (framed) {
        window.parent.postMessage({ mathia: "undo" }, "*");
        return;
      }
      commitActiveUnit(false);
      if (!undoStack.length) return;
      redoStack.push(cloneData(blocks));
      blocks = undoStack.pop();
      dirty = !isSameBlocks(blocks, baselineBlocks);
      rebuildAllUnits();
      syncSourceDataElement();
      renderTocFromBlocks();
      renderAllBlocks(editMode);
      setStatus(dirty ? say("Unsaved changes", "تغییرات ذخیره‌نشده") : say("Live Preview", "پیش‌نمایش زنده"), dirty);
      scheduleDraftSave();
      updateBar();
    }

    function redo() {
      if (framed) {
        window.parent.postMessage({ mathia: "redo" }, "*");
        return;
      }
      commitActiveUnit(false);
      if (!redoStack.length) return;
      undoStack.push(cloneData(blocks));
      blocks = redoStack.pop();
      dirty = !isSameBlocks(blocks, baselineBlocks);
      rebuildAllUnits();
      syncSourceDataElement();
      renderTocFromBlocks();
      renderAllBlocks(editMode);
      setStatus(dirty ? say("Unsaved changes", "تغییرات ذخیره‌نشده") : say("Live Preview", "پیش‌نمایش زنده"), dirty);
      scheduleDraftSave();
      updateBar();
    }

    /* ── Edit Mode Switching ───────────────────────────────────────────────── */

    function setEditMode(enabled) {
      var next = Boolean(enabled);
      if (next === editMode) return;
      if (!next) {
        commitActiveUnit(false);
      } else if (window.Mathia && window.Mathia.mindmap && typeof window.Mathia.mindmap.setMode === "function") {
        var view = document.body.dataset.mxView || "document";
        if (view !== "document" && view !== "split") {
          window.Mathia.mindmap.setMode("document");
        }
      }
      editMode = next;
      document.body.classList.toggle("mx-editing", editMode);
      editToggle.setAttribute("aria-pressed", String(editMode));
      renderAllBlocks(editMode);
      if (editMode) {
        setStatus(dirty ? say("Unsaved changes", "تغییرات ذخیره‌نشده") : say("Live Preview", "پیش‌نمایش زنده"), dirty);
      } else {
        setStatus("", false);
      }
      updateBar();
      if (framed) {
        window.parent.postMessage({ mathia: "edit-mode-changed", enabled: editMode }, "*");
      }
    }

    editToggle.addEventListener("click", function () {
      setEditMode(!editMode);
    });

    document.addEventListener("keydown", function (e) {
      var key = e.key ? e.key.toLowerCase() : "";
      if ((e.ctrlKey || e.metaKey) && e.shiftKey && !e.altKey && key === "e") {
        e.preventDefault();
        setEditMode(!editMode);
        return;
      }
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && key === "s" && (editMode || dirty)) {
        e.preventDefault();
        flushCommit(true);
        return;
      }
      if (!framed && editMode && !active && (e.ctrlKey || e.metaKey) && !e.altKey && (key === "z" || key === "y")) {
        var targetTag = (e.target && e.target.tagName) || "";
        if (!/^(INPUT|TEXTAREA|SELECT)$/.test(targetTag) && !(e.target && e.target.isContentEditable)) {
          e.preventDefault();
          if (key === "y" || e.shiftKey) redo();
          else undo();
        }
      }
    });

    if (framed) {
      window.addEventListener("message", function (e) {
        if (e.source !== window.parent || !e.data) return;
        if (e.data.mathia === "set-edit-mode") {
          setEditMode(e.data.enabled);
        } else if (e.data.mathia === "toggle-edit") {
          setEditMode(!editMode);
        } else if (e.data.mathia === "edit-synced") {
          applyHostSync(e.data);
        }
      });
    } else {
      try {
        var savedRaw = window.localStorage.getItem(draftKey);
        if (savedRaw) {
          var saved = JSON.parse(savedRaw);
          if (saved && Array.isArray(saved.blocks) && !isSameBlocks(saved.blocks, baselineBlocks)) {
            blocks = saved.blocks;
            dirty = true;
            banner.hidden = false;
            rebuildAllUnits();
            syncSourceDataElement();
            renderTocFromBlocks();
            renderAllBlocks(false);
            setStatus(say("Restored draft", "پیش‌نویس بازیابی شد"), true);
            updateBar();
          }
        }
      } catch (err) { /* ignore invalid draft */ }
    }

    if (window.Mathia) {
      window.Mathia.editor = {
        isEditing: function () { return editMode; },
        setEditMode: setEditMode,
        getBlocks: function () { return cloneData(blocks); },
        serializeHtml: serializeStandaloneHtml,
        _internal: {
          splitMarkdownUnits: splitMarkdownUnits,
          joinMarkdownUnits: joinMarkdownUnits,
          classifyClientBlock: classifyClientBlock,
          safeUrl: safeUrl
        }
      };
    }
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", start);
  } else {
    start();
  }
})();
