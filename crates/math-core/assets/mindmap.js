/* Deterministic tree supplied by Rust. No requests, inference, or dependencies. */
(function () {
  "use strict";

  function start() {
    var data = document.getElementById("mx-mindmap-data");
    if (!data || !window.Mathia) return;

    var raw = JSON.parse(data.textContent);
    var tree = raw.root || raw;
    var dependencyEdges = raw.edges || [];
    var rtl = document.documentElement.dir === "rtl";
    if (document.body.dataset.mxAi) document.body.classList.add("mx-map-has-ai");

    var main = document.getElementById("mx-main");
    var ns = "http:" + "//www.w3.org/2000/svg";
    var CARD_WIDTH = 280;
    var COLUMN_STEP = 440;

    var mode = "document", scale = 1, tx = 24, ty = 24;
    var userMovedCamera = false;
    var collapsed = new Set();
    var entries = new Map();
    var bounds = { x: 0, y: 0, width: 1, height: 1 };
    var connections = [];
    var depConnections = [];
    var extraViews = {};
    var showDependencies = false;
    var selectedId = null;

    var say = function (en, fa) { return rtl ? fa : en; };

    // Branches start closed: only the root is visible until the reader expands.
    function collapseAll() {
      entries.forEach(function (entry) {
        if ((entry.node.children || []).length) collapsed.add(entry.node.id);
      });
    }

    function element(tag, cls, text) {
      var el = document.createElement(tag);
      if (cls) el.className = cls;
      if (text !== undefined) el.textContent = text;
      return el;
    }

    function svgElement(tag) { return document.createElementNS(ns, tag); }

    function button(parent, label, action, title) {
      var b = element("button", "", label);
      b.type = "button";
      b.title = title || label;
      b.setAttribute("aria-label", title || label);
      b.addEventListener("click", action);
      parent.appendChild(b);
      return b;
    }

    // --- 1. View Switcher ---
    var switcher = element("div", "mx-map-switch");
    switcher.setAttribute("role", "group");
    switcher.setAttribute("aria-label", say("Document view", "نمای سند"));

    var modes = {};
    [
      ["document", "Document", "سند"],
      ["split", "Split view", "نمای دوگانه"],
      ["map", "Mind map", "نقشه ذهنی"]
    ].forEach(function (item) {
      modes[item[0]] = button(switcher, say(item[1], item[2]), function () { setMode(item[0]); });
    });

    function registerView(id, en, fa, viewPanel, onShow, onHide) {
      extraViews[id] = { panel: viewPanel, onShow: onShow, onHide: onHide };
      modes[id] = button(switcher, say(en, fa), function () { setMode(id); });
      modes[id].setAttribute("aria-pressed", "false");
    }
    document.body.appendChild(switcher);

    // --- 2. Panel & Toolbar ---
    var panel = element("section", "mx-map-panel");
    panel.hidden = true;
    panel.setAttribute("aria-label", say("Mind map", "نقشه ذهنی"));

    var toolbar = element("div", "mx-map-toolbar");
    panel.appendChild(toolbar);

    var viewport = element("div", "mx-map-canvas");
    viewport.setAttribute("aria-label", say("Mind map. Drag to pan; scroll to zoom.", "نقشه ذهنی؛ کشیدن برای جابجایی و پیمایش برای بزرگ‌نمایی"));

    var world = element("div", "mx-map-world");
    var svg = svgElement("svg");
    var edges = svgElement("g");
    var depEdges = svgElement("g");
    var cards = element("div", "mx-map-cards");

    svg.classList.add("mx-map-links");
    svg.style.overflow = "visible";
    svg.setAttribute("aria-hidden", "true");
    svg.appendChild(edges);
    svg.appendChild(depEdges);
    world.append(svg, cards);
    viewport.appendChild(world);
    panel.appendChild(viewport);

    var status = element("span", "mx-map-status");
    status.setAttribute("role", "status");
    panel.appendChild(status);
    document.body.appendChild(panel);

    button(toolbar, "+", function () { zoom(1.2); }, say("Zoom in", "بزرگ‌نمایی"));
    button(toolbar, "−", function () { zoom(1 / 1.2); }, say("Zoom out", "کوچک‌نمایی"));
    button(toolbar, say("Fit", "جا دادن"), function () { userMovedCamera = false; fit(); });
    button(toolbar, say("Reset layout", "بازنشانی چیدمان"), function () {
      entries.forEach(function (entry) { entry.position = null; });
      userMovedCamera = false;
      layout();
      fit();
    });
    button(toolbar, say("Expand all", "باز کردن همه"), function () {
      collapsed.clear();
      userMovedCamera = false;
      layout();
      fit();
    });
    button(toolbar, say("Collapse all", "بستن همه"), function () {
      collapseAll();
      userMovedCamera = false;
      layout();
      fit();
    });

    var depBtn = button(toolbar, say("Show Dependencies", "نمایش وابستگی‌ها"), function () {
      showDependencies = !showDependencies;
      depBtn.setAttribute("aria-pressed", String(showDependencies));
      depBtn.classList.toggle("mx-map-btn-active", showDependencies);
      rebuildDependencies();
    }, say("Show Dependencies", "نمایش وابستگی‌ها"));
    depBtn.setAttribute("aria-pressed", "false");

    button(toolbar, "SVG ↓", function () { exportMap(false); }, say("Export mind map as SVG", "خروجی SVG نقشه ذهنی"));
    button(toolbar, "PNG ↓", function () { exportMap(true); }, say("Export mind map as PNG", "خروجی PNG نقشه ذهنی"));

    // --- 3. Mount Cards ---
    function label(parent, text) {
      var at = 0;
      function dollar(index) {
        if (text[index] !== "$") return false;
        var backslashes = 0;
        while (index > 0 && text[--index] === "\\") backslashes++;
        return backslashes % 2 === 0;
      }
      for (var i = 0; i < text.length; i++) {
        if (!dollar(i)) continue;
        var end = i + 1;
        while (end < text.length && !dollar(end)) end++;
        if (end === text.length) break;
        parent.appendChild(document.createTextNode(text.slice(at, i)));
        var span = element("span", "mx-map-math");
        if (window.katex && window.katex.renderToString) {
          span.innerHTML = window.katex.renderToString(text.slice(i + 1, end), {
            throwOnError: false, trust: false, displayMode: false, maxExpand: 1000
          });
        } else {
          span.textContent = text.slice(i, end + 1);
        }
        parent.appendChild(span);
        at = end + 1;
        i = end;
      }
      parent.appendChild(document.createTextNode(text.slice(at)));
    }

    var badges = {
      root: ["DOC", "سند"], heading: ["§", "بخش"], definition: ["DEF", "تعریف"],
      theorem: ["THM", "قضیه"], lemma: ["LEM", "لم"], corollary: ["COR", "نتیجه"],
      proposition: ["PROP", "گزاره"], example: ["EX", "مثال"], formula: ["ƒ", "فرمول"], note: ["NOTE", "نکته"]
    };

    function mount(node, ancestors) {
      var children = node.children || [];
      var type = (node.node_type || "").toLowerCase();
      var group = element("div", "mx-map-node");
      group.dataset.mapId = node.id;

      var card = element("div", "mx-map-card mx-map-" + type);
      card.setAttribute("dir", "auto");

      var body = button(card, "", function () { navigate(node.id); }, node.label);
      body.className = "mx-map-label";
      body.title = node.label + " — " + say(
        "Drag to move; click to read. Alt + arrow keys also move this card.",
        "برای جابجایی بکشید؛ برای خواندن کلیک کنید. کلیدهای Alt و جهت نیز کارت را جابجا می‌کنند."
      );
      body.setAttribute("aria-keyshortcuts", "Alt+ArrowUp Alt+ArrowDown Alt+ArrowLeft Alt+ArrowRight");

      var badge = badges[type] || badges.note;
      body.appendChild(element("span", "mx-map-badge", say(badge[0], badge[1])));
      var title = element("span", "mx-map-title");
      label(title, node.label || "");
      body.appendChild(title);

      if (node.subtitle) {
        var sub = element("span", "mx-map-subtitle");
        label(sub, node.subtitle);
        body.appendChild(sub);
      }

      if (node.key_terms && node.key_terms.length) {
        var termsBox = element("div", "mx-map-key-terms");
        node.key_terms.forEach(function (term) {
          var termPill = element("span", "mx-map-term");
          label(termPill, term);
          termsBox.appendChild(termPill);
        });
        body.appendChild(termsBox);
      }

      if (node.proof_id) {
        var proofPill = element("span", "mx-map-proof", say("Proof ↗", "اثبات ↗"));
        proofPill.setAttribute("role", "button");
        proofPill.setAttribute("tabindex", "0");
        proofPill.setAttribute("title", say("Jump to proof", "رفتن به اثبات"));
        proofPill.addEventListener("click", function (e) {
          e.stopPropagation();
          navigate(node.proof_id);
        });
        proofPill.addEventListener("keydown", function (e) {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            e.stopPropagation();
            navigate(node.proof_id);
          }
        });
        body.appendChild(proofPill);
      }

      var toggle = null;
      if (children.length) {
        toggle = button(card, "−", function () {
          if (collapsed.has(node.id)) collapsed.delete(node.id);
          else collapsed.add(node.id);
          layout();
        }, say("Expand or collapse branch", "باز و بسته کردن شاخه"));
        toggle.className = "mx-map-fold";
      }

      group.appendChild(card);
      cards.appendChild(group);

      entries.set(node.id, {
        node: node,
        ancestors: ancestors,
        group: group,
        card: card,
        body: body,
        toggle: toggle,
        height: 100,
        x: 0, y: 0,
        position: null,
        visible: true,
        edges: [],
        depEdges: []
      });

      children.forEach(function (child) { mount(child, ancestors.concat(node.id)); });
    }
    mount(tree, []);
    collapseAll();

    // --- 4. Layout & Edge Geometry ---
    function gapBefore(children, index) {
      if (!index) return 0;
      var previous = children[index - 1], current = children[index];
      var prevType = (previous.node_type || "").toLowerCase();
      var currType = (current.node_type || "").toLowerCase();
      return prevType === "heading" || currType === "heading"
        || (previous.children || []).length || (current.children || []).length ? 88 : 48;
    }

    function updateTreeEdgePath(edge) {
      var x = edge.from.x + CARD_WIDTH, y = edge.from.y + edge.from.height / 2;
      var endX = edge.to.x, endY = edge.to.y + edge.to.height / 2;
      var bend = Math.max(25, Math.min(160, Math.abs(endX - x) / 2));
      edge.path.setAttribute("d", "M" + x + "," + y + " C" + (x + bend) + "," + y + " " + (endX - bend) + "," + endY + " " + endX + "," + endY);
      edge.left = Math.min(x, endX, endX - bend);
      edge.right = Math.max(x, endX, x + bend);
    }

    function updateDepEdgePath(edge) {
      var from = edge.from, to = edge.to;
      var startY = from.y + from.height / 2;
      var endY = to.y + to.height / 2;
      var startX, endX;
      if (from.x < to.x) {
        startX = from.x + CARD_WIDTH;
        endX = to.x;
        var dx = Math.max(30, (endX - startX) / 2);
        edge.path.setAttribute("d", "M" + startX + "," + startY + " C" + (startX + dx) + "," + startY + " " + (endX - dx) + "," + endY + " " + endX + "," + endY);
      } else {
        startX = from.x;
        endX = to.x + CARD_WIDTH;
        var bend = Math.max(40, Math.min(180, Math.abs(startY - endY) * 0.4 + 40));
        edge.path.setAttribute("d", "M" + startX + "," + startY + " C" + (startX - bend) + "," + startY + " " + (endX + bend) + "," + endY + " " + endX + "," + endY);
      }
    }

    function positionCard(entry) {
      entry.group.style.left = entry.x + "px";
      entry.group.style.top = entry.y + "px";
    }

    function rebuildDependencies() {
      depEdges.replaceChildren();
      depConnections = [];
      entries.forEach(function (e) { e.depEdges.length = 0; });
      if (!showDependencies) return;

      dependencyEdges.forEach(function (edge) {
        var from = entries.get(edge.from);
        var to = entries.get(edge.to);
        if (!from || !to || !from.visible || !to.visible) return;

        var path = svgElement("path");
        path.className = "mx-map-edge mx-map-dep-edge";
        path.setAttribute("class", "mx-map-edge mx-map-dep-edge");
        path.setAttribute("stroke-dasharray", "6,4");
        path.dataset.depFrom = edge.from;
        path.dataset.depTo = edge.to;
        if (edge.label) {
          var titleEl = svgElement("title");
          titleEl.textContent = edge.label;
          path.appendChild(titleEl);
        }

        var conn = { from: from, to: to, path: path };
        updateDepEdgePath(conn);
        depEdges.appendChild(path);
        depConnections.push(conn);
        from.depEdges.push(conn);
        to.depEdges.push(conn);
      });
    }

    function updateBounds() {
      var left = Infinity, top = Infinity, right = -Infinity, bottom = -Infinity;
      entries.forEach(function (entry) {
        if (!entry.visible) return;
        left = Math.min(left, entry.x);
        top = Math.min(top, entry.y);
        right = Math.max(right, entry.x + CARD_WIDTH);
        bottom = Math.max(bottom, entry.y + entry.height);
      });
      connections.forEach(function (edge) {
        left = Math.min(left, edge.left);
        right = Math.max(right, edge.right);
      });
      if (!isFinite(left)) return;
      bounds = { x: left, y: top, width: Math.max(1, right - left), height: Math.max(1, bottom - top) };
      world.style.width = bounds.width + "px";
      world.style.height = bounds.height + "px";
      svg.setAttribute("width", String(bounds.width));
      svg.setAttribute("height", String(bounds.height));
    }

    function layout() {
      edges.replaceChildren();
      connections = [];

      // Pass 1 (write): unhide all cards for measurement
      entries.forEach(function (e) {
        e.group.style.display = "";
        e.edges.length = 0;
      });
      // Pass 2 (read): batch scrollHeight reads to avoid layout thrashing
      entries.forEach(function (e) {
        e.height = Math.max(72, e.card.scrollHeight + 4);
      });
      // Pass 3 (write): apply heights and fold indicators
      entries.forEach(function (e) {
        e.group.style.height = e.height + "px";
        if (e.toggle) {
          var isCollapsed = collapsed.has(e.node.id);
          e.toggle.textContent = isCollapsed ? "+" : "−";
          e.toggle.setAttribute("aria-expanded", String(!isCollapsed));
        }
      });

      function measure(node) {
        var e = entries.get(node.id);
        e.visibleChildren = collapsed.has(node.id) ? [] : (node.children || []);
        e.space = Math.max(e.height, e.visibleChildren.reduce(function (sum, child, index, children) {
          return sum + gapBefore(children, index) + measure(child);
        }, 0));
        return e.space;
      }
      measure(tree);

      var visible = new Set();
      function place(node, depth, top) {
        var e = entries.get(node.id);
        visible.add(node.id);
        e.x = e.position ? e.position.x : depth * COLUMN_STEP;
        e.y = e.position ? e.position.y : top + (e.space - e.height) / 2;
        positionCard(e);

        var offset = top;
        e.visibleChildren.forEach(function (child, index, children) {
          offset += gapBefore(children, index);
          var c = entries.get(child.id);
          var path = svgElement("path");
          path.setAttribute("class", "mx-map-edge");
          path.dataset.mapFrom = node.id;
          path.dataset.mapTo = child.id;
          edges.appendChild(path);

          var conn = { from: e, to: c, path: path, left: 0, right: 0 };
          connections.push(conn);
          e.edges.push(conn);
          c.edges.push(conn);

          place(child, depth + 1, offset);
          offset += c.space;
        });
      }
      place(tree, 0, 0);

      entries.forEach(function (e, id) {
        e.visible = visible.has(id);
        e.group.style.display = e.visible ? "" : "none";
      });

      connections.forEach(updateTreeEdgePath);
      updateBounds();
      rebuildDependencies();
      transform();
    }

    // O(degree) update while dragging a card
    function moveCard(entry, x, y) {
      entry.position = { x: x, y: y };
      entry.x = x;
      entry.y = y;
      positionCard(entry);
      entry.edges.forEach(updateTreeEdgePath);
      if (showDependencies) entry.depEdges.forEach(updateDepEdgePath);
      updateBounds();
    }

    // --- 5. Camera ---
    function transform() {
      world.style.transform = "translate(" + tx + "px," + ty + "px) scale(" + scale + ")";
    }

    function zoom(factor, x, y) {
      x = x === undefined ? viewport.clientWidth / 2 : x;
      y = y === undefined ? viewport.clientHeight / 2 : y;
      var next = Math.max(0.05, Math.min(3, scale * factor));
      tx = x - ((x - tx) * next) / scale;
      ty = y - ((y - ty) * next) / scale;
      scale = next;
      userMovedCamera = true;
      transform();
    }

    function fit() {
      if (panel.hidden) return;
      scale = Math.max(0.05, Math.min(1.3, (viewport.clientWidth - 48) / bounds.width, (viewport.clientHeight - 48) / bounds.height));
      tx = (viewport.clientWidth - bounds.width * scale) / 2 - bounds.x * scale;
      ty = (viewport.clientHeight - bounds.height * scale) / 2 - bounds.y * scale;
      transform();
    }

    // --- 6. Document Sync & Modes ---
    var observer;
    function watchDocument() {
      if (observer) observer.disconnect();
      if (!window.IntersectionObserver || (mode !== "document" && mode !== "split")) return;
      observer = new IntersectionObserver(function (changes) {
        var visible = changes.filter(function (e) { return e.isIntersecting; });
        if (visible.length) select(visible[0].target.id);
      }, { root: mode === "split" ? main : null, rootMargin: "0px 0px -45% 0px" });
      entries.forEach(function (_, id) {
        var target = document.getElementById(id);
        if (target) observer.observe(target);
      });
    }

    function select(id) {
      if (selectedId === id) return;
      if (selectedId) {
        var prev = entries.get(selectedId);
        if (prev) {
          prev.card.classList.remove("mx-map-selected");
          prev.body.setAttribute("aria-current", "false");
        }
      }
      selectedId = id;
      var next = entries.get(id);
      if (next) {
        next.card.classList.add("mx-map-selected");
        next.body.setAttribute("aria-current", "true");
      }
    }

    function setMode(next) {
      var previous = mode, extra = extraViews[next];
      mode = next;
      document.body.dataset.mxView = mode;
      panel.hidden = mode === "document" || !!extra;
      main.hidden = mode !== "document" && mode !== "split";
      Object.keys(extraViews).forEach(function (id) { extraViews[id].panel.hidden = id !== mode; });
      if (previous !== mode) {
        var leaving = extraViews[previous];
        if (leaving && leaving.onHide) leaving.onHide();
        if (extra && extra.onShow) extra.onShow();
      }
      Object.keys(modes).forEach(function (key) {
        modes[key].setAttribute("aria-pressed", String(key === mode));
      });
      if (mode === "split" || mode === "map") {
        userMovedCamera = false;
        layout();
        fit();
      }
      watchDocument();
    }

    function navigate(id) {
      if (mode !== "document" && mode !== "split") setMode("split");
      var target = document.getElementById(id);
      if (!target) return;
      target.scrollIntoView({ behavior: "smooth", block: "center" });
      target.classList.remove("mathia-scroll-highlight");
      void target.offsetWidth;
      target.classList.add("mathia-scroll-highlight");
      window.setTimeout(function () { target.classList.remove("mathia-scroll-highlight"); }, 1500);
      select(id);
      if (window.parent !== window) window.parent.postMessage({ mathia: "select", id: id }, "*");
    }

    main.addEventListener("click", function (event) {
      var target = event.target.closest("[data-block-id]");
      if (!target || !entries.has(target.id)) return;
      var e = entries.get(target.id), changed = false;
      e.ancestors.forEach(function (id) { if (collapsed.delete(id)) changed = true; });
      if (changed && mode !== "document") layout();
      select(target.id);
    });

    // --- 7. Pointer, Wheel & Keyboard Interaction ---
    var drag = null, suppressClick = false;

    viewport.addEventListener("wheel", function (event) {
      event.preventDefault();
      if (drag) return;
      var rect = viewport.getBoundingClientRect();
      zoom(Math.exp(-event.deltaY * 0.0015), event.clientX - rect.left, event.clientY - rect.top);
    }, { passive: false });

    viewport.addEventListener("pointerdown", function (event) {
      if (event.button !== 0 || drag) return;
      suppressClick = false;
      if (event.target.closest(".mx-map-fold") || event.target.closest(".mx-map-proof")) return;
      var node = event.target.closest(".mx-map-node");
      var entry = node ? entries.get(node.dataset.mapId) : null;
      drag = {
        pointerId: event.pointerId,
        x: event.clientX, y: event.clientY,
        tx: tx, ty: ty,
        entry: entry,
        originX: entry ? entry.x : 0,
        originY: entry ? entry.y : 0,
        hadPosition: entry ? !!entry.position : false,
        moved: false
      };
      if (!entry) viewport.setPointerCapture(event.pointerId);
    });

    viewport.addEventListener("pointermove", function (event) {
      if (!drag || event.pointerId !== drag.pointerId) return;
      var dx = event.clientX - drag.x, dy = event.clientY - drag.y;
      if (!drag.moved && Math.hypot(dx, dy) < 4) return;
      if (!drag.moved) {
        drag.moved = true;
        suppressClick = true;
        viewport.setPointerCapture(event.pointerId);
        viewport.classList.add("mx-map-drag-active");
        if (drag.entry) drag.entry.group.classList.add("mx-map-dragging");
      }
      event.preventDefault();
      if (drag.entry) {
        moveCard(drag.entry, drag.originX + dx / scale, drag.originY + dy / scale);
      } else {
        tx = drag.tx + dx;
        ty = drag.ty + dy;
        userMovedCamera = true;
        transform();
      }
    });

    function endDrag(event) {
      if (!drag || (event && event.pointerId !== drag.pointerId)) return;
      if (drag.entry) drag.entry.group.classList.remove("mx-map-dragging");
      viewport.classList.remove("mx-map-drag-active");
      drag = null;
    }
    ["pointerup", "pointercancel", "lostpointercapture"].forEach(function (name) {
      viewport.addEventListener(name, endDrag);
    });

    viewport.addEventListener("click", function (event) {
      if (!suppressClick || event.detail === 0) return;
      suppressClick = false;
      event.preventDefault();
      event.stopPropagation();
    }, true);

    viewport.addEventListener("keydown", function (event) {
      if (event.key === "Escape" && drag) {
        if (drag.entry) {
          moveCard(drag.entry, drag.originX, drag.originY);
          if (!drag.hadPosition) drag.entry.position = null;
        } else {
          tx = drag.tx;
          ty = drag.ty;
          transform();
        }
        endDrag(null);
        return;
      }
      var deltas = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };
      if (!event.altKey || !deltas[event.key] || drag) return;
      var node = event.target.closest(".mx-map-node");
      if (!node) return;
      event.preventDefault();
      var entry = entries.get(node.dataset.mapId);
      var step = event.shiftKey ? 50 : 20;
      moveCard(entry, entry.x + deltas[event.key][0] * step, entry.y + deltas[event.key][1] * step);
    });

    if (window.ResizeObserver) {
      new ResizeObserver(function () {
        if (!panel.hidden && !userMovedCamera) fit();
      }).observe(viewport);
    }

    // --- 8. SVG / PNG Export ---
    function download(blob, extension) {
      if (window.parent !== window) {
        var reader = new FileReader();
        reader.onload = function () {
          window.parent.postMessage({ mathia: "mindmap-export", data: reader.result, extension: extension }, "*");
        };
        reader.readAsDataURL(blob);
        return;
      }
      var url = URL.createObjectURL(blob), a = element("a");
      a.href = url;
      a.download = "mathia-mindmap." + extension;
      a.click();
      window.setTimeout(function () { URL.revokeObjectURL(url); }, 30000);
    }

    async function exportMap(png) {
      try {
        if (document.fonts) await document.fonts.ready;
        var copy = svgElement("svg");
        var w = Math.ceil(bounds.width + 48), h = Math.ceil(bounds.height + 48);
        var bg = getComputedStyle(document.body).backgroundColor;
        copy.setAttribute("xmlns", ns);
        copy.setAttribute("width", String(w));
        copy.setAttribute("height", String(h));
        copy.setAttribute("viewBox", (bounds.x - 24) + " " + (bounds.y - 24) + " " + w + " " + h);
        copy.style.cssText = "background:" + bg;

        // Embed page stylesheets directly instead of cloning 300+ computed styles per node
        var style = svgElement("style");
        var cssText = Array.from(document.querySelectorAll("style")).map(function (el) {
          return el.textContent;
        }).join("\n");
        style.textContent = cssText + "\n* { transition: none !important; animation: none !important; }";
        copy.appendChild(style);

        var lines = edges.cloneNode(true);
        lines.style.transform = "none";
        copy.appendChild(lines);

        if (showDependencies) {
          var depLines = depEdges.cloneNode(true);
          depLines.style.transform = "none";
          copy.appendChild(depLines);
        }

        entries.forEach(function (entry) {
          if (!entry.visible) return;
          var fo = svgElement("foreignObject");
          fo.setAttribute("x", String(entry.x));
          fo.setAttribute("y", String(entry.y));
          fo.setAttribute("width", String(CARD_WIDTH));
          fo.setAttribute("height", String(entry.height));
          var card = entry.card.cloneNode(true);
          card.setAttribute("xmlns", "http://www.w3.org/1999/xhtml");
          fo.appendChild(card);
          copy.appendChild(fo);
        });

        var source = new XMLSerializer().serializeToString(copy);
        if (!png) {
          download(new Blob([source], { type: "image/svg+xml" }), "svg");
          return;
        }

        var ratio = Math.min(2, 8192 / w, 8192 / h, Math.sqrt(16000000 / (w * h)));
        var canvas = element("canvas");
        canvas.width = Math.ceil(w * ratio);
        canvas.height = Math.ceil(h * ratio);

        var image = new Image();
        await new Promise(function (resolve, reject) {
          image.onload = resolve;
          image.onerror = reject;
          image.src = "data:image/svg+xml;charset=utf-8," + encodeURIComponent(source);
        });

        var ctx = canvas.getContext("2d");
        ctx.fillStyle = bg;
        ctx.fillRect(0, 0, canvas.width, canvas.height);
        ctx.drawImage(image, 0, 0, canvas.width, canvas.height);
        canvas.toBlob(function (blob) {
          if (blob) download(blob, "png");
          else status.textContent = say("PNG export failed. Try SVG.", "خروجی PNG ناموفق بود؛ SVG را امتحان کنید.");
        }, "image/png");
      } catch (_) {
        status.textContent = say("Export failed. Try SVG.", "خروجی ناموفق بود؛ SVG را امتحان کنید.");
      }
    }

    window.Mathia.mindmap = {
      tree: tree,
      data: raw,
      setMode: setMode,
      navigate: navigate,
      registerView: registerView,
      updateDependencies: rebuildDependencies
    };

    setMode("document");
    if (document.fonts) {
      document.fonts.ready.then(function () {
        if (!panel.hidden) { layout(); if (!userMovedCamera) fit(); }
      });
    }
  }

  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", start);
  else start();
})();
