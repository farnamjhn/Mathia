// Exercise the actual runtime pointer state machine without a browser. The tiny
// DOM supplies element/event APIs only; screen rendering remains covered by the
// browser geometry regressions in runtime.test.mjs.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { runInNewContext } from "node:vm";

const runtime = readFileSync(new URL("../../crates/math-core/assets/mindmap.js", import.meta.url), "utf8");

function setup(customTree) {
  class Element {
    constructor(tag) {
      this.tagName = tag;
      this.children = [];
      this.attributes = {};
      this.dataset = {};
      this.style = {};
      this.events = new Map();
      this.className = "";
      this.scrollHeight = 80;
      this.clientWidth = 1000;
      this.clientHeight = 700;
      this.classList = {
        contains: (name) => this.className.split(" ").includes(name),
        add: (name) => { if (!this.classList.contains(name)) this.className += " " + name; },
        remove: (name) => { this.className = this.className.split(" ").filter((n) => n !== name).join(" "); },
        toggle: (name, on) => on ? this.classList.add(name) : this.classList.remove(name),
      };
    }
    set textContent(value) { this.value = value; this.children = []; }
    get textContent() { return (this.value || "") + this.children.map((e) => e.textContent).join(""); }
    setAttribute(name, value) { this.attributes[name] = value; }
    appendChild(child) { child.parent = this; this.children.push(child); return child; }
    append(...children) { children.forEach((child) => this.appendChild(child)); }
    replaceChildren() { this.children = []; }
    addEventListener(name, fn, capture = false) {
      if (!this.events.has(name)) this.events.set(name, []);
      this.events.get(name).push({ fn, capture });
    }
    matches(selector) {
      return selector === "*" || (selector.startsWith(".")
        ? this.classList.contains(selector.slice(1)) : selector === this.tagName);
    }
    querySelectorAll(selector) {
      return this.children.flatMap((child) => [ ...(child.matches(selector) ? [child] : []), ...child.querySelectorAll(selector) ]);
    }
    querySelector(selector) { return this.querySelectorAll(selector)[0] || null; }
    closest(selector) { return this.matches(selector) ? this : this.parent?.closest(selector) || null; }
    setPointerCapture(id) { this.capturedPointer = id; }
    getBoundingClientRect() { return { left: 0, top: 0 }; }
    scrollIntoView() { this.scrolls = (this.scrolls || 0) + 1; }
  }
  const body = new Element("body"), main = new Element("main"), data = new Element("div");
  const payload = customTree || {
    id: "root", label: "Root", node_type: "root", children: [
      { id: "section", label: "Section", node_type: "heading", children: [
        { id: "definition", label: "Definition", node_type: "definition", children: [] },
      ] },
    ],
  };
  const rootNode = payload.root || payload;
  const targets = new Map([ ["mx-main", main], ["mx-mindmap-data", data] ]);
  function addTargets(node) {
    targets.set(node.id, new Element("section"));
    if (node.proof_id) {
      targets.set(node.proof_id, new Element("section"));
    }
    node.children.forEach(addTargets);
  }
  addTargets(rootNode);
  data.textContent = JSON.stringify(payload);
  const document = {
    body, documentElement: { dir: "ltr" }, readyState: "complete",
    createElement: (tag) => new Element(tag),
    createElementNS: (_, tag) => new Element(tag),
    createTextNode: (text) => { const e = new Element("#text"); e.textContent = text; return e; },
    getElementById: (id) => targets.get(id),
  };
  const window = { Mathia: {}, setTimeout() {} }; window.parent = window;
  runInNewContext(runtime, { window, document, ResizeObserver: class { observe() {} } });
  const viewport = body.querySelector(".mx-map-canvas");
  function fire(el, type, init = {}) {
    const event = { target: el, pointerId: 1, button: 0, clientX: 0, clientY: 0, detail: 1,
      preventDefault() { this.defaultPrevented = true; },
      stopPropagation() { this.stopped = true; }, ...init };
    for (const { fn } of el.events.get(type) || []) fn(event);
    return event;
  }
  function click(el) {
    const event = el.closest(".mx-map-canvas") ? fire(viewport, "click", { target: el }) : {};
    if (!event.stopped) fire(el, "click");
  }
  const control = (name) => body.querySelectorAll("button").find((b) => b.attributes["aria-label"] === name);
  const node = (id) => body.querySelectorAll(".mx-map-node").find((n) => n.dataset.mapId === id);
  const position = (id) => { const n = node(id); return { x: parseFloat(n.style.left), y: parseFloat(n.style.top) }; };
  const camera = () => body.querySelector(".mx-map-world").style.transform;
  const zoom = () => Number(camera().match(/scale\(([^)]+)\)/)[1]);
  function drag(id, dx, dy) {
    const label = node(id).querySelector(".mx-map-label");
    fire(viewport, "pointerdown", { target: label });
    fire(viewport, "pointermove", { target: label, clientX: dx, clientY: dy });
    fire(viewport, "pointerup", { target: viewport, clientX: dx, clientY: dy });
    click(label);
  }
  window.Mathia.mindmap.setMode("map");
  return { body, main, targets, viewport, fire, click, control, node, position, camera, zoom, drag, api: window.Mathia.mindmap };
}

