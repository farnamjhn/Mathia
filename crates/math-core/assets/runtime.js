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
    renderMath();
    setupSidebar();
    setupScrollSpy();
  }
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", start);
  } else {
    start();
  }
})();
