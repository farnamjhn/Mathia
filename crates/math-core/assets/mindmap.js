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
    var collapsed = new Set(), entries = new Map(), bounds = { width: 1, height: 1 };
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
    var svg = svgElement("svg");
    svg.classList.add("mx-map-canvas");
    svg.setAttribute("aria-label", say("Mind map. Drag to pan; scroll to zoom.", "نقشه ذهنی؛ کشیدن برای جابجایی و پیمایش برای بزرگ‌نمایی"));
    var world = svgElement("g"), edges = svgElement("g"), cards = svgElement("g");
    world.append(edges, cards); svg.appendChild(world); panel.appendChild(svg);
    var status = element("span", "mx-map-status");
    status.setAttribute("role", "status");
    panel.appendChild(status); document.body.appendChild(panel);
    button(toolbar, "+", function () { zoom(1.2); }, say("Zoom in", "بزرگ‌نمایی"));
    button(toolbar, "−", function () { zoom(1 / 1.2); }, say("Zoom out", "کوچک‌نمایی"));
    button(toolbar, say("Fit", "جا دادن"), fit);
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
      var group = svgElement("g"), fo = svgElement("foreignObject");
      group.classList.add("mx-map-node");
      group.dataset.mapId = node.id;
      fo.setAttribute("width", "280"); fo.setAttribute("height", "200");
      var card = document.createElementNS(htmlNs, "div");
      card.className = "mx-map-card mx-map-" + node.node_type;
      card.setAttribute("dir", "auto");
      var body = button(card, "", function () { navigate(node.id); }, node.label);
      body.className = "mx-map-label";
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
      fo.appendChild(card); group.appendChild(fo); cards.appendChild(group);
      entries.set(node.id, { node: node, ancestors: ancestors, group: group, fo: fo, card: card, toggle: toggle, height: 100 });
      node.children.forEach(function (child) { mount(child, ancestors.concat(node.id)); });
    }
    mount(tree, []);
    function layout() {
      edges.replaceChildren();
      entries.forEach(function (e) {
        e.group.style.display = "";
        e.height = Math.max(72, e.card.scrollHeight + 4);
        e.fo.setAttribute("height", String(e.height));
        if (e.toggle) {
          e.toggle.textContent = collapsed.has(e.node.id) ? "+" : "−";
          e.toggle.setAttribute("aria-expanded", String(!collapsed.has(e.node.id)));
        }
      });
      function measure(node) {
        var e = entries.get(node.id);
        e.visibleChildren = collapsed.has(node.id) ? [] : node.children;
        e.space = Math.max(e.height, e.visibleChildren.reduce(function (sum, child) { return sum + measure(child) + 22; }, -22));
        return e.space;
      }
      measure(tree);
      var visible = new Set(), width = 0;
      function place(node, depth, top) {
        var e = entries.get(node.id), x = depth * 330, y = top + (e.space - e.height) / 2;
        visible.add(node.id); width = Math.max(width, x + 280);
        e.group.setAttribute("transform", "translate(" + x + " " + y + ")");
        e.group.style.transform = "translate(" + x + "px," + y + "px)";
        var offset = top;
        e.visibleChildren.forEach(function (child) {
          var c = entries.get(child.id), cy = offset + c.space / 2;
          var path = svgElement("path"), start = y + e.height / 2;
          path.setAttribute("d", "M" + (x + 280) + "," + start + " C" + (x + 305) + "," + start + " " + (x + 305) + "," + cy + " " + (x + 330) + "," + cy);
          path.setAttribute("class", "mx-map-edge"); edges.appendChild(path);
          place(child, depth + 1, offset); offset += c.space + 22;
        });
      }
      place(tree, 0, 0);
      entries.forEach(function (e, id) { e.group.style.display = visible.has(id) ? "" : "none"; });
      bounds = { width: width, height: entries.get(tree.id).space };
      transform();
    }
    function transform() { world.setAttribute("transform", "translate(" + tx + " " + ty + ") scale(" + scale + ")"); }
    function zoom(factor, x, y) {
      x = x === undefined ? svg.clientWidth / 2 : x;
      y = y === undefined ? svg.clientHeight / 2 : y;
      var next = Math.max(.001, Math.min(3, scale * factor));
      tx = x - (x - tx) * next / scale; ty = y - (y - ty) * next / scale; scale = next; transform();
    }
    function fit() {
      if (panel.hidden) return;
      scale = Math.max(.001, Math.min(1.3, (svg.clientWidth - 48) / bounds.width, (svg.clientHeight - 48) / bounds.height));
      tx = (svg.clientWidth - bounds.width * scale) / 2;
      ty = (svg.clientHeight - bounds.height * scale) / 2; transform();
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
    svg.addEventListener("wheel", function (event) {
      event.preventDefault();
      var rect = svg.getBoundingClientRect(); zoom(Math.exp(-event.deltaY * .0015), event.clientX - rect.left, event.clientY - rect.top);
    }, { passive: false });
    var drag = null;
    svg.addEventListener("pointerdown", function (event) {
      if (event.button !== 0 || event.target.closest("foreignObject")) return;
      drag = { x: event.clientX, y: event.clientY, tx: tx, ty: ty }; svg.setPointerCapture(event.pointerId);
    });
    svg.addEventListener("pointermove", function (event) {
      if (!drag) return;
      tx = drag.tx + event.clientX - drag.x; ty = drag.ty + event.clientY - drag.y; transform();
    });
    ["pointerup", "pointercancel", "lostpointercapture"].forEach(function (name) { svg.addEventListener(name, function () { drag = null; }); });
    new ResizeObserver(function () { if (!panel.hidden) { layout(); fit(); } }).observe(svg);

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
        var copy = svg.cloneNode(true), w = Math.ceil(bounds.width + 48), h = Math.ceil(bounds.height + 48);
        copy.setAttribute("xmlns", ns); copy.setAttribute("width", w); copy.setAttribute("height", h);
        copy.setAttribute("viewBox", "0 0 " + w + " " + h);
        copy.style.cssText = "background:" + getComputedStyle(document.body).backgroundColor;
        copy.firstElementChild.setAttribute("transform", "translate(24 24)");
        // Inline computed styles so the exported SVG is independent of theme,
        // ancestor selectors, CSS variables, and the original document.
        var originals = svg.querySelectorAll("*"), clones = copy.querySelectorAll("*");
        originals.forEach(function (original, i) {
          var computed = getComputedStyle(original), clone = clones[i];
          for (var j = 0; j < computed.length; j++) {
            var prop = computed[j]; clone.style.setProperty(prop, computed.getPropertyValue(prop));
          }
          clone.style.transition = "none";
          clone.style.animation = "none";
        });
        copy.firstElementChild.style.transform = "translate(24px,24px)";
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
