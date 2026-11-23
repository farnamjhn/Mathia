/* Obsidian-style force-directed graph over the same deterministic nodes the
   mind map uses. Canvas-rendered with Barnes-Hut quadtree repulsion and live
   damped physics. No requests, inference, or dependencies. */
(function () {
  "use strict";

  function start() {
    var data = document.getElementById("mx-mindmap-data");
    var mm = window.Mathia && window.Mathia.mindmap;
    if (!data || !mm || !mm.registerView) return;

    var raw = JSON.parse(data.textContent);
    var tree = raw.root || raw;
    var rtl = document.documentElement.dir === "rtl";
    var say = function (en, fa) { return rtl ? fa : en; };

    function element(tag, cls, text) {
      var el = document.createElement(tag);
      if (cls) el.className = cls;
      if (text !== undefined) el.textContent = text;
      return el;
    }

    function button(parent, label, action, title) {
      var b = element("button", "", label);
      b.type = "button";
      b.title = title || label;
      b.setAttribute("aria-label", title || label);
      b.addEventListener("click", action);
      parent.appendChild(b);
      return b;
    }

    function shortLabel(label) {
      var text = label.replace(/\$/g, "");
      return text.length > 42 ? text.slice(0, 40) + "…" : text;
    }

    // --- 1. Build graph data (O(N + E)) ---
    var TYPE_COLORS = {
      root: "#f59e0b",
      heading: "#64748b",
      definition: "#a78bfa",
      theorem: "#a78bfa",
      lemma: "#a78bfa",
      corollary: "#a78bfa",
      proposition: "#a78bfa",
      example: "#10b981",
      note: "#94a3b8",
      formula: "#94a3b8"
    };

    var nodes = [];
    var byId = new Map();
    var links = [];
    var seenLinks = new Set();

    function linkKey(a, b) {
      return a.id < b.id ? a.id + "\0" + b.id : b.id + "\0" + a.id;
    }

    function addLink(source, target, kind, label) {
      if (!source || !target || source === target) return;
      var key = linkKey(source, target);
      if (seenLinks.has(key)) return;
      seenLinks.add(key);
      var link = { source: source, target: target, kind: kind, label: label || "" };
      links.push(link);
      source.degree++;
      target.degree++;
      source.neighbors.add(target);
      target.neighbors.add(source);
    }

    function flatten(node, parent) {
      var type = (node.node_type || "note").toLowerCase();
      var entry = {
        id: node.id,
        label: node.label || "",
        shortLabel: shortLabel(node.label || ""),
        lowerLabel: (node.label || "").toLowerCase(),
        type: type,
        color: TYPE_COLORS[type] || "#94a3b8",
        level: node.level || 0,
        parent: parent,
        degree: 0,
        neighbors: new Set(),
        x: 0, y: 0, vx: 0, vy: 0,
        radius: 4,
        hidden: false
      };
      nodes.push(entry);
      byId.set(entry.id, entry);
      if (parent) addLink(parent, entry, "tree", "");
      (node.children || []).forEach(function (child) { flatten(child, entry); });
    }

    flatten(tree, null);
    (raw.edges || []).forEach(function (edge) {
      addLink(byId.get(edge.from), byId.get(edge.to), "ref", edge.label);
    });

    nodes.forEach(function (node) {
      node.radius = node.type === "root"
        ? 10
        : Math.min(15, (node.type === "heading" ? 4.8 : 3.6) + Math.sqrt(node.degree) * 2.1);
    });

    // --- 2. Build UI ---
    var panel = element("section", "mx-graph-panel");
    panel.hidden = true;
    panel.setAttribute("aria-label", say("Graph view", "نمای نمودار"));

    var toolbar = element("div", "mx-graph-toolbar");
    panel.appendChild(toolbar);

    var search = element("input", "mx-graph-search");
    search.type = "search";
    search.autocomplete = "off";
    search.spellcheck = false;
    search.placeholder = say("Filter nodes…", "پالایش گره‌ها…");
    search.setAttribute("aria-label", say("Filter graph nodes", "پالایش گره‌های نمودار"));
    toolbar.appendChild(search);

    var hiddenTypes = new Set();
    [
      { label: say("Sections", "بخش‌ها"), color: "#64748b", types: ["heading"] },
      { label: say("Statements", "گزاره‌ها"), color: "#a78bfa", types: ["definition", "theorem", "lemma", "corollary", "proposition"] },
      { label: say("Examples", "مثال‌ها"), color: "#10b981", types: ["example"] },
      { label: say("Notes", "نکته‌ها"), color: "#94a3b8", types: ["note", "formula"] }
    ].forEach(function (group) {
      var legend = element("label", "mx-graph-legend");
      var box = element("input");
      box.type = "checkbox";
      box.checked = true;
      box.addEventListener("change", function () {
        group.types.forEach(function (type) {
          if (box.checked) hiddenTypes.delete(type);
          else hiddenTypes.add(type);
        });
        applyFilters();
      });
      var swatch = element("span", "mx-graph-swatch");
      swatch.style.background = group.color;
      legend.append(box, swatch, document.createTextNode(group.label));
      toolbar.appendChild(legend);
    });

    var showLabels = true;
    button(toolbar, say("Fit", "جا دادن"), function () { fit(); }, say("Fit graph to the viewport", "جا دادن نمودار در نما"));
    button(toolbar, say("Reset layout", "بازنشانی چیدمان"), function () { resetLayout(); }, say("Re-run the force layout", "اجرای دوبارهٔ چیدمان نیرو"));
    var labelsBtn = button(toolbar, say("Labels", "برچسب‌ها"), function () {
      showLabels = !showLabels;
      labelsBtn.setAttribute("aria-pressed", String(showLabels));
      labelsBtn.classList.toggle("mx-graph-btn-active", showLabels);
      requestDraw();
    }, say("Show or hide node labels", "نمایش یا پنهان کردن برچسب گره‌ها"));
    labelsBtn.setAttribute("aria-pressed", "true");
    labelsBtn.classList.add("mx-graph-btn-active");

    var status = element("span", "mx-graph-status");
    status.setAttribute("role", "status");
    toolbar.appendChild(status);

    var canvasWrap = element("div", "mx-graph-canvas");
    canvasWrap.setAttribute("aria-label", say("Graph. Drag to pan; scroll to zoom; click a node to read.", "نمودار؛ کشیدن برای جابجایی، پیمایش برای بزرگ‌نمایی و کلیک گره برای خواندن"));
    var canvas = element("canvas", "mx-graph-svg");
    canvas.style.width = "100%";
    canvas.style.height = "100%";
    canvas.style.display = "block";
    canvas.tabIndex = 0;
    canvasWrap.appendChild(canvas);
    panel.appendChild(canvasWrap);
    document.body.appendChild(panel);

    var ctx = canvas.getContext("2d");

    // --- 3. Barnes-Hut Quadtree (O(N log N) repulsion) ---
    function Quad(x, y, size) {
      this.x = x;
      this.y = y;
      this.size = size;
      this.mass = 0;
      this.cx = 0;
      this.cy = 0;
      this.body = null;
      this.nw = null;
      this.ne = null;
      this.sw = null;
      this.se = null;
    }

    function insertQuad(q, node, depth) {
      if (q.mass === 0) {
        q.body = node;
        q.mass = 1;
        q.cx = node.x;
        q.cy = node.y;
        return;
      }
      if (depth > 20) {
        // Coincident points guard: accumulate mass without infinite subdivision
        var m = q.mass + 1;
        q.cx = (q.cx * q.mass + node.x) / m;
        q.cy = (q.cy * q.mass + node.y) / m;
        q.mass = m;
        return;
      }
      if (q.body !== null) {
        var existing = q.body;
        q.body = null;
        subdivideInsert(q, existing, depth + 1);
      }
      subdivideInsert(q, node, depth + 1);
      var total = q.mass + 1;
      q.cx = (q.cx * q.mass + node.x) / total;
      q.cy = (q.cy * q.mass + node.y) / total;
      q.mass = total;
    }

    function subdivideInsert(q, node, depth) {
      var half = q.size * 0.5;
      var east = node.x >= q.x + half;
      var south = node.y >= q.y + half;
      if (east) {
        if (south) {
          if (!q.se) q.se = new Quad(q.x + half, q.y + half, half);
          insertQuad(q.se, node, depth);
        } else {
          if (!q.ne) q.ne = new Quad(q.x + half, q.y, half);
          insertQuad(q.ne, node, depth);
        }
      } else {
        if (south) {
          if (!q.sw) q.sw = new Quad(q.x, q.y + half, half);
          insertQuad(q.sw, node, depth);
        } else {
          if (!q.nw) q.nw = new Quad(q.x, q.y, half);
          insertQuad(q.nw, node, depth);
        }
      }
    }

    var THETA_SQ = 0.64; // theta = 0.8
    var REPULSION = 2200;
    var LINK_DIST = 68;
    var LINK_STRENGTH = 0.085;
    var CENTER_STRENGTH = 0.012;
    var DAMPING = 0.82;

    function applyRepulsion(q, node, alpha) {
      if (!q || q.mass === 0) return;
      var dx = node.x - q.cx;
      var dy = node.y - q.cy;
      var d2 = dx * dx + dy * dy;

      if (q.body !== null) {
        if (q.body === node) return;
        if (d2 < 1) {
          dx = (Math.random() - 0.5) * 2;
          dy = (Math.random() - 0.5) * 2;
          d2 = dx * dx + dy * dy + 1;
        }
        var f = (REPULSION * alpha) / (d2 + 16);
        node.vx += dx * f;
        node.vy += dy * f;
        return;
      }

      if ((q.size * q.size) / (d2 + 1) < THETA_SQ) {
        var force = (REPULSION * q.mass * alpha) / (d2 + 16);
        node.vx += dx * force;
        node.vy += dy * force;
        return;
      }

      if (q.nw) applyRepulsion(q.nw, node, alpha);
      if (q.ne) applyRepulsion(q.ne, node, alpha);
      if (q.sw) applyRepulsion(q.sw, node, alpha);
      if (q.se) applyRepulsion(q.se, node, alpha);
    }

    // --- 4. Simulation State & Loop ---
    var viewW = 800, viewH = 600, dpr = 1;
    var scale = 1, tx = 400, ty = 300;
    var alpha = 0;
    var ALPHA_MIN = 0.004;
    var ALPHA_DECAY = 0.985;
    var rafId = 0;
    var needsDraw = false;
    var hovered = null;
    var drag = null;
    var query = "";
    var matchedNodes = new Set();

    function seedPositions() {
      var count = nodes.length;
      var spread = Math.sqrt(Math.max(1, count)) * 28;
      for (var i = 0; i < count; i++) {
        var n = nodes[i];
        var angle = i * 2.399963229728653;
        var r = Math.sqrt(i + 0.5) * (spread / Math.sqrt(count + 1)) * 1.4;
        n.x = Math.cos(angle) * r;
        n.y = Math.sin(angle) * r;
        n.vx = 0;
        n.vy = 0;
      }
    }

    function stepPhysics() {
      var count = nodes.length;
      if (!count) return;

      // 1. Build Barnes-Hut quadtree over visible nodes
      var minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
      for (var i = 0; i < count; i++) {
        var n = nodes[i];
        if (n.hidden) continue;
        if (n.x < minX) minX = n.x;
        if (n.y < minY) minY = n.y;
        if (n.x > maxX) maxX = n.x;
        if (n.y > maxY) maxY = n.y;
      }
      if (!isFinite(minX)) return;

      var size = Math.max(maxX - minX, maxY - minY, 100) + 20;
      var rootQuad = new Quad(minX - 10, minY - 10, size);
      for (var i = 0; i < count; i++) {
        if (!nodes[i].hidden) insertQuad(rootQuad, nodes[i], 0);
      }

      // 2. Repulsion + Center gravity
      for (var i = 0; i < count; i++) {
        var n = nodes[i];
        if (n.hidden || (drag && drag.node === n)) continue;
        applyRepulsion(rootQuad, n, alpha);
        n.vx -= n.x * CENTER_STRENGTH * alpha;
        n.vy -= n.y * CENTER_STRENGTH * alpha;
      }

      // 3. Spring attraction along links
      for (var j = 0; j < links.length; j++) {
        var link = links[j];
        var a = link.source, b = link.target;
        if (a.hidden || b.hidden) continue;
        var dx = b.x - a.x;
        var dy = b.y - a.y;
        var dist = Math.sqrt(dx * dx + dy * dy) || 0.01;
        var targetDist = link.kind === "ref" ? LINK_DIST * 1.35 : LINK_DIST;
        var pull = ((dist - targetDist) / dist) * LINK_STRENGTH * alpha;
        var fx = dx * pull;
        var fy = dy * pull;
        if (!drag || drag.node !== a) { a.vx += fx; a.vy += fy; }
        if (!drag || drag.node !== b) { b.vx -= fx; b.vy -= fy; }
      }

      // 4. Integrate velocity with damping
      for (var i = 0; i < count; i++) {
        var n = nodes[i];
        if (n.hidden || (drag && drag.node === n)) {
          n.vx = 0;
          n.vy = 0;
          continue;
        }
        n.vx *= DAMPING;
        n.vy *= DAMPING;
        // Clamp extreme velocities for stability
        var speedSq = n.vx * n.vx + n.vy * n.vy;
        if (speedSq > 900) {
          var s = 30 / Math.sqrt(speedSq);
          n.vx *= s;
          n.vy *= s;
        }
        n.x += n.vx;
        n.y += n.vy;
      }

      if (!drag || !drag.node) {
        alpha *= ALPHA_DECAY;
        if (alpha < ALPHA_MIN) alpha = 0;
      }
    }

    // --- 5. Canvas Rendering ---
    function getThemeColors() {
      var styles = getComputedStyle(panel);
      var dark = document.documentElement.dataset.theme === "dark"
        || window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches;
      return {
        text: styles.color || (dark ? "#e2e8f0" : "#1e293b"),
        link: dark ? "rgba(148, 163, 184, 0.28)" : "rgba(100, 116, 139, 0.28)",
        linkRef: dark ? "rgba(167, 139, 250, 0.45)" : "rgba(124, 58, 237, 0.42)",
        linkHot: dark ? "rgba(196, 181, 253, 0.92)" : "rgba(109, 40, 217, 0.85)",
        ring: dark ? "#f8fafc" : "#0f172a"
      };
    }

    function draw() {
      if (panel.hidden) return;
      var theme = getThemeColors();

      ctx.save();
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, viewW, viewH);

      ctx.translate(tx, ty);
      ctx.scale(scale, scale);

      var hasHover = !!hovered;
      var hasQuery = !!query;

      // Draw links
      var baseLineWidth = Math.max(0.6, 1.1 / Math.sqrt(scale));
      for (var i = 0; i < links.length; i++) {
        var link = links[i];
        var a = link.source, b = link.target;
        if (a.hidden || b.hidden) continue;

        var isHot = hasHover && (a === hovered || b === hovered);
        var isMatch = !hasQuery || matchedNodes.has(a) || matchedNodes.has(b);

        ctx.beginPath();
        if (link.kind === "ref") ctx.setLineDash([4 / scale, 3 / scale]);
        else ctx.setLineDash([]);

        if (isHot) {
          ctx.strokeStyle = theme.linkHot;
          ctx.lineWidth = baseLineWidth * 1.7;
          ctx.globalAlpha = 1;
        } else {
          ctx.strokeStyle = link.kind === "ref" ? theme.linkRef : theme.link;
          ctx.lineWidth = baseLineWidth;
          ctx.globalAlpha = (hasHover || !isMatch) ? 0.14 : 1;
        }

        ctx.moveTo(a.x, a.y);
        ctx.lineTo(b.x, b.y);
        ctx.stroke();
      }
      ctx.setLineDash([]);

      // Draw nodes
      for (var i = 0; i < nodes.length; i++) {
        var n = nodes[i];
        if (n.hidden) continue;

        var isHovered = n === hovered;
        var isNeighbor = hasHover && hovered.neighbors.has(n);
        var isSearchMatch = hasQuery && matchedNodes.has(n);
        var dimmed = (hasHover && !isHovered && !isNeighbor) || (hasQuery && !isSearchMatch && !isHovered);

        ctx.globalAlpha = dimmed ? 0.18 : 1;
        ctx.beginPath();
        ctx.arc(n.x, n.y, n.radius, 0, Math.PI * 2);
        ctx.fillStyle = n.color;
        ctx.fill();

        if (isHovered || isSearchMatch) {
          ctx.lineWidth = (isHovered ? 2.4 : 1.8) / Math.sqrt(scale);
          ctx.strokeStyle = theme.ring;
          ctx.stroke();
        }
      }

      // Draw labels (Obsidian-style smooth zoom fade)
      if (showLabels) {
        var baseFade = Math.max(0, Math.min(1, (scale - 0.38) / 0.42));
        var fontSize = Math.max(9, Math.min(13, 11.5 / Math.pow(scale, 0.35)));
        ctx.font = "500 " + fontSize + "px system-ui, -apple-system, sans-serif";
        ctx.textAlign = "center";
        ctx.textBaseline = "top";
        ctx.fillStyle = theme.text;

        for (var i = 0; i < nodes.length; i++) {
          var n = nodes[i];
          if (n.hidden) continue;

          var isHovered = n === hovered;
          var isNeighbor = hasHover && hovered.neighbors.has(n);
          var isSearchMatch = hasQuery && matchedNodes.has(n);
          var forceShow = isHovered || isNeighbor || isSearchMatch || n.type === "root";

          var labelAlpha = forceShow ? 1 : baseFade;
          if (hasHover && !isHovered && !isNeighbor) labelAlpha *= 0.12;
          else if (hasQuery && !isSearchMatch && !isHovered) labelAlpha *= 0.12;

          if (labelAlpha <= 0.04) continue;
          ctx.globalAlpha = labelAlpha;
          ctx.fillText(n.shortLabel, n.x, n.y + n.radius + 4 / Math.sqrt(scale));
        }
      }

      ctx.restore();
    }

    function tick() {
      rafId = 0;
      if (panel.hidden) return;
      if (alpha > 0) {
        stepPhysics();
        needsDraw = true;
      }
      if (needsDraw) {
        needsDraw = false;
        draw();
      }
      if (alpha > 0 || (drag && drag.node)) {
        rafId = requestAnimationFrame(tick);
      }
    }

    function wake(heat) {
      if (heat !== undefined) alpha = Math.max(alpha, heat);
      needsDraw = true;
      if (!rafId && !panel.hidden) rafId = requestAnimationFrame(tick);
    }

    function requestDraw() {
      needsDraw = true;
      if (!rafId && !panel.hidden) rafId = requestAnimationFrame(tick);
    }

    // --- 6. Camera & Sizing ---
    function measure() {
      viewW = Math.max(320, canvasWrap.clientWidth || 800);
      viewH = Math.max(240, canvasWrap.clientHeight || 600);
      dpr = Math.min(window.devicePixelRatio || 1, 2);
      canvas.width = Math.round(viewW * dpr);
      canvas.height = Math.round(viewH * dpr);
    }

    function screenToWorld(sx, sy) {
      return { x: (sx - tx) / scale, y: (sy - ty) / scale };
    }

    function hitTest(sx, sy) {
      var pt = screenToWorld(sx, sy);
      var pad = 6 / scale;
      // Reverse loop so top-drawn nodes win
      for (var i = nodes.length - 1; i >= 0; i--) {
        var n = nodes[i];
        if (n.hidden) continue;
        var dx = pt.x - n.x;
        var dy = pt.y - n.y;
        var r = n.radius + pad;
        if (dx * dx + dy * dy <= r * r) return n;
      }
      return null;
    }

    function fit() {
      if (panel.hidden) return;
      measure();
      var minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
      for (var i = 0; i < nodes.length; i++) {
        var n = nodes[i];
        if (n.hidden) continue;
        minX = Math.min(minX, n.x - n.radius);
        minY = Math.min(minY, n.y - n.radius);
        maxX = Math.max(maxX, n.x + n.radius);
        maxY = Math.max(maxY, n.y + n.radius + 16);
      }
      if (!isFinite(minX)) return;
      var width = Math.max(1, maxX - minX);
      var height = Math.max(1, maxY - minY);
      scale = Math.max(0.08, Math.min(1.6, (viewW - 64) / width, (viewH - 64) / height));
      tx = (viewW - width * scale) / 2 - minX * scale;
      ty = (viewH - height * scale) / 2 - minY * scale;
      requestDraw();
    }

    function zoom(factor, x, y) {
      x = x === undefined ? viewW / 2 : x;
      y = y === undefined ? viewH / 2 : y;
      var next = Math.max(0.08, Math.min(4.5, scale * factor));
      tx = x - ((x - tx) * next) / scale;
      ty = y - ((y - ty) * next) / scale;
      scale = next;
      requestDraw();
    }

    function resetLayout() {
      seedPositions();
      // Pre-warm a few frames so the initial view is already readable
      alpha = 1;
      var prewarm = nodes.length > 600 ? 25 : 55;
      for (var i = 0; i < prewarm; i++) stepPhysics();
      fit();
      wake(0.65);
    }

    // --- 7. Filtering ---
    function applyFilters() {
      matchedNodes.clear();
      var visible = 0, found = 0, visibleLinks = 0;
      for (var i = 0; i < nodes.length; i++) {
        var n = nodes[i];
        n.hidden = hiddenTypes.has(n.type);
        if (!n.hidden) {
          visible++;
          if (query && n.lowerLabel.indexOf(query) !== -1) {
            matchedNodes.add(n);
            found++;
          }
        }
      }
      for (var j = 0; j < links.length; j++) {
        if (!links[j].source.hidden && !links[j].target.hidden) visibleLinks++;
      }
      status.textContent = query
        ? say(found + " of " + visible + " nodes", found + " از " + visible + " گره")
        : say(visible + " nodes · " + visibleLinks + " links", visible + " گره · " + visibleLinks + " پیوند");
      requestDraw();
    }

    search.addEventListener("input", function () {
      query = search.value.trim().toLowerCase();
      applyFilters();
    });

    // --- 8. Pointer & Wheel Interaction ---
    canvas.addEventListener("pointerdown", function (event) {
      if (event.button !== 0 || drag) return;
      var rect = canvas.getBoundingClientRect();
      var sx = event.clientX - rect.left;
      var sy = event.clientY - rect.top;
      var node = hitTest(sx, sy);
      drag = {
        pointerId: event.pointerId,
        startX: event.clientX,
        startY: event.clientY,
        tx: tx,
        ty: ty,
        node: node,
        originX: node ? node.x : 0,
        originY: node ? node.y : 0,
        moved: false
      };
      canvas.setPointerCapture(event.pointerId);
    });

    canvas.addEventListener("pointermove", function (event) {
      var rect = canvas.getBoundingClientRect();
      var sx = event.clientX - rect.left;
      var sy = event.clientY - rect.top;

      if (!drag) {
        var hit = hitTest(sx, sy);
        if (hit !== hovered) {
          hovered = hit;
          canvas.style.cursor = hit ? "pointer" : "grab";
          canvas.title = hit ? hit.label : "";
          requestDraw();
        }
        return;
      }

      if (event.pointerId !== drag.pointerId) return;
      var dx = event.clientX - drag.startX;
      var dy = event.clientY - drag.startY;
      if (!drag.moved && Math.hypot(dx, dy) < 4) return;
      drag.moved = true;
      canvasWrap.classList.add("mx-graph-drag-active");

      if (drag.node) {
        drag.node.x = drag.originX + dx / scale;
        drag.node.y = drag.originY + dy / scale;
        drag.node.vx = 0;
        drag.node.vy = 0;
        wake(0.35); // Reheat physics so neighbors follow smoothly
      } else {
        tx = drag.tx + dx;
        ty = drag.ty + dy;
        requestDraw();
      }
    });

    function endDrag(event) {
      if (!drag || event.pointerId !== drag.pointerId) return;
      var finished = drag;
      drag = null;
      canvasWrap.classList.remove("mx-graph-drag-active");
      if (!finished.moved && finished.node) {
        if (finished.node.type === "root") fit();
        else mm.navigate(finished.node.id);
      } else if (finished.node) {
        wake(0.25); // Let graph settle after releasing node
      }
    }

    canvas.addEventListener("pointerup", endDrag);
    canvas.addEventListener("pointercancel", endDrag);
    canvas.addEventListener("lostpointercapture", endDrag);
    canvas.addEventListener("pointerleave", function () {
      if (!drag && hovered) {
        hovered = null;
        canvas.style.cursor = "grab";
        canvas.title = "";
        requestDraw();
      }
    });

    canvas.addEventListener("wheel", function (event) {
      event.preventDefault();
      var rect = canvas.getBoundingClientRect();
      zoom(Math.exp(-event.deltaY * 0.0015), event.clientX - rect.left, event.clientY - rect.top);
    }, { passive: false });

    // --- 9. Lifecycle ---
    var laidOut = false;
    function show() {
      measure();
      if (!laidOut) {
        laidOut = true;
        resetLayout();
      } else {
        fit();
        requestDraw();
      }
    }

    if (window.ResizeObserver) {
      new ResizeObserver(function () {
        if (panel.hidden) return;
        measure();
        requestDraw();
      }).observe(canvasWrap);
    }

    applyFilters();
    mm.registerView("graph", "Graph", "نمودار", panel, show);
  }

  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", start);
  else start();
})();
