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
        clone.querySelectorAll(".mx-block-head").forEach(function (n) { n.remove(); });
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
      button.addEventListener("click", function () { action.run(describe(el)); });
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

  function start() {
    setupTheme();
    renderMath();
    setupSidebar();
    setupTocFolding();
    setupScrollSpy();
  }
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", start);
  } else {
    start();
  }
})();