test("card dragging respects zoom, follows the pointer, and suppresses navigation", () => {
  const s = setup();
  s.click(s.control("Zoom in"));
  const before = s.position("definition"), camera = s.camera(), scale = s.zoom();
  s.drag("definition", 96, -48);
  const after = s.position("definition");
  assert.ok(Math.abs(after.x - before.x - 96 / scale) < 1e-8);
  assert.ok(Math.abs(after.y - before.y + 48 / scale) < 1e-8);
  assert.equal(s.camera(), camera, "moving a card must not pan the camera");
  assert.equal(s.main.hidden, true, "drag must not navigate to the document");
  assert.equal(s.node("definition").classList.contains("mx-map-dragging"), false);
  const edge = s.body.querySelectorAll("path").find((p) => p.dataset.mapTo === "definition");
  assert.ok(edge.attributes.d.endsWith(`${after.x},${after.y + 42}`), "connector endpoint follows the moved card");
  const label = s.node("definition").querySelector(".mx-map-label");
  s.fire(s.viewport, "pointerdown", { target: label });
  s.fire(s.viewport, "pointerup", { target: label });
  s.click(label);
  assert.equal(s.main.hidden, false, "the next ordinary click still navigates");
  assert.equal(s.targets.get("definition").scrolls, 1);
});

test("manual positions survive folding and view changes; reset restores automatic layout", () => {
  const s = setup(), initial = s.position("definition");
  s.drag("definition", -1400, -800);
  const moved = s.position("definition");
  assert.ok(moved.x < 0 && moved.y < 0);
  s.click(s.control("Collapse all"));
  s.click(s.control("Expand all"));
  assert.deepEqual(s.position("definition"), moved);
  s.api.setMode("document"); s.api.setMode("split");
  assert.deepEqual(s.position("definition"), moved);
  // Fit includes a manually moved card above/left of the original origin.
  const [tx, ty] = s.camera().match(/translate\(([^p]+)px,([^p]+)px\)/).slice(1).map(Number);
  assert.ok(tx + moved.x * s.zoom() >= 23.9);
  assert.ok(ty + moved.y * s.zoom() >= 23.9);
  s.click(s.control("Reset layout"));
  assert.deepEqual(s.position("definition"), initial);
});

test("click threshold, fold buttons, keyboard movement, and pointer cancellation", () => {
  const s = setup(), initial = s.position("definition"), section = s.position("section");
  const label = s.node("definition").querySelector(".mx-map-label");
  s.drag("definition", 2, 1);
  assert.deepEqual(s.position("definition"), initial);
  assert.equal(s.main.hidden, false, "small pointer jitter is still a click");
  s.fire(s.viewport, "keydown", { target: label, altKey: true, key: "ArrowLeft" });
  assert.equal(s.position("definition").x, initial.x - 20);
  const fold = s.node("section").querySelector(".mx-map-fold");
  s.fire(s.viewport, "pointerdown", { target: fold });
  s.fire(s.viewport, "pointermove", { target: fold, clientX: 100 });
  assert.deepEqual(s.position("section"), section, "fold controls do not start a drag");
  s.fire(s.viewport, "pointerdown", { target: label });
  s.fire(s.viewport, "pointermove", { target: label, clientX: 30, pointerId: 2 });
  assert.equal(s.position("definition").x, initial.x - 20, "other pointers are ignored");
  s.fire(s.viewport, "pointermove", { target: label, clientX: 30 });
  s.fire(s.viewport, "pointercancel", { target: label });
  const cancelled = s.position("definition");
  s.fire(s.viewport, "pointermove", { target: label, clientX: 80 });
  assert.deepEqual(s.position("definition"), cancelled);
  assert.equal(s.viewport.classList.contains("mx-map-drag-active"), false);
});

