/* Deterministic tree supplied by Rust. No requests, inference, or dependencies. */
(function () {
  "use strict";
  function start() {
    var data = document.getElementById("mx-mindmap-data");
    if (!data || !window.Mathia) return;
    var tree = JSON.parse(data.textContent);
    var rtl = document.documentElement.dir === "rtl";
    if (document.body.dataset.mxAi) document.body.classList.add("mx-map-has-ai");
    var main = document.getElementById("mx-main");
    var ns = "http:" + "//www.w3.org/2000/svg";
    var htmlNs = "http:" + "//www.w3.org/1999/xhtml";
    var mode = "document", scale = 1, tx = 24, ty = 24;
    var collapsed = new Set(), entries = new Map(), bounds = { x: 0, y: 0, width: 1, height: 1 }, connections = [];
    var say = function (en, fa) { return rtl ? fa : en; };
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
    var switcher = element("div", "mx-map-switch");
    switcher.setAttribute("role", "group");
    switcher.setAttribute("aria-label", say("Document view", "نمای سند"));
    var modes = {};
    [["document", "Document", "سند"], ["split", "Split view", "نمای دوگانه"], ["map", "Mind map", "نقشه ذهنی"]].forEach(function (item) {
      modes[item[0]] = button(switcher, say(item[1], item[2]), function () { setMode(item[0]); });
    });
    document.body.appendChild(switcher);
    var panel = element("section", "mx-map-panel");
    panel.hidden = true;
    panel.setAttribute("aria-label", say("Mind map", "نقشه ذهنی"));
    var toolbar = element("div", "mx-map-toolbar");
    panel.appendChild(toolbar);
    // Keep interactive HTML outside SVG: WebKit can paint positioned KaTeX
    // descendants of foreignObject at the SVG origin, ignoring pan and zoom.
    // One HTML world transforms both the connector SVG and the card layer.
    var viewport = element("div", "mx-map-canvas");
    viewport.setAttribute("aria-label", say("Mind map. Drag to pan; scroll to zoom.", "نقشه ذهنی؛ کشیدن برای جابجایی و پیمایش برای بزرگ‌نمایی"));
    var world = element("div", "mx-map-world");
    var svg = svgElement("svg"), edges = svgElement("g"), cards = element("div", "mx-map-cards");
    svg.classList.add("mx-map-links");
    svg.setAttribute("aria-hidden", "true");
    svg.appendChild(edges); world.append(svg, cards); viewport.appendChild(world); panel.appendChild(viewport);
    var status = element("span", "mx-map-status");
    status.setAttribute("role", "status");
    panel.appendChild(status); document.body.appendChild(panel);
    button(toolbar, "+", function () { zoom(1.2); }, say("Zoom in", "بزرگ‌نمایی"));
    button(toolbar, "−", function () { zoom(1 / 1.2); }, say("Zoom out", "کوچک‌نمایی"));
    button(toolbar, say("Fit", "جا دادن"), fit);
    button(toolbar, say("Reset layout", "بازنشانی چیدمان"), function () {
      entries.forEach(function (entry) { entry.position = null; });
      layout(); fit();
    });
    button(toolbar, say("Expand all", "باز کردن همه"), function () { collapsed.clear(); layout(); fit(); });
    button(toolbar, say("Collapse all", "بستن همه"), function () {
      entries.forEach(function (entry) { if (entry.node.children.length) collapsed.add(entry.node.id); });
      layout(); fit();
    });
    button(toolbar, "SVG ↓", function () { exportMap(false); }, say("Export mind map as SVG", "خروجی SVG نقشه ذهنی"));
    button(toolbar, "PNG ↓", function () { exportMap(true); }, say("Export mind map as PNG", "خروجی PNG نقشه ذهنی"));

    // Labels are text; only KaTeX's own trusted=false output becomes HTML.
    function label(parent, text) {
      var at = 0;
      function dollar(index) {
        if (text[index] !== "$" ) return false;
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
          span.innerHTML = window.katex.renderToString(text.slice(i + 1, end), { throwOnError: false, trust: false, displayMode: false, maxExpand: 1000 });
        } else span.textContent = text.slice(i, end + 1);
        parent.appendChild(span); at = end + 1; i = end;
      }
      parent.appendChild(document.createTextNode(text.slice(at)));
    }
    var badges = {
      root: ["TOP", "آغاز"], heading: ["§", "بخش"], definition: ["DEF", "تعریف"],
      theorem: ["THM", "قضیه"], lemma: ["LEM", "لم"], corollary: ["COR", "نتیجه"],
      proposition: ["PROP", "گزاره"], example: ["EX", "مثال"], formula: ["ƒ", "فرمول"], note: ["NOTE", "نکته"]
    };
    function mount(node, ancestors) {
      var group = element("div", "mx-map-node");
      group.dataset.mapId = node.id;
      var card = document.createElementNS(htmlNs, "div");
      card.className = "mx-map-card mx-map-" + node.node_type;
      card.setAttribute("dir", "auto");
      var body = button(card, "", function () { navigate(node.id); }, node.label);
      body.className = "mx-map-label";
      body.title = node.label + " — " + say("Drag to move; click to read. Alt + arrow keys also move this card.", "برای جابجایی بکشید؛ برای خواندن کلیک کنید. کلیدهای Alt و جهت نیز کارت را جابجا می‌کنند.");
      body.setAttribute("aria-keyshortcuts", "Alt+ArrowUp Alt+ArrowDown Alt+ArrowLeft Alt+ArrowRight");
      var badge = badges[node.node_type] || badges.note;
      body.appendChild(element("span", "mx-map-badge", say(badge[0], badge[1])));
      var title = element("span", "mx-map-title"); label(title, node.label); body.appendChild(title);
      if (node.subtitle) {
        var sub = element("span", "mx-map-subtitle"); label(sub, node.subtitle); body.appendChild(sub);
      }
      var toggle = null;
      if (node.children.length) {
        toggle = button(card, "−", function () {
          if (collapsed.has(node.id)) collapsed.delete(node.id); else collapsed.add(node.id);
          layout();
        }, say("Expand or collapse branch", "باز و بسته کردن شاخه"));
        toggle.className = "mx-map-fold";
      }
      group.appendChild(card); cards.appendChild(group);
      entries.set(node.id, { node: node, ancestors: ancestors, group: group, card: card, toggle: toggle, height: 100, x: 0, y: 0, position: null });
      node.children.forEach(function (child) { mount(child, ancestors.concat(node.id)); });
    }
    mount(tree, []);
    // Leave room for connectors and separate section subtrees more clearly
    // than individual cards within the same section.
    var columnStep = 440;
    function gapBefore(children, index) {
      if (!index) return 0;
      var previous = children[index - 1], current = children[index];
      return previous.node_type === "heading" || current.node_type === "heading"
        || previous.children.length || current.children.length ? 88 : 48;
    }
    function layout() {
      edges.replaceChildren();
      connections = [];
      entries.forEach(function (e) {
        e.group.style.display = "";
        e.height = Math.max(72, e.card.scrollHeight + 4);
        e.group.style.height = e.height + "px";
        if (e.toggle) {
          e.toggle.textContent = collapsed.has(e.node.id) ? "+" : "−";
          e.toggle.setAttribute("aria-expanded", String(!collapsed.has(e.node.id)));
        }
      });
      function measure(node) {
        var e = entries.get(node.id);
        e.visibleChildren = collapsed.has(node.id) ? [] : node.children;
        e.space = Math.max(e.height, e.visibleChildren.reduce(function (sum, child, index, children) { return sum + gapBefore(children, index) + measure(child); }, 0));
        return e.space;
      }
      measure(tree);
      var visible = new Set();
      function place(node, depth, top) {
        var e = entries.get(node.id);
        visible.add(node.id);
        e.x = e.position ? e.position.x : depth * columnStep;
        e.y = e.position ? e.position.y : top + (e.space - e.height) / 2;
        positionCard(e);
        var offset = top;
        e.visibleChildren.forEach(function (child, index, children) {
          offset += gapBefore(children, index);
          var c = entries.get(child.id), path = svgElement("path");
          path.setAttribute("class", "mx-map-edge");
          path.dataset.mapFrom = node.id; path.dataset.mapTo = child.id;
          edges.appendChild(path);
          connections.push({ from: e, to: c, path: path });
          place(child, depth + 1, offset); offset += c.space;
        });
      }
      place(tree, 0, 0);
      entries.forEach(function (e, id) { e.group.style.display = visible.has(id) ? "" : "none"; });
      updateConnections(); updateBounds(); transform();
    }
    function positionCard(entry) {
      entry.group.style.left = entry.x + "px";
      entry.group.style.top = entry.y + "px";
    }
    function updateConnections() {
      connections.forEach(function (edge) {
        var x = edge.from.x + 280, y = edge.from.y + edge.from.height / 2;
        var endX = edge.to.x, endY = edge.to.y + edge.to.height / 2;
        var bend = Math.max(25, Math.min(160, Math.abs(endX - x) / 2));
        edge.path.setAttribute("d", "M" + x + "," + y + " C" + (x + bend) + "," + y + " " + (endX - bend) + "," + endY + " " + endX + "," + endY);
        edge.left = Math.min(x, endX, endX - bend);
        edge.right = Math.max(x, endX, x + bend);
      });
    }
    function updateBounds() {
      var left = Infinity, top = Infinity, right = -Infinity, bottom = -Infinity;
      entries.forEach(function (entry) {
        if (entry.group.style.display === "none") return;
        left = Math.min(left, entry.x); top = Math.min(top, entry.y);
        right = Math.max(right, entry.x + 280); bottom = Math.max(bottom, entry.y + entry.height);
      });
      // Backward connections can bend outside the card rectangles.
      connections.forEach(function (edge) { left = Math.min(left, edge.left); right = Math.max(right, edge.right); });
      bounds = { x: left, y: top, width: right - left, height: bottom - top };
      world.style.width = bounds.width + "px"; world.style.height = bounds.height + "px";
      svg.setAttribute("width", String(bounds.width)); svg.setAttribute("height", String(bounds.height));
    }
    function moveCard(entry, x, y) {
      entry.position = { x: x, y: y }; entry.x = x; entry.y = y;
      positionCard(entry); updateConnections(); updateBounds();
    }
    function transform() { world.style.transform = "translate(" + tx + "px," + ty + "px) scale(" + scale + ")"; }
    function zoom(factor, x, y) {
      x = x === undefined ? viewport.clientWidth / 2 : x;
      y = y === undefined ? viewport.clientHeight / 2 : y;
      var next = Math.max(.001, Math.min(3, scale * factor));
      tx = x - (x - tx) * next / scale; ty = y - (y - ty) * next / scale; scale = next; transform();
    }
    function fit() {
      if (panel.hidden) return;
      scale = Math.max(.001, Math.min(1.3, (viewport.clientWidth - 48) / bounds.width, (viewport.clientHeight - 48) / bounds.height));
      tx = (viewport.clientWidth - bounds.width * scale) / 2 - bounds.x * scale;
      ty = (viewport.clientHeight - bounds.height * scale) / 2 - bounds.y * scale; transform();
    }
    var observer;
    function watchDocument() {
      if (observer) observer.disconnect();
      if (!window.IntersectionObserver || mode === "map") return;
      observer = new IntersectionObserver(function (changes) {
        var visible = changes.filter(function (e) { return e.isIntersecting; });
        if (visible.length) select(visible[0].target.id);
      }, { root: mode === "split" ? main : null, rootMargin: "0px 0px -45% 0px" });
      entries.forEach(function (_, id) { var target = document.getElementById(id); if (target) observer.observe(target); });
    }
    function select(id) {
      entries.forEach(function (e, key) {
        e.card.classList.toggle("mx-map-selected", id === key);
        e.card.querySelector("button").setAttribute("aria-current", String(id === key));
      });
    }
    function setMode(next) {
      mode = next; document.body.dataset.mxView = mode;
      panel.hidden = mode === "document";
      main.hidden = mode === "map";
      Object.keys(modes).forEach(function (key) { modes[key].setAttribute("aria-pressed", String(key === mode)); });
      if (mode !== "document") { layout(); fit(); }
      watchDocument();
    }
    function navigate(id) {
      if (mode === "map") setMode("split");
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
    viewport.addEventListener("wheel", function (event) {
      event.preventDefault();
      if (drag) return;
      var rect = viewport.getBoundingClientRect(); zoom(Math.exp(-event.deltaY * .0015), event.clientX - rect.left, event.clientY - rect.top);
    }, { passive: false });
    var drag = null, suppressClick = false;
    viewport.addEventListener("pointerdown", function (event) {
      if (event.button !== 0 || drag) return;
      suppressClick = false;
      if (event.target.closest(".mx-map-fold")) return;
      var node = event.target.closest(".mx-map-node");
      var entry = node ? entries.get(node.dataset.mapId) : null;
      drag = { pointerId: event.pointerId, x: event.clientX, y: event.clientY, tx: tx, ty: ty,
        entry: entry, originX: entry ? entry.x : 0, originY: entry ? entry.y : 0, moved: false };
      // Delay card capture until movement crosses the threshold so an ordinary
      // click still targets the label button and navigates to the document.
      if (!entry) viewport.setPointerCapture(event.pointerId);
    });
    viewport.addEventListener("pointermove", function (event) {
      if (!drag || event.pointerId !== drag.pointerId) return;
      var dx = event.clientX - drag.x, dy = event.clientY - drag.y;
      if (!drag.moved && Math.hypot(dx, dy) < 4) return;
      if (!drag.moved) {
        drag.moved = true; suppressClick = true;
        viewport.setPointerCapture(event.pointerId);
        viewport.classList.add("mx-map-drag-active");
        if (drag.entry) drag.entry.group.classList.add("mx-map-dragging");
      }
      event.preventDefault();
      if (drag.entry) moveCard(drag.entry, drag.originX + dx / scale, drag.originY + dy / scale);
      else { tx = drag.tx + dx; ty = drag.ty + dy; transform(); }
    });
    function endDrag(event) {
      if (!drag || event.pointerId !== drag.pointerId) return;
      if (drag.entry) drag.entry.group.classList.remove("mx-map-dragging");
      viewport.classList.remove("mx-map-drag-active"); drag = null;
    }
    ["pointerup", "pointercancel", "lostpointercapture"].forEach(function (name) { viewport.addEventListener(name, endDrag); });
    viewport.addEventListener("click", function (event) {
      if (!suppressClick || event.detail === 0) return;
      suppressClick = false; event.preventDefault(); event.stopPropagation();
    }, true);
    viewport.addEventListener("keydown", function (event) {
      var deltas = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };
      if (!event.altKey || !deltas[event.key] || drag) return;
      var node = event.target.closest(".mx-map-node");
      if (!node) return;
      event.preventDefault();
      var entry = entries.get(node.dataset.mapId), step = event.shiftKey ? 50 : 20;
      moveCard(entry, entry.x + deltas[event.key][0] * step, entry.y + deltas[event.key][1] * step);
    });
    new ResizeObserver(function () { if (!panel.hidden) { layout(); fit(); } }).observe(viewport);

    function download(blob, extension) {
      // The desktop host performs the download outside its sandboxed preview.
      if (window.parent !== window) {
        var reader = new FileReader();
        reader.onload = function () { window.parent.postMessage({ mathia: "mindmap-export", data: reader.result, extension: extension }, "*"); };
        reader.readAsDataURL(blob); return;
      }
      var url = URL.createObjectURL(blob), a = element("a");
      a.href = url; a.download = "mathia-mindmap." + extension; a.click();
      window.setTimeout(function () { URL.revokeObjectURL(url); }, 30000);
    }
    async function exportMap(png) {
      try {
        if (document.fonts) await document.fonts.ready;
        var copy = svgElement("svg"), w = Math.ceil(bounds.width + 48), h = Math.ceil(bounds.height + 48);
        copy.setAttribute("xmlns", ns); copy.setAttribute("width", w); copy.setAttribute("height", h);
        copy.setAttribute("viewBox", (bounds.x - 24) + " " + (bounds.y - 24) + " " + w + " " + h);
        copy.style.cssText = "background:" + getComputedStyle(document.body).backgroundColor;
        // Rebuild at layout coordinates, independently of the viewport camera
        // and in-progress CSS transitions. foreignObject is export-only.
        function inlineStyles(original, clone) {
          var originals = [original].concat(Array.from(original.querySelectorAll("*")));
          var clones = [clone].concat(Array.from(clone.querySelectorAll("*")));
          originals.forEach(function (source, i) {
            var computed = getComputedStyle(source), target = clones[i];
            for (var j = 0; j < computed.length; j++) {
              var prop = computed[j]; target.style.setProperty(prop, computed.getPropertyValue(prop));
            }
            target.style.transition = "none";
            target.style.animation = "none";
          });
        }
        var lines = edges.cloneNode(true);
        inlineStyles(edges, lines);
        lines.style.transform = "none";
        copy.appendChild(lines);
        entries.forEach(function (entry) {
          if (entry.group.style.display === "none") return;
          var fo = svgElement("foreignObject"), card = entry.card.cloneNode(true);
          fo.setAttribute("x", String(entry.x)); fo.setAttribute("y", String(entry.y));
          fo.setAttribute("width", "280"); fo.setAttribute("height", String(entry.height));
          inlineStyles(entry.card, card);
          fo.appendChild(card); copy.appendChild(fo);
        });
        var style = svgElement("style");
        style.textContent = Array.from(document.querySelectorAll("style")).map(function (el) {
          return (el.textContent.match(/@font-face\s*\{[^}]+\}/g) || []).filter(function (rule) { return rule.indexOf("data:") !== -1; }).join("\n");
        }).join("\n");
        copy.insertBefore(style, copy.firstChild);
        var source = new XMLSerializer().serializeToString(copy);
        if (!png) { download(new Blob([source], { type: "image/svg+xml" }), "svg"); return; }
        // Bound raster allocation for large lecture notes.
        var ratio = Math.min(2, 8192 / w, 8192 / h, Math.sqrt(16000000 / (w * h)));
        var canvas = element("canvas"); canvas.width = Math.ceil(w * ratio); canvas.height = Math.ceil(h * ratio);
        var image = new Image();
        await new Promise(function (resolve, reject) {
          image.onload = resolve; image.onerror = reject;
          image.src = "data:image/svg+xml;charset=utf-8," + encodeURIComponent(source);
        });
        var ctx = canvas.getContext("2d");
        ctx.fillStyle = getComputedStyle(document.body).backgroundColor; ctx.fillRect(0, 0, canvas.width, canvas.height);
        ctx.drawImage(image, 0, 0, canvas.width, canvas.height);
        canvas.toBlob(function (blob) {
          if (blob) download(blob, "png"); else status.textContent = say("PNG export failed. Try SVG.", "خروجی PNG ناموفق بود؛ SVG را امتحان کنید.");
        }, "image/png");
      } catch (_) { status.textContent = say("Export failed. Try SVG.", "خروجی ناموفق بود؛ SVG را امتحان کنید."); }
    }
    window.Mathia.mindmap = { tree: tree, setMode: setMode };
    setMode("document");
    if (document.fonts) document.fonts.ready.then(function () { if (!panel.hidden) { layout(); fit(); } });
  }
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", start);
  else start();
})();