test("background dragging still pans the camera without moving cards", () => {
  const s = setup(), before = s.position("definition"), camera = s.camera();
  s.fire(s.viewport, "pointerdown");
  s.fire(s.viewport, "pointermove", { clientX: 100, clientY: 50 });
  s.fire(s.viewport, "pointerup", { clientX: 100, clientY: 50 });
  assert.notEqual(s.camera(), camera);
  assert.deepEqual(s.position("definition"), before);
  assert.equal(s.viewport.classList.contains("mx-map-drag-active"), false);
});


test("initial layout separates columns and groups unequal section subtrees", () => {
  const leaf = (id) => ({ id, label: id, node_type: "definition", children: [] });
  const section = (id, children) => ({ id, label: id, node_type: "heading", children });
  const s = setup({ id: "root", label: "Root", node_type: "root", children: [
    section("short", [leaf("one")]),
    section("long", [leaf("two"), leaf("three"), leaf("four")]),
    section("last", [leaf("five")]),
  ] });
  const height = (id) => parseFloat(s.node(id).style.height);
  const bottom = (id) => s.position(id).y + height(id);
  assert.ok(s.position("short").x - s.position("root").x - 280 >= 140, "columns leave room for connectors");
  assert.ok(s.position("three").y - bottom("two") >= 48, "related cards have breathing room");
  assert.ok(s.position("two").y - bottom("one") >= 88, "section groups have additional separation");
  assert.ok(s.position("five").y - bottom("four") >= 88, "unequal subtrees remain separated");
  assert.equal(s.position("long").y + height("long") / 2,
    (s.position("two").y + bottom("four")) / 2, "section is centered beside its children");
  const initial = s.position("four");
  s.click(s.node("long").querySelector(".mx-map-fold"));
  s.click(s.node("long").querySelector(".mx-map-fold"));
  assert.deepEqual(s.position("four"), initial, "expanding a section restores the composed layout");
});

test("mind map data container with proof_id, key_terms, and dependency edges", () => {
  const data = {
    root: {
      id: "root", label: "Document", node_type: "root", level: 0, children: [
        {
          id: "sec-1", label: "Section 1", node_type: "heading", level: 1,
          key_terms: ["vector space", "basis"],
          children: [
            {
              id: "def-1", label: "Definition 1", node_type: "definition", level: 2,
              children: []
            },
            {
              id: "thm-1", label: "Theorem 1", node_type: "theorem", level: 2,
              proof_id: "proof-1",
              children: []
            }
          ]
        }
      ]
    },
    edges: [
      { from: "thm-1", to: "def-1", label: "Definition 1" }
    ]
  };

  const s = setup(data);
  // Verify key_terms rendered
  const secNode = s.node("sec-1");
  const terms = secNode.querySelectorAll(".mx-map-term");
  assert.equal(terms.length, 2);
  assert.equal(terms[0].textContent, "vector space");
  assert.equal(terms[1].textContent, "basis");

  // Verify proof_id mini-pill rendered
  const thmNode = s.node("thm-1");
  const proofPill = thmNode.querySelector(".mx-map-proof");
  assert.ok(proofPill, "proof pill should be rendered on theorem with proof_id");
  assert.equal(proofPill.textContent, "Proof ↗");

  // Clicking proof pill navigates to proof target without triggering card navigation
  const targetProof = s.targets.get("proof-1");
  assert.equal(targetProof.scrolls || 0, 0);
  s.fire(proofPill, "click");
  assert.equal(targetProof.scrolls, 1, "proof pill scrolls to proof anchor");

  // Verify Show Dependencies toggle
  const depControl = s.control("Show Dependencies");
  assert.ok(depControl, "Show Dependencies toolbar button should exist");
  assert.equal(depControl.attributes["aria-pressed"], "false");

  // Toggle dependencies ON
  s.fire(depControl, "click");
  assert.equal(depControl.attributes["aria-pressed"], "true");
  const depEdges = s.body.querySelectorAll(".mx-map-dep-edge");
  assert.equal(depEdges.length, 1);
  assert.equal(depEdges[0].dataset.depFrom, "thm-1");
  assert.equal(depEdges[0].dataset.depTo, "def-1");
  assert.equal(depEdges[0].attributes["stroke-dasharray"], "6,4");

  // Toggle dependencies OFF
  s.fire(depControl, "click");
  assert.equal(depControl.attributes["aria-pressed"], "false");
  const depEdgesOff = s.body.querySelectorAll(".mx-map-dep-edge");
  assert.equal(depEdgesOff.length, 0);
});
