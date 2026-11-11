/* Mathia AI interactive runtime: settings, providers, streaming, safe rendering,
   and the per-block Ask and Illustrate actions. See docs/archive/phase2-spec.md. */
(function () {
  "use strict";

  var body = document.body;
  var aiAttr = (body && body.dataset && body.dataset.mxAi) ? body.dataset.mxAi.trim() : "";
  if (!body || !aiAttr) {
    return;
  }

  var enabledProviders = aiAttr.split(/\s+/).filter(Boolean);
  if (enabledProviders.length === 0) {
    return;
  }

  // Parse default endpoints from body[data-mx-ai-endpoints]
  var defaultEndpoints = {};
  var epAttr = (body.dataset && body.dataset.mxAiEndpoints) ? body.dataset.mxAiEndpoints.trim() : "";
  if (epAttr) {
    var epParts = epAttr.split(/\s+/).filter(Boolean);
    for (var i = 0; i < epParts.length; i++) {
      var eqIdx = epParts[i].indexOf("=");
      if (eqIdx !== -1) {
        var slug = epParts[i].slice(0, eqIdx).trim();
        var url = epParts[i].slice(eqIdx + 1).trim();
        if (slug && url) {
          defaultEndpoints[slug] = url;
        }
      }
    }
  }

  // Online / offline class management on <html>
  function updateOnlineStatus() {
    var isOffline = typeof navigator !== "undefined" && navigator.onLine === false;
    document.documentElement.classList.toggle("mx-ai-offline", isOffline);
    if (typeof updateAllPanelKeyNotices === "function") {
      updateAllPanelKeyNotices();
    }
  }
  updateOnlineStatus();
  window.addEventListener("online", updateOnlineStatus);
  window.addEventListener("offline", updateOnlineStatus);

  var DEFAULT_MODELS = {
    anthropic: "claude-sonnet-5-5",
    openai: "gpt-5-mini"
  };

  var BUILTIN_ORIGINS = {
    anthropic: "https://api." + "anthropic." + "com",
    openai: "https://api.openai.com",
    gemini: "https://generativelanguage.googleapis.com"
  };

  /* Anthropic, OpenAI and Gemini have fixed hosts; every other provider slug
     (the "openai-compatible" any-URL one, or a pinned --ai-custom one) speaks the
     OpenAI /chat/completions protocol against an endpoint the reader supplies. */
  function isBuiltin(provider) {
    return provider === "anthropic" || provider === "openai" || provider === "gemini";
  }

  /* Accepts a base URL ("https://host/v1"), with or without a trailing slash,
     or a pasted full URL ("https://host/v1/chat/completions"). */
  function normalizeEndpoint(ep) {
    return String(ep || "").trim().replace(/\/+$/, "").replace(/\/chat\/completions$/i, "").replace(/\/+$/, "");
  }

  // Storage
  var STORAGE_KEY = "mathia-ai";
  var memoryStorage = null;

  function readStorage() {
    var raw = null;
    try {
      raw = window.localStorage.getItem(STORAGE_KEY);
    } catch (e) {}
    if (!raw && memoryStorage) {
      return memoryStorage;
    }
    if (raw) {
      try {
        var parsed = JSON.parse(raw);
        if (parsed && typeof parsed === "object") {
          return parsed;
        }
      } catch (e) {}
    }
    return {
      provider: "",
      models: {},
      keys: {},
      endpoints: {}
    };
  }

  function writeStorage(data) {
    memoryStorage = data;
    try {
      window.localStorage.setItem(STORAGE_KEY, JSON.stringify(data));
    } catch (e) {}
  }

  function currentProvider() {
    var s = readStorage();
    if (s.provider && enabledProviders.indexOf(s.provider) !== -1) {
      return s.provider;
    }
    return enabledProviders[0] || "";
  }

  function getEndpoint(provider) {
    var s = readStorage();
    if (s.endpoints && s.endpoints[provider]) {
      return s.endpoints[provider];
    }
    if (defaultEndpoints[provider]) {
      return defaultEndpoints[provider];
    }
    if (provider === "openai") {
      return "https://api.openai.com/v1";
    }
    return "";
  }

  function getOrigin(provider, customEp) {
    if (BUILTIN_ORIGINS[provider]) {
      return BUILTIN_ORIGINS[provider];
    }
    var ep = customEp !== undefined ? customEp : getEndpoint(provider);
    if (!ep) return "(not configured)";
    try {
      var base = window.location && window.location.href ? window.location.href : "http://localhost";
      var u = new URL(ep, base);
      return u.origin;
    } catch (e) {
      var m = ep.match(/^(https?:\/\/[^\/?#]+)/i);
      return m ? m[1] : ep;
    }
  }

  function hasKeyFor(provider) {
    var s = readStorage();
    var key = s.keys && s.keys[provider];
    return Boolean(key && key.trim());
  }

  function settings() {
    var s = readStorage();
    var p = currentProvider();
    var m = (s.models && s.models[p]) || DEFAULT_MODELS[p] || "";
    var hasKey = hasKeyFor(p);
    return {
      provider: p,
      model: m,
      hasKey: hasKey
    };
  }

  function configure(opts) {
    if (!opts || typeof opts !== "object") return;
    var s = readStorage();
    if (!s.models) s.models = {};
    if (!s.keys) s.keys = {};
    if (!s.endpoints) s.endpoints = {};

    if (opts.provider && enabledProviders.indexOf(opts.provider) !== -1) {
      s.provider = opts.provider;
    }
    var target = opts.provider || s.provider || enabledProviders[0];

    if (typeof opts.model === "string") {
      s.models[target] = opts.model.trim();
    }
    if (typeof opts.key === "string") {
      if (opts.key.trim()) {
        s.keys[target] = opts.key.trim();
      } else {
        delete s.keys[target];
      }
    }
    if (typeof opts.endpoint === "string") {
      if (opts.endpoint.trim()) {
        s.endpoints[target] = opts.endpoint.trim();
      } else {
        delete s.endpoints[target];
      }
    }
    writeStorage(s);
  }

  function forgetKey(provider) {
    var s = readStorage();
    if (!s.keys) s.keys = {};
    if (typeof provider === "string" && provider.trim()) {
      delete s.keys[provider.trim()];
    } else {
      s.keys = {};
    }
    writeStorage(s);
  }

  function available() {
    if (enabledProviders.length === 0) return false;
    if (typeof navigator !== "undefined" && navigator.onLine === false) return false;
    var p = currentProvider();
    if (!p) return false;
    if (isBuiltin(p)) return hasKeyFor(p);
    /* Endpoint-based providers: the endpoint is what is required. The key is
       optional, since local servers and many gateways take none. */
    return /^https?:\/\//i.test(getEndpoint(p));
  }

  // SSE parser: accepts arbitrary chunk boundaries, returns list of complete data: payloads
  function findNewline(buf) {
    for (var i = 0; i < buf.length; i++) {
      var c = buf.charCodeAt(i);
      if (c === 10 /* \n */) {
        return { start: i, end: i + 1 };
      }
      if (c === 13 /* \r */) {
        if (i + 1 < buf.length) {
          if (buf.charCodeAt(i + 1) === 10 /* \n */) {
            return { start: i, end: i + 2 };
          }
          return { start: i, end: i + 1 };
        }
        // \r is at the very end of buffer; wait for next chunk
        return null;
      }
    }
    return null;
  }

  function parseSSE(chunkText, state) {
    if (!state) state = {};
    state.buffer = (state.buffer || "") + (chunkText || "");
    state.dataLines = state.dataLines || [];
    var results = [];

    while (true) {
      var nl = findNewline(state.buffer);
      if (!nl) break;

      var line = state.buffer.slice(0, nl.start);
      state.buffer = state.buffer.slice(nl.end);

      if (line.length === 0) {
        if (state.dataLines.length > 0) {
          results.push(state.dataLines.join("\n"));
          state.dataLines = [];
        }
      } else if (line.charAt(0) === ":") {
        // comment line, ignore
      } else if (line.indexOf("data:") === 0) {
        var val = line.slice(5);
        if (val.charAt(0) === " ") {
          val = val.slice(1);
        }
        state.dataLines.push(val);
      }
      // ignore event:, id:, retry:, etc.
    }

    return results;
  }

  // buildRequest(provider, model, key, {system, messages})
  function buildRequest(provider, model, key, options) {
    options = options || {};
    var system = options.system || "";
    var messages = options.messages || [];

    if (provider === "anthropic") {
      var anthBody = {
        model: model,
        max_tokens: 4096,
        messages: messages,
        stream: true
      };
      if (system) {
        anthBody.system = system;
      }
      return {
        url: "https://api." + "anthropic." + "com/v1/messages",
        init: {
          method: "POST",
          headers: {
            "content-type": "application/json",
            "x-api-key": key || "",
            "anthropic-version": "2023-06-01",
            "anthropic-dangerous-direct-browser-access": "true"
          },
          body: JSON.stringify(anthBody)
        }
      };
    }

    if (provider === "gemini") {
      var contents = [];
      for (var i = 0; i < messages.length; i++) {
        var m = messages[i];
        contents.push({
          role: m.role === "assistant" ? "model" : "user",
          parts: [{ text: m.content }]
        });
      }
      var geminiBody = { contents: contents };
      if (system) {
        geminiBody.systemInstruction = {
          parts: [{ text: system }]
        };
      }
      return {
        url: "https://generativelanguage.googleapis.com/v1beta/models/" + encodeURIComponent(model) + ":streamGenerateContent?alt=sse",
        init: {
          method: "POST",
          headers: {
            "content-type": "application/json",
            "x-goog-api-key": key || ""
          },
          body: JSON.stringify(geminiBody)
        }
      };
    }

    // openai or custom openai-compatible
    var ep = normalizeEndpoint(options.endpoint || (provider === "openai" ? "https://api.openai.com/v1" : getEndpoint(provider)));
    var reqMessages = [];
    if (system) {
      reqMessages.push({ role: "system", content: system });
    }
    for (var j = 0; j < messages.length; j++) {
      reqMessages.push(messages[j]);
    }
    var headers = {
      "content-type": "application/json"
    };
    if (key) {
      headers["authorization"] = "Bearer " + key;
    }
    return {
      url: ep + "/chat/completions",
      init: {
        method: "POST",
        headers: headers,
        body: JSON.stringify({
          model: model,
          stream: true,
          messages: reqMessages
        })
      }
    };
  }

  // extractDelta(provider, parsedJson)
  function extractDelta(provider, payload) {
    if (!payload) return "";
    var data = payload;
    if (typeof payload === "string") {
      if (payload.trim() === "[DONE]") return "";
      try {
        data = JSON.parse(payload);
      } catch (e) {
        return "";
      }
    }
    if (!data || typeof data !== "object") return "";

    if (provider === "anthropic") {
      if (data.type === "error" && data.error) {
        throw new Error(data.error.message || "Anthropic error");
      }
      if (data.type === "content_block_delta" && data.delta && data.delta.type === "text_delta") {
        return data.delta.text || "";
      }
      return "";
    }

    if (provider === "gemini") {
      if (data.error) {
        throw new Error(data.error.message || "Gemini error");
      }
      if (data.candidates && data.candidates[0] && data.candidates[0].content && data.candidates[0].content.parts) {
        var parts = data.candidates[0].content.parts;
        var text = "";
        for (var i = 0; i < parts.length; i++) {
          if (parts[i] && parts[i].text) {
            text += parts[i].text;
          }
        }
        return text;
      }
      return "";
    }

    // openai / custom
    if (data.error) {
      throw new Error(data.error.message || "OpenAI error");
    }
    if (data.choices && data.choices[0] && data.choices[0].delta && typeof data.choices[0].delta.content === "string") {
      return data.choices[0].delta.content;
    }
    return "";
  }

  // complete({system, messages, onDelta, signal}) -> Promise<string>
  function complete(options) {
    options = options || {};
    return new Promise(function (resolve, reject) {
      if (!available()) {
        reject(new Error("AI is not available (check configuration or network connection)"));
        return;
      }
      if (options.signal && options.signal.aborted) {
        reject(options.signal.reason || new Error("Aborted"));
        return;
      }

      var p = currentProvider();
      var s = readStorage();
      var m = (s.models && s.models[p]) || DEFAULT_MODELS[p] || "";
      var key = (s.keys && s.keys[p]) || "";
      var endpoint = getEndpoint(p);

      var req = buildRequest(p, m, key, {
        system: options.system,
        messages: options.messages,
        endpoint: endpoint
      });

      var init = Object.assign({}, req.init);
      if (options.signal) {
        init.signal = options.signal;
      }

      fetch(req.url, init)
        .then(function (res) {
          if (!res.ok) {
            return res.text().then(function (bodyText) {
              var msg = "";
              try {
                var json = JSON.parse(bodyText);
                if (json.error && json.error.message) msg = json.error.message;
                else if (json.message) msg = json.message;
              } catch (e) {
                msg = bodyText;
              }
              var err = new Error("HTTP " + res.status + (res.statusText ? " " + res.statusText : "") + (msg ? ": " + msg : ""));
              err.status = res.status;
              throw err;
            });
          }

          if (!res.body || !res.body.getReader) {
            throw new Error("Streaming not supported by response body");
          }

          var reader = res.body.getReader();
          var decoder = new TextDecoder("utf-8");
          var sseState = {};
          var fullText = "";

          if (options.signal) {
            options.signal.addEventListener("abort", function () {
              try { reader.cancel(); } catch (e) {}
            });
          }

          function pump() {
            return reader.read().then(function (chunk) {
              if (chunk.done) {
                if (options.signal && options.signal.aborted) {
                  var abortErr = new Error("Aborted");
                  abortErr.name = "AbortError";
                  throw abortErr;
                }
                return fullText;
              }
              var chunkText = decoder.decode(chunk.value, { stream: true });
              var payloads = parseSSE(chunkText, sseState);
              for (var k = 0; k < payloads.length; k++) {
                var payload = payloads[k];
                if (payload === "[DONE]") {
                  try { reader.cancel(); } catch (e) {}
                  return fullText;
                }
                var delta = extractDelta(p, payload);
                if (delta) {
                  fullText += delta;
                  if (typeof options.onDelta === "function") {
                    options.onDelta(delta, fullText);
                  }
                }
              }
              return pump();
            });
          }

          return pump();
        })
        .then(resolve)
        .catch(reject);
    });
  }

  // sanitizeSvg(string) -> string | null
  var SVG_NS = "http://www.w3.org/2000/svg";
  var XLINK_NS = "http://www.w3.org/1999/xlink";

  function toSet(list) {
    var set = Object.create(null);
    for (var i = 0; i < list.length; i++) set[list[i]] = true;
    return set;
  }

  var ALLOWED_SVG_ELEMENTS = toSet([
    "svg", "g", "defs", "title", "desc", "path", "rect", "circle", "ellipse",
    "line", "polyline", "polygon", "text", "tspan", "marker", "linearGradient",
    "radialGradient", "stop", "clipPath", "mask", "pattern", "symbol", "use"
  ]);

  /* Geometry and presentation attributes only: nothing that can run script or
     name an outside resource. */
  var ALLOWED_SVG_ATTRIBUTES = toSet([
    "id", "class", "xmlns", "xmlns:xlink", "version", "viewBox", "preserveAspectRatio",
    "width", "height", "x", "y", "x1", "y1", "x2", "y2", "cx", "cy", "r", "rx", "ry",
    "d", "points", "dx", "dy", "rotate", "pathLength", "transform",
    "fill", "fill-opacity", "fill-rule", "stroke", "stroke-width", "stroke-linecap",
    "stroke-linejoin", "stroke-dasharray", "stroke-dashoffset", "stroke-opacity",
    "stroke-miterlimit", "opacity", "color", "display", "visibility", "vector-effect",
    "paint-order", "shape-rendering", "text-rendering",
    "font-family", "font-size", "font-weight", "font-style", "text-anchor",
    "dominant-baseline", "alignment-baseline", "baseline-shift", "letter-spacing",
    "word-spacing", "text-decoration", "direction", "unicode-bidi", "writing-mode",
    "textLength", "lengthAdjust", "xml:space", "xml:lang", "lang",
    "marker-start", "marker-mid", "marker-end", "markerWidth", "markerHeight",
    "markerUnits", "refX", "refY", "orient",
    "offset", "stop-color", "stop-opacity", "gradientUnits", "gradientTransform",
    "spreadMethod", "fx", "fy", "fr",
    "clip-path", "clip-rule", "clipPathUnits", "mask", "maskUnits", "maskContentUnits",
    "patternUnits", "patternContentUnits", "patternTransform",
    "href", "xlink:href", "style", "role", "aria-label", "aria-hidden"
  ]);

  /* Every url(...) in the value must point inside the figure: url(#id). */
  function hasOnlyLocalUrls(value) {
    var urls = value.match(/url\s*\([^)]*\)?/gi) || [];
    for (var i = 0; i < urls.length; i++) {
      if (!/^url\s*\(\s*(['"]?)#[^'")\s]*\1\s*\)$/i.test(urls[i])) return false;
    }
    return true;
  }

  function isSafeSvgAttribute(name, value) {
    if (!ALLOWED_SVG_ATTRIBUTES[name]) return false;
    if (name === "xmlns") return value === SVG_NS;
    if (name === "xmlns:xlink") return value === XLINK_NS;
    if (name === "href" || name === "xlink:href") return value.trim().charAt(0) === "#";
    if (!hasOnlyLocalUrls(value)) return false;
    /* A backslash can spell "url" with CSS escapes, so it is refused outright. */
    if (name === "style" && /\\|@import|expression|javascript:|behavior/i.test(value)) return false;
    return true;
  }

  function sanitizeSvg(svgText) {
    if (typeof svgText !== "string" || !svgText.trim()) return null;
    if (typeof DOMParser === "undefined" || typeof XMLSerializer === "undefined") return null;

    /* Models often leave the namespaces out; without them the figure would not
       parse (xlink) or not draw (svg). */
    var open = svgText.match(/<svg\b[^>]*>/i);
    if (!open) return null;
    var extra = "";
    if (!/\sxmlns\s*=/.test(open[0])) extra += ' xmlns="' + SVG_NS + '"';
    if (/xlink:/.test(svgText) && !/\sxmlns:xlink\s*=/.test(open[0])) extra += ' xmlns:xlink="' + XLINK_NS + '"';
    if (extra) {
      svgText = svgText.slice(0, open.index + 4) + extra + svgText.slice(open.index + 4);
    }

    var doc;
    try {
      doc = new DOMParser().parseFromString(svgText, "image/svg+xml");
    } catch (e) {
      return null;
    }
    if (!doc || doc.getElementsByTagName("parsererror").length > 0) return null;

    var root = doc.documentElement;
    if (!root || root.localName !== "svg" || root.namespaceURI !== SVG_NS) return null;

    function sanitizeElement(el) {
      var attrs = Array.prototype.slice.call(el.attributes);
      for (var j = 0; j < attrs.length; j++) {
        if (!isSafeSvgAttribute(attrs[j].name, attrs[j].value)) {
          el.removeAttributeNode(attrs[j]);
        }
      }
      var children = Array.prototype.slice.call(el.childNodes);
      for (var k = 0; k < children.length; k++) {
        var child = children[k];
        if (child.nodeType === 1) {
          if (child.namespaceURI === SVG_NS && ALLOWED_SVG_ELEMENTS[child.localName]) {
            sanitizeElement(child);
          } else {
            el.removeChild(child);
          }
        } else if (child.nodeType !== 3 && child.nodeType !== 4) {
          /* comments, processing instructions, entity references */
          el.removeChild(child);
        }
      }
    }

    sanitizeElement(root);
    return new XMLSerializer().serializeToString(root);
  }

  // Safe markdown to DOM nodes
  function parseInline(text, parent) {
    var i = 0;
    var len = text.length;
    var plainStart = 0;

    function flushPlain(end) {
      if (end > plainStart) {
        var str = text.slice(plainStart, end);
        str = str.replace(/\\([$*`\\\[\]()])/g, "$1");
        parent.appendChild(document.createTextNode(str));
      }
    }

    while (i < len) {
      var c = text.charAt(i);

      if (c === "\\") {
        if (i + 1 < len) {
          var next = text.charAt(i + 1);
          if (next === "[") {
            var closeIdx = text.indexOf("\\]", i + 2);
            if (closeIdx !== -1) {
              flushPlain(i);
              var div = document.createElement("div");
              div.className = "math math-display";
              div.textContent = text.slice(i + 2, closeIdx);
              parent.appendChild(div);
              i = closeIdx + 2;
              plainStart = i;
              continue;
            }
          }
          if (next === "(") {
            var closeIdx2 = text.indexOf("\\)", i + 2);
            if (closeIdx2 !== -1) {
              flushPlain(i);
              var span = document.createElement("span");
              span.className = "math math-inline";
              span.textContent = text.slice(i + 2, closeIdx2);
              parent.appendChild(span);
              i = closeIdx2 + 2;
              plainStart = i;
              continue;
            }
          }
        }
        i += 2;
        continue;
      }

      if (c === "$" && text.charAt(i + 1) === "$") {
        var closeDd = text.indexOf("$$", i + 2);
        if (closeDd !== -1) {
          flushPlain(i);
          var mDiv = document.createElement("div");
          mDiv.className = "math math-display";
          mDiv.textContent = text.slice(i + 2, closeDd);
          parent.appendChild(mDiv);
          i = closeDd + 2;
          plainStart = i;
          continue;
        }
      }

      if (c === "$") {
        var closeD = -1;
        for (var k = i + 1; k < len; k++) {
          if (text.charAt(k) === "\n") break;
          if (text.charAt(k) === "$" && text.charAt(k - 1) !== "\\") {
            closeD = k;
            break;
          }
        }
        if (closeD !== -1 && closeD > i + 1) {
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

      if (c === "`") {
        var closeCode = text.indexOf("`", i + 1);
        if (closeCode !== -1) {
          flushPlain(i);
          var codeEl = document.createElement("code");
          codeEl.textContent = text.slice(i + 1, closeCode);
          parent.appendChild(codeEl);
          i = closeCode + 1;
          plainStart = i;
          continue;
        }
      }

      if (c === "*" && text.charAt(i + 1) === "*") {
        var closeBold = text.indexOf("**", i + 2);
        if (closeBold !== -1 && closeBold > i + 2) {
          flushPlain(i);
          var strong = document.createElement("strong");
          parseInline(text.slice(i + 2, closeBold), strong);
          parent.appendChild(strong);
          i = closeBold + 2;
          plainStart = i;
          continue;
        }
      }

      if (c === "*") {
        var closeItalic = -1;
        for (var m = i + 1; m < len; m++) {
          if (text.charAt(m) === "\n") break;
          if (text.charAt(m) === "*" && text.charAt(m - 1) !== "\\") {
            if (text.charAt(m + 1) !== "*") {
              closeItalic = m;
              break;
            }
          }
        }
        if (closeItalic !== -1 && closeItalic > i + 1) {
          flushPlain(i);
          var em = document.createElement("em");
          parseInline(text.slice(i + 1, closeItalic), em);
          parent.appendChild(em);
          i = closeItalic + 1;
          plainStart = i;
          continue;
        }
      }

      if (c === "[") {
        var closeBracket = text.indexOf("]", i + 1);
        if (closeBracket !== -1 && text.charAt(closeBracket + 1) === "(") {
          var closeParen = text.indexOf(")", closeBracket + 2);
          if (closeParen !== -1) {
            flushPlain(i);
            var label = text.slice(i + 1, closeBracket);
            parent.appendChild(document.createTextNode(label));
            i = closeParen + 1;
            plainStart = i;
            continue;
          }
        }
      }

      i++;
    }

    flushPlain(len);
  }

  function markdownToNodes(markdownText) {
    var frag = document.createDocumentFragment();
    if (typeof markdownText !== "string" || !markdownText.trim()) {
      return frag;
    }

    var rawLines = markdownText.replace(/\r\n/g, "\n").replace(/\r/g, "\n").split("\n");
    var i = 0;
    var numLines = rawLines.length;

    while (i < numLines) {
      var line = rawLines[i];

      if (!line.trim()) {
        i++;
        continue;
      }

      // Fenced code block
      var codeFenceMatch = line.match(/^```/);
      if (codeFenceMatch) {
        i++;
        var codeLines = [];
        while (i < numLines && !rawLines[i].match(/^```/)) {
          codeLines.push(rawLines[i]);
          i++;
        }
        if (i < numLines) i++;
        var pre = document.createElement("pre");
        var code = document.createElement("code");
        code.textContent = codeLines.join("\n");
        pre.appendChild(code);
        frag.appendChild(pre);
        continue;
      }

      // Display math block: $$...$$ or \[...\]
      if (line.trim().indexOf("$$") === 0) {
        var trimmed = line.trim();
        if (trimmed.length > 2 && trimmed.endsWith("$$")) {
          var mDiv = document.createElement("div");
          mDiv.className = "math math-display";
          mDiv.textContent = trimmed.slice(2, -2).trim();
          frag.appendChild(mDiv);
          i++;
          continue;
        }
        i++;
        var mathLines = [];
        while (i < numLines && rawLines[i].trim().indexOf("$$") === -1) {
          mathLines.push(rawLines[i]);
          i++;
        }
        if (i < numLines) i++;
        var mDiv2 = document.createElement("div");
        mDiv2.className = "math math-display";
        mDiv2.textContent = mathLines.join("\n").trim();
        frag.appendChild(mDiv2);
        continue;
      }

      if (line.trim().indexOf("\\[") === 0) {
        var trimmed2 = line.trim();
        if (trimmed2.length > 2 && trimmed2.endsWith("\\]")) {
          var mDiv3 = document.createElement("div");
          mDiv3.className = "math math-display";
          mDiv3.textContent = trimmed2.slice(2, -2).trim();
          frag.appendChild(mDiv3);
          i++;
          continue;
        }
        i++;
        var mathLines2 = [];
        while (i < numLines && rawLines[i].trim().indexOf("\\]") === -1) {
          mathLines2.push(rawLines[i]);
          i++;
        }
        if (i < numLines) i++;
        var mDiv4 = document.createElement("div");
        mDiv4.className = "math math-display";
        mDiv4.textContent = mathLines2.join("\n").trim();
        frag.appendChild(mDiv4);
        continue;
      }

      // Headings: #, ##, ###
      var headingMatch = line.match(/^(#{1,3})\s+(.*)$/);
      if (headingMatch) {
        var level = headingMatch[1].length;
        var h = document.createElement("h" + level);
        h.setAttribute("dir", "auto");
        parseInline(headingMatch[2], h);
        frag.appendChild(h);
        i++;
        continue;
      }

      // Unordered lists (- or *)
      var ulMatch = line.match(/^[-*]\s+(.*)$/);
      if (ulMatch) {
        var ul = document.createElement("ul");
        while (i < numLines) {
          var itemMatch = rawLines[i].match(/^[-*]\s+(.*)$/);
          if (!itemMatch) break;
          var li = document.createElement("li");
          parseInline(itemMatch[1], li);
          ul.appendChild(li);
          i++;
        }
        frag.appendChild(ul);
        continue;
      }

      // Ordered lists (1., 2., etc.)
      var olMatch = line.match(/^(\d+)\.\s+(.*)$/);
      if (olMatch) {
        var ol = document.createElement("ol");
        while (i < numLines) {
          var itemMatch2 = rawLines[i].match(/^(\d+)\.\s+(.*)$/);
          if (!itemMatch2) break;
          var li2 = document.createElement("li");
          parseInline(itemMatch2[2], li2);
          ol.appendChild(li2);
          i++;
        }
        frag.appendChild(ol);
        continue;
      }

      // Paragraph: collect lines until blank line or special starter
      var paraLines = [];
      while (i < numLines) {
        var cur = rawLines[i];
        if (!cur.trim()) break;
        if (
          cur.match(/^```/) ||
          cur.match(/^(#{1,3})\s+/) ||
          cur.match(/^[-*]\s+/) ||
          cur.match(/^(\d+)\.\s+/) ||
          cur.trim().indexOf("$$") === 0 ||
          cur.trim().indexOf("\\[") === 0
        ) {
          break;
        }
        paraLines.push(cur);
        i++;
      }
      if (paraLines.length > 0) {
        var p = document.createElement("p");
        p.setAttribute("dir", "auto");
        parseInline(paraLines.join(" "), p);
        frag.appendChild(p);
      }
    }

    return frag;
  }

  function renderRich(container, markdownText) {
    if (!container) return;
    while (container.firstChild) {
      container.removeChild(container.firstChild);
    }
    var frag = markdownToNodes(markdownText);
    container.appendChild(frag);
    renderMathIn(container);
  }

  /* Same as Mathia.renderMath, but scoped to one container, which may not be
     attached to the document yet (a restored conversation is built detached). */
  function renderMathIn(container) {
    if (!window.katex) return;
    var nodes = container.querySelectorAll(".math");
    for (var i = 0; i < nodes.length; i++) {
      var el = nodes[i];
      if (el.dataset.mxTex !== undefined) continue;
      var tex = el.textContent;
      el.dataset.mxTex = tex;
      try {
        window.katex.render(tex, el, {
          displayMode: el.classList.contains("math-display"),
          throwOnError: false
        });
      } catch (err) {
        el.classList.add("mx-math-error");
        el.title = String(err);
      }
    }
  }

  // UI Setup (Settings toggle and dialog)
  var isRtl = (document.documentElement.dir === "rtl" || document.documentElement.getAttribute("dir") === "rtl");

  var STRINGS = {
    en: {
      toggleAria: "AI Settings",
      dialogTitle: "AI Settings",
      providerLabel: "Provider",
      modelLabel: "Model",
      refreshModels: "Refresh models",
      loadingModels: "Loading models…",
      modelsAvailable: "models available from Google.",
      modelsLoadFailed: "Could not load models. Check the API key and try again.",
      endpointLabel: "API Endpoint URL",
      endpointPlaceholder: "https://host/v1 (an OpenAI-compatible base URL)",
      modelPlaceholder: "model name as the server lists it",
      compatibleName: "OpenAI-compatible (any URL)",
      keyLabel: "API Key",
      keyLabelOptional: "API Key (optional for local servers)",
      missingEndpointNotice: "Set an API endpoint URL to ask questions.",
      noticePrefix: "Stored only in this browser; sent only to ",
      save: "Save",
      forgetKey: "Forget key",
      forgetAll: "Forget all",
      close: "Close",

      // S3 Ask Action & Conversational Panel
      askAction: "Ask",
      send: "Send",
      stop: "Stop",
      clear: "Clear",
      includeNeighbours: "Include neighbouring blocks",
      askPlaceholder: "Ask a question about this block...",
      missingKeyNotice: "API key required to ask questions.",
      configureKey: "Configure API Key...",
      chatLogAria: "Conversation history",
      inputAria: "Question about this block",
      emptyAnswer: "The model returned an empty answer.",
      offlineNotice: "You are offline; AI features need a network connection.",

      // S4 Illustrate Action
      illustrateAction: "Illustrate",
      regenerate: "Regenerate",
      drawing: "Drawing a figure…",
      noFigure: "The model did not return a usable figure.",
      figureAlt: "Explanatory figure",
      figureAria: "Figure"
    },
    fa: {
      toggleAria: "تنظیمات هوش مصنوعی",
      dialogTitle: "تنظیمات هوش مصنوعی",
      providerLabel: "ارائه‌دهنده",
      modelLabel: "مدل",
      refreshModels: "نوسازی فهرست مدل‌ها",
      loadingModels: "در حال دریافت مدل‌ها…",
      modelsAvailable: "مدل از Google در دسترس است.",
      modelsLoadFailed: "دریافت مدل‌ها ممکن نشد. کلید API را بررسی کنید و دوباره تلاش کنید.",
      endpointLabel: "نشانی وب‌سرویس",
      endpointPlaceholder: "https://host/v1 (نشانی پایه سازگار با OpenAI)",
      modelPlaceholder: "نام مدل، مطابق فهرست سرور",
      compatibleName: "سازگار با OpenAI (هر نشانی)",
      keyLabel: "کلید دسترسی (API Key)",
      keyLabelOptional: "کلید دسترسی (برای سرور محلی اختیاری)",
      missingEndpointNotice: "برای پرسش، نشانی وب‌سرویس را وارد کنید.",
      noticePrefix: "تنها در این مرورگر ذخیره می‌شود؛ فقط به ",
      noticeSuffix: " ارسال می‌شود",
      save: "ذخیره",
      forgetKey: "فراموشی کلید",
      forgetAll: "فراموشی همه",
      close: "بستن",

      // S3 Ask Action & Conversational Panel
      askAction: "پرسش",
      send: "ارسال",
      stop: "توقف",
      clear: "پاک کردن",
      includeNeighbours: "شامل بخش‌های مجاور",
      askPlaceholder: "درباره این بخش سوال کنید...",
      missingKeyNotice: "برای پرسش، کلید دسترسی (API Key) لازم است.",
      configureKey: "تنظیم کلید API...",
      chatLogAria: "تاریخچه گفتگو",
      inputAria: "متن پرسش درباره این بخش",
      emptyAnswer: "مدل پاسخی برنگرداند.",
      offlineNotice: "اتصال اینترنت برقرار نیست؛ امکانات هوش مصنوعی به شبکه نیاز دارند.",

      // S4 Illustrate Action
      illustrateAction: "شکل",
      regenerate: "رسم دوباره",
      drawing: "در حال رسم شکل…",
      noFigure: "مدل شکل قابل‌استفاده‌ای برنگرداند.",
      figureAlt: "شکل توضیحی",
      figureAria: "شکل"
    }
  };
  var t = isRtl ? STRINGS.fa : STRINGS.en;

  // Toggle button
  var toggleBtn = document.createElement("button");
  toggleBtn.id = "mx-ai-toggle";
  toggleBtn.className = "mx-ai-toggle";
  toggleBtn.type = "button";
  toggleBtn.setAttribute("aria-label", t.toggleAria);
  toggleBtn.setAttribute("title", t.toggleAria);
  toggleBtn.textContent = "⚙";
  document.body.appendChild(toggleBtn);

  // Dialog element
  var dialog = document.createElement("dialog");
  dialog.id = "mx-ai-dialog";
  dialog.className = "mx-ai-dialog";
  dialog.setAttribute("aria-label", t.dialogTitle);

  var header = document.createElement("div");
  header.className = "mx-ai-dialog-header";
  var title = document.createElement("h3");
  title.className = "mx-ai-dialog-title";
  title.textContent = t.dialogTitle;
  header.appendChild(title);
  dialog.appendChild(header);

  var dialogBody = document.createElement("div");
  dialogBody.className = "mx-ai-dialog-body";

  // Provider field
  var providerField = document.createElement("div");
  providerField.className = "mx-ai-field";
  var providerLbl = document.createElement("label");
  providerLbl.setAttribute("for", "mx-ai-provider-select");
  providerLbl.textContent = t.providerLabel;
  var providerSelect = document.createElement("select");
  providerSelect.id = "mx-ai-provider-select";
  for (var pIdx = 0; pIdx < enabledProviders.length; pIdx++) {
    var pSlug = enabledProviders[pIdx];
    var opt = document.createElement("option");
    opt.value = pSlug;
    if (pSlug === "anthropic") opt.textContent = "Anthropic (Claude)";
    else if (pSlug === "openai") opt.textContent = "OpenAI";
    else if (pSlug === "gemini") opt.textContent = "Google";
    else if (pSlug === "openai-compatible") opt.textContent = t.compatibleName;
    else opt.textContent = pSlug;
    providerSelect.appendChild(opt);
  }
  providerField.appendChild(providerLbl);
  providerField.appendChild(providerSelect);
  dialogBody.appendChild(providerField);

  // Model field
  var modelField = document.createElement("div");
  modelField.className = "mx-ai-field";
  var modelLbl = document.createElement("label");
  modelLbl.setAttribute("for", "mx-ai-model-input");
  modelLbl.textContent = t.modelLabel;
  var modelInput = document.createElement("input");
  modelInput.id = "mx-ai-model-input";
  modelInput.type = "text";
  modelInput.autocomplete = "off";
  modelField.appendChild(modelLbl);
  modelField.appendChild(modelInput);
  var googleModelList = document.createElement("datalist");
  googleModelList.id = "mx-ai-google-models";
  modelField.appendChild(googleModelList);
  var refreshModelsBtn = document.createElement("button");
  refreshModelsBtn.id = "mx-ai-refresh-models";
  refreshModelsBtn.type = "button";
  refreshModelsBtn.className = "mx-ai-btn";
  refreshModelsBtn.textContent = t.refreshModels;
  modelField.appendChild(refreshModelsBtn);
  var modelStatus = document.createElement("p");
  modelStatus.id = "mx-ai-model-status";
  modelStatus.className = "mx-ai-notice";
  modelStatus.hidden = true;
  modelField.appendChild(modelStatus);
  dialogBody.appendChild(modelField);

  // Endpoint URL field (for custom providers)
  var endpointField = document.createElement("div");
  endpointField.className = "mx-ai-field";
  endpointField.id = "mx-ai-endpoint-field";
  var endpointLbl = document.createElement("label");
  endpointLbl.setAttribute("for", "mx-ai-endpoint-input");
  endpointLbl.textContent = t.endpointLabel;
  var endpointInput = document.createElement("input");
  endpointInput.id = "mx-ai-endpoint-input";
  endpointInput.type = "url";
  endpointInput.autocomplete = "off";
  endpointField.appendChild(endpointLbl);
  endpointField.appendChild(endpointInput);
  dialogBody.appendChild(endpointField);

  // API Key field
  var keyField = document.createElement("div");
  keyField.className = "mx-ai-field";
  var keyLbl = document.createElement("label");
  keyLbl.setAttribute("for", "mx-ai-key-input");
  keyLbl.textContent = t.keyLabel;
  var keyInput = document.createElement("input");
  keyInput.id = "mx-ai-key-input";
  keyInput.type = "password";
  keyInput.autocomplete = "off";
  keyField.appendChild(keyLbl);
  keyField.appendChild(keyInput);
  dialogBody.appendChild(keyField);

  // Notice
  var noticeEl = document.createElement("p");
  noticeEl.id = "mx-ai-notice";
  noticeEl.className = "mx-ai-notice";
  dialogBody.appendChild(noticeEl);

  dialog.appendChild(dialogBody);

  // Actions
  var actions = document.createElement("div");
  actions.className = "mx-ai-dialog-actions";

  var saveBtn = document.createElement("button");
  saveBtn.type = "button";
  saveBtn.className = "mx-ai-btn mx-ai-btn-primary";
  saveBtn.textContent = t.save;
  actions.appendChild(saveBtn);

  var forgetKeyBtn = document.createElement("button");
  forgetKeyBtn.type = "button";
  forgetKeyBtn.className = "mx-ai-btn";
  forgetKeyBtn.textContent = t.forgetKey;
  actions.appendChild(forgetKeyBtn);

  var forgetAllBtn = document.createElement("button");
  forgetAllBtn.type = "button";
  forgetAllBtn.className = "mx-ai-btn";
  forgetAllBtn.textContent = t.forgetAll;
  actions.appendChild(forgetAllBtn);

  var closeBtn = document.createElement("button");
  closeBtn.type = "button";
  closeBtn.className = "mx-ai-btn";
  closeBtn.textContent = t.close;
  actions.appendChild(closeBtn);

  dialog.appendChild(actions);
  document.body.appendChild(dialog);

  function updateDialogNotice() {
    var p = providerSelect.value;
    var ep = !isBuiltin(p) ? endpointInput.value.trim() : "";
    var origin = getOrigin(p, ep);
    if (isRtl) {
      noticeEl.textContent = t.noticePrefix + origin + (t.noticeSuffix || "");
    } else {
      noticeEl.textContent = t.noticePrefix + origin;
    }
  }

  var googleModels = [];
  var googleModelsKey = "";

  function showModelStatus(message) {
    modelStatus.textContent = message || "";
    modelStatus.hidden = !message;
  }

  function renderGoogleModels() {
    googleModelList.textContent = "";
    for (var i = 0; i < googleModels.length; i++) {
      var model = googleModels[i];
      var option = document.createElement("option");
      option.value = model.name;
      option.label = model.displayName;
      option.textContent = model.displayName;
      googleModelList.appendChild(option);
    }
  }

  async function refreshGoogleModels() {
    var key = keyInput.value.trim() || ((readStorage().keys || {}).gemini || "").trim();
    if (!key) {
      showModelStatus(t.missingKeyNotice);
      return;
    }

    refreshModelsBtn.disabled = true;
    refreshModelsBtn.textContent = t.loadingModels;
    showModelStatus("");
    try {
      var models = [];
      var pageToken = "";
      do {
        var url = new URL("https://generativelanguage.googleapis.com/v1beta/models");
        url.searchParams.set("pageSize", "1000");
        if (pageToken) url.searchParams.set("pageToken", pageToken);
        var response = await fetch(url.toString(), {
          headers: { "x-goog-api-key": key }
        });
        if (!response.ok) throw new Error("models request failed");
        var payload = await response.json();
        var listed = Array.isArray(payload.models) ? payload.models : [];
        for (var i = 0; i < listed.length; i++) {
          var item = listed[i] || {};
          if (!Array.isArray(item.supportedGenerationMethods) || item.supportedGenerationMethods.indexOf("generateContent") === -1) continue;
          var name = String(item.name || "").replace(/^models\//, "");
          if (name) models.push({ name: name, displayName: String(item.displayName || name) });
        }
        pageToken = typeof payload.nextPageToken === "string" ? payload.nextPageToken : "";
      } while (pageToken);

      models.sort(function (a, b) { return a.name.localeCompare(b.name); });
      googleModels = models;
      googleModelsKey = key;
      renderGoogleModels();
      if (!modelInput.value.trim() && googleModels.length) modelInput.value = googleModels[0].name;
      showModelStatus(googleModels.length + " " + t.modelsAvailable);
    } catch (error) {
      showModelStatus(t.modelsLoadFailed);
    } finally {
      refreshModelsBtn.disabled = false;
      refreshModelsBtn.textContent = t.refreshModels;
    }
  }

  function syncDialogFields() {
    var p = providerSelect.value;
    var s = readStorage();
    modelInput.value = (s.models && s.models[p]) || DEFAULT_MODELS[p] || "";
    var isGoogle = p === "gemini";
    modelInput.toggleAttribute("list", isGoogle);
    if (isGoogle) modelInput.setAttribute("list", googleModelList.id);
    refreshModelsBtn.hidden = !isGoogle;
    if (!isGoogle) showModelStatus("");
    if (!isBuiltin(p)) {
      endpointField.style.display = "";
      endpointInput.value = (s.endpoints && s.endpoints[p]) || defaultEndpoints[p] || "";
      endpointInput.placeholder = t.endpointPlaceholder;
      modelInput.placeholder = t.modelPlaceholder;
      keyLbl.textContent = t.keyLabelOptional;
    } else {
      endpointInput.placeholder = "";
      modelInput.placeholder = "";
      keyLbl.textContent = t.keyLabel;
      endpointField.style.display = "none";
      endpointInput.value = "";
    }
    keyInput.value = (s.keys && s.keys[p]) || "";
    updateDialogNotice();
    if (isGoogle && keyInput.value.trim() && googleModelsKey !== keyInput.value.trim()) {
      refreshGoogleModels();
    }
  }

  function openSettings() {
    var p = currentProvider();
    providerSelect.value = p;
    syncDialogFields();
    if (typeof dialog.showModal === "function") {
      dialog.showModal();
    } else {
      dialog.setAttribute("open", "");
    }
  }

  function closeSettings() {
    keyInput.value = "";
    if (typeof dialog.close === "function") {
      dialog.close();
    } else {
      dialog.removeAttribute("open");
    }
  }

  toggleBtn.addEventListener("click", openSettings);

  providerSelect.addEventListener("change", syncDialogFields);
  endpointInput.addEventListener("input", updateDialogNotice);
  refreshModelsBtn.addEventListener("click", refreshGoogleModels);

  saveBtn.addEventListener("click", function () {
    var p = providerSelect.value;
    var m = modelInput.value.trim();
    var k = keyInput.value.trim();
    var ep = endpointInput.value.trim();
    configure({
      provider: p,
      model: m,
      key: k,
      endpoint: ep
    });
    closeSettings();
    updateAllPanelKeyNotices();
  });

  forgetKeyBtn.addEventListener("click", function () {
    var p = providerSelect.value;
    forgetKey(p);
    keyInput.value = "";
    updateAllPanelKeyNotices();
  });

  forgetAllBtn.addEventListener("click", function () {
    forgetKey();
    keyInput.value = "";
    updateAllPanelKeyNotices();
  });

  closeBtn.addEventListener("click", closeSettings);

  dialog.addEventListener("close", function () {
    keyInput.value = "";
    updateAllPanelKeyNotices();
  });

  dialog.addEventListener("cancel", function () {
    keyInput.value = "";
    updateAllPanelKeyNotices();
  });

  dialog.addEventListener("click", function (e) {
    if (e.target === dialog) {
      closeSettings();
    }
  });

  // =========================================================================
  // S3: "Ask" Action per Block
  // =========================================================================

  var ASK_SYSTEM_PROMPT =
    "You are a mathematics tutor helping a student read a study guide. " +
    "Answer in the language of the question. " +
    "Use $...$ for inline math and $$...$$ for display math. " +
    "Be concise, clear, and rigorous.";

  var MAX_CONTEXT_CHARS = 4000;
  var MAX_PRIMARY_CHARS = 8000;
  var CHAT_STORAGE_PREFIX = "mathia-ai-chat:";
  var conversationStore = {};

  function isValidConversationItem(item) {
    if (!item || typeof item !== "object" || Array.isArray(item)) return false;
    try {
      if (item.role !== "user" && item.role !== "assistant") return false;
      if (typeof item.content !== "string") return false;
      return true;
    } catch (e) {
      return false;
    }
  }

  function sanitizeConversationItem(item) {
    if (!isValidConversationItem(item)) return null;
    try {
      var clean = {
        role: item.role,
        content: item.content
      };
      if (typeof item.display === "string") clean.display = item.display;
      if (item.isError === true) clean.isError = true;
      return clean;
    } catch (e) {
      return null;
    }
  }

  function sanitizeConversationList(list) {
    if (!list || !Array.isArray(list)) return [];
    var cleanList = [];
    for (var i = 0; i < list.length; i++) {
      try {
        var clean = sanitizeConversationItem(list[i]);
        if (clean) cleanList.push(clean);
      } catch (e) {}
    }
    return cleanList;
  }

  function getChatStorageKey(blockId) {
    var path = "";
    try {
      path = (window.location && window.location.pathname) ? window.location.pathname : "";
    } catch (e) {}
    return CHAT_STORAGE_PREFIX + path + ":" + blockId;
  }

  function getConversation(blockId) {
    if (blockId === null || blockId === undefined) return [];
    var strId = String(blockId);
    if (!strId) return [];

    if (Array.isArray(conversationStore[strId])) {
      return conversationStore[strId].slice();
    }
    var key = getChatStorageKey(strId);
    var list = [];
    try {
      if (typeof window !== "undefined" && window.sessionStorage) {
        var raw = window.sessionStorage.getItem(key);
        if (raw) {
          var parsed = JSON.parse(raw);
          if (Array.isArray(parsed)) {
            list = sanitizeConversationList(parsed);
          }
        }
      }
    } catch (e) {}
    conversationStore[strId] = list;
    return list.slice();
  }
  var loadConversation = getConversation;

  function saveConversation(blockId, list) {
    if (blockId === null || blockId === undefined) return;
    var strId = String(blockId);
    if (!strId) return;

    var cleanList = sanitizeConversationList(list);
    conversationStore[strId] = cleanList;
    var key = getChatStorageKey(strId);
    try {
      if (typeof window !== "undefined" && window.sessionStorage) {
        window.sessionStorage.setItem(key, JSON.stringify(cleanList));
      }
    } catch (e) {}
  }

  function clearConversation(blockId) {
    if (blockId === null || blockId === undefined) return;
    var strId = String(blockId);
    delete conversationStore[strId];
    var key = getChatStorageKey(strId);
    try {
      if (typeof window !== "undefined" && window.sessionStorage) {
        window.sessionStorage.removeItem(key);
      }
    } catch (e) {}
  }

  function isSemanticBlock(block) {
    if (!block || !block.type) return false;
    return block.type !== "heading" && block.type !== "text";
  }

  function getSemanticBlocks() {
    var all = [];
    if (window.Mathia && typeof window.Mathia.blocks === "function") {
      all = window.Mathia.blocks();
    } else {
      var elements = document.querySelectorAll("[data-block-id]");
      for (var i = 0; i < elements.length; i++) {
        var el = elements[i];
        all.push({
          id: el.dataset.blockId,
          type: el.dataset.blockType,
          proves: el.dataset.proves || null,
          element: el
        });
      }
    }
    var result = [];
    for (var j = 0; j < all.length; j++) {
      if (isSemanticBlock(all[j])) {
        result.push(all[j]);
      }
    }
    return result;
  }

  function getBlockTitle(block) {
    if (!block || !block.element) return "";
    var aria = block.element.getAttribute("aria-label");
    if (aria && aria.trim()) {
      return aria.trim();
    }
    var badge = block.element.querySelector(".mx-badge");
    var titleEl = block.element.querySelector(".mx-block-title");
    var parts = [];
    if (badge && badge.textContent.trim()) parts.push(badge.textContent.trim());
    if (titleEl && titleEl.textContent.trim()) parts.push(titleEl.textContent.trim());
    if (parts.length > 0) return parts.join(" - ");
    return block.type || block.id || "";
  }

  function extractCleanBlockText(block) {
    if (!block) return "";
    var el = block.element;
    if (!el) {
      if (typeof block.text === "string") return block.text;
      return "";
    }

    var bodyEl = el.querySelector(".mx-block-body");
    var target = bodyEl || el;
    var clone = target.cloneNode(true);

    // Strip block header, AI panels, and AI figures
    var unwanted = clone.querySelectorAll(".mx-block-head, .mx-ai-panel, .mx-ai-figure-wrap, [data-mx-ai-panel]");
    for (var i = 0; i < unwanted.length; i++) {
      if (unwanted[i].parentNode) {
        unwanted[i].parentNode.removeChild(unwanted[i]);
      }
    }

    // Restore LaTeX formulas
    var mathNodes = clone.querySelectorAll(".math");
    for (var j = 0; j < mathNodes.length; j++) {
      var m = mathNodes[j];
      var tex = m.dataset.mxTex !== undefined ? m.dataset.mxTex : m.textContent;
      m.textContent = m.classList.contains("math-display") ? ("$$" + tex + "$$") : ("$" + tex + "$");
    }

    return clone.textContent.trim();
  }

  function truncateText(text, limit) {
    if (typeof text !== "string") return "";
    var max = (typeof limit === "number" && !isNaN(limit)) ? Math.max(0, Math.floor(limit)) : MAX_CONTEXT_CHARS;
    if (text.length <= max) return text;
    if (max > 0 && text.charCodeAt(max - 1) >= 0xD800 && text.charCodeAt(max - 1) <= 0xDBFF) {
      max--;
    }
    return text.slice(0, max);
  }

  function assembleInitialPrompt(currentBlock, userQuestion, includeNeighbours) {
    var parts = [];

    // 1. Primary Block
    var type = (currentBlock && currentBlock.type) || "block";
    var title = currentBlock ? getBlockTitle(currentBlock) : "";
    var header = title ? (type + " (" + title + ")") : type;
    var rawText = currentBlock ? extractCleanBlockText(currentBlock) : "";
    var text = truncateText(rawText, MAX_PRIMARY_CHARS);

    parts.push("Block: " + header);
    if (text) {
      parts.push(text);
    }

    // 2. Neighbouring Context
    if (includeNeighbours) {
      var semantics = getSemanticBlocks();
      var idx = -1;
      for (var i = 0; i < semantics.length; i++) {
        if (semantics[i].id === currentBlock.id) {
          idx = i;
          break;
        }
      }

      var prev = (idx > 0) ? semantics[idx - 1] : null;
      var next = (idx >= 0 && idx < semantics.length - 1) ? semantics[idx + 1] : null;
      var proved = null;
      if (currentBlock.proves) {
        if (window.Mathia && typeof window.Mathia.block === "function") {
          proved = window.Mathia.block(currentBlock.proves);
        }
        if (!proved) {
          for (var j = 0; j < semantics.length; j++) {
            if (semantics[j].id === currentBlock.proves) {
              proved = semantics[j];
              break;
            }
          }
        }
      }

      var contextSections = [];

      if (prev) {
        var pTitle = getBlockTitle(prev);
        var pType = prev.type || "block";
        var pHead = pTitle ? (pType + " (" + pTitle + ")") : pType;
        var pText = truncateText(extractCleanBlockText(prev), MAX_CONTEXT_CHARS);
        if (pText) {
          contextSections.push("Preceding block: " + pHead + "\n" + pText);
        }
      }

      if (next) {
        var nTitle = getBlockTitle(next);
        var nType = next.type || "block";
        var nHead = nTitle ? (nType + " (" + nTitle + ")") : nType;
        var nText = truncateText(extractCleanBlockText(next), MAX_CONTEXT_CHARS);
        if (nText) {
          contextSections.push("Next block: " + nHead + "\n" + nText);
        }
      }

      if (proved) {
        var pvTitle = getBlockTitle(proved);
        var pvType = proved.type || "block";
        var pvHead = pvTitle ? (pvType + " (" + pvTitle + ")") : pvType;
        var pvText = truncateText(extractCleanBlockText(proved), MAX_CONTEXT_CHARS);
        if (pvText) {
          contextSections.push("Proved block: " + pvHead + "\n" + pvText);
        }
      }

      if (contextSections.length > 0) {
        parts.push("Context:\n" + contextSections.join("\n\n"));
      }
    }

    // 3. User Question
    parts.push("Question: " + userQuestion.trim());

    return parts.join("\n\n");
  }

  var raf = (typeof window !== "undefined" && typeof window.requestAnimationFrame === "function")
    ? window.requestAnimationFrame.bind(window)
    : function (cb) { return setTimeout(cb, 16); };

  var cancelRaf = (typeof window !== "undefined" && typeof window.cancelAnimationFrame === "function")
    ? window.cancelAnimationFrame.bind(window)
    : clearTimeout;

  function createThrottledRenderer(container) {
    var rafId = null;
    var pendingText = "";

    function flush() {
      if (rafId !== null) {
        cancelRaf(rafId);
        rafId = null;
      }
      renderRich(container, pendingText);
    }

    function update(text) {
      pendingText = text;
      if (rafId === null) {
        rafId = raf(function () {
          rafId = null;
          renderRich(container, pendingText);
        });
      }
    }

    function finish(finalText) {
      if (finalText !== undefined) {
        pendingText = finalText;
      }
      flush();
    }

    function cancel() {
      if (rafId !== null) {
        cancelRaf(rafId);
        rafId = null;
      }
    }

    return {
      update: update,
      finish: finish,
      cancel: cancel,
      flush: flush
    };
  }

  function isOffline() {
    return typeof navigator !== "undefined" && navigator.onLine === false;
  }

  /* The one-line "cannot ask right now" message: no key (with a button that
     opens the settings) or no network. Returns the notice element. */
  function showUnavailableNotice(host, before) {
    var notice = null;
    for (var i = 0; i < host.children.length; i++) {
      if (host.children[i].classList.contains("mx-ai-missing-key")) notice = host.children[i];
    }
    if (!notice) {
      notice = document.createElement("div");
      notice.className = "mx-ai-notice mx-ai-missing-key";

      var noticeSpan = document.createElement("span");
      noticeSpan.className = "mx-ai-panel-notice-text";
      notice.appendChild(noticeSpan);

      var configBtn = document.createElement("button");
      configBtn.type = "button";
      configBtn.className = "mx-ai-btn mx-ai-btn-sm";
      configBtn.textContent = t.configureKey;
      configBtn.addEventListener("click", function () {
        openSettings();
      });
      notice.appendChild(configBtn);
      host.insertBefore(notice, before || host.firstChild);
    }
    var offline = isOffline();
    var missing = isBuiltin(currentProvider()) ? t.missingKeyNotice : t.missingEndpointNotice;
    notice.firstChild.textContent = (offline ? t.offlineNotice : missing) + " ";
    notice.lastChild.hidden = offline;
    return notice;
  }

  function hideUnavailableNotice(host) {
    for (var i = host.children.length - 1; i >= 0; i--) {
      if (host.children[i].classList.contains("mx-ai-missing-key")) {
        host.removeChild(host.children[i]);
      }
    }
  }

  function updatePanelKeyNotice(panel) {
    if (!panel) return;
    var isAvail = available();
    var input = panel.querySelector(".mx-ai-input");
    var sendBtn = panel.querySelector(".mx-ai-btn-send");
    var streaming = Boolean(panel._state && panel._state.isStreaming);

    if (isAvail) {
      hideUnavailableNotice(panel);
    } else {
      showUnavailableNotice(panel, panel.querySelector(".mx-ai-chat-log"));
    }
    if (input) input.disabled = !isAvail || streaming;
    if (sendBtn) sendBtn.disabled = !isAvail;
  }

  function updateAllPanelKeyNotices() {
    var panels = document.querySelectorAll(".mx-ai-panel");
    for (var i = 0; i < panels.length; i++) {
      updatePanelKeyNotice(panels[i]);
    }
    var figures = document.querySelectorAll(".mx-ai-figure-wrap");
    for (var j = 0; j < figures.length; j++) {
      if (typeof figures[j]._refresh === "function") figures[j]._refresh();
    }
  }

  function restoreChatLog(chatLog, blockId) {
    if (!chatLog || typeof chatLog.appendChild !== "function") return;
    try {
      while (chatLog.firstChild) {
        chatLog.removeChild(chatLog.firstChild);
      }
    } catch (e) {}

    var list;
    try {
      list = getConversation(blockId);
    } catch (e) {
      list = [];
    }
    if (!Array.isArray(list)) return;

    for (var i = 0; i < list.length; i++) {
      var item = list[i];
      if (!item || typeof item !== "object" || Array.isArray(item)) continue;

      var role;
      try { role = item.role; } catch (e) { continue; }
      if (role !== "assistant" && role !== "user") continue;

      var content;
      try { content = item.content; } catch (e) { continue; }
      if (typeof content !== "string") continue;

      try {
        var bubble = document.createElement("div");
        bubble.className = "mx-ai-bubble mx-ai-bubble-" + role;
        bubble.setAttribute("dir", "auto");

        if (item.isError === true) {
          bubble.classList.add("mx-ai-error");
          bubble.textContent = content;
        } else if (role === "assistant") {
          try {
            renderRich(bubble, content);
          } catch (errRich) {
            bubble.textContent = content;
          }
        } else {
          var displayText = (typeof item.display === "string" && item.display) ? item.display : content;
          bubble.textContent = displayText;
        }
        chatLog.appendChild(bubble);
      } catch (e) {}
    }

    try {
      chatLog.scrollTop = chatLog.scrollHeight;
    } catch (e) {}
  }

  function createAskPanel(block) {
    var blockId = block.id;
    var panel = document.createElement("div");
    panel.className = "mx-ai-panel";
    panel.setAttribute("data-mx-ai-panel", blockId);
    panel.id = "mx-ai-panel-" + blockId;
    panel.setAttribute("role", "region");
    panel.setAttribute("aria-label", t.askAction + ": " + (getBlockTitle(block) || blockId));

    var panelState = {
      history: getConversation(blockId).slice(),
      isStreaming: false,
      abortController: null
    };
    panel._state = panelState;

    // 1. Conversation Log
    var chatLog = document.createElement("div");
    chatLog.className = "mx-ai-chat-log";
    chatLog.setAttribute("role", "log");
    chatLog.setAttribute("aria-live", "polite");
    chatLog.setAttribute("aria-atomic", "false");
    chatLog.setAttribute("aria-label", t.chatLogAria);
    panel.appendChild(chatLog);

    // 2. Question Textarea
    var textarea = document.createElement("textarea");
    textarea.className = "mx-ai-input mx-ai-textarea";
    textarea.rows = 2;
    textarea.setAttribute("dir", "auto");
    textarea.placeholder = t.askPlaceholder;
    textarea.setAttribute("aria-label", t.inputAria);
    panel.appendChild(textarea);

    // 3. Controls Bar
    var controls = document.createElement("div");
    controls.className = "mx-ai-controls";

    // 3a. Context toggle checkbox
    var checkLabel = document.createElement("label");
    checkLabel.className = "mx-ai-checkbox-label";
    var checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.className = "mx-ai-context-toggle";
    checkbox.checked = true;
    var checkText = document.createElement("span");
    checkText.className = "mx-ai-checkbox-text";
    checkText.textContent = t.includeNeighbours;
    checkLabel.appendChild(checkbox);
    checkLabel.appendChild(checkText);
    controls.appendChild(checkLabel);

    // 3b. Button Group
    var btnGroup = document.createElement("div");
    btnGroup.className = "mx-ai-btn-group";

    var sendBtn = document.createElement("button");
    sendBtn.type = "button";
    sendBtn.className = "mx-ai-btn mx-ai-btn-send mx-ai-btn-primary";
    sendBtn.textContent = t.send;
    btnGroup.appendChild(sendBtn);

    var stopBtn = document.createElement("button");
    stopBtn.type = "button";
    stopBtn.className = "mx-ai-btn mx-ai-btn-stop";
    stopBtn.textContent = t.stop;
    stopBtn.hidden = true;
    btnGroup.appendChild(stopBtn);

    var clearBtn = document.createElement("button");
    clearBtn.type = "button";
    clearBtn.className = "mx-ai-btn mx-ai-btn-clear";
    clearBtn.textContent = t.clear;
    btnGroup.appendChild(clearBtn);

    controls.appendChild(btnGroup);
    panel.appendChild(controls);

    function triggerSend() {
      var question = textarea.value.trim();
      if (!question || panelState.isStreaming) return;

      if (!available()) {
        updatePanelKeyNotice(panel);
        return;
      }

      var isFirstTurn = (panelState.history.length === 0);
      var promptContent = isFirstTurn
        ? assembleInitialPrompt(block, question, checkbox.checked)
        : question;

      var userTurn = {
        role: "user",
        content: promptContent,
        display: question
      };
      panelState.history.push(userTurn);

      var userBubble = document.createElement("div");
      userBubble.className = "mx-ai-bubble mx-ai-bubble-user";
      userBubble.setAttribute("dir", "auto");
      userBubble.textContent = question;
      chatLog.appendChild(userBubble);

      textarea.value = "";
      textarea.disabled = true;

      var assistantBubble = document.createElement("div");
      assistantBubble.className = "mx-ai-bubble mx-ai-bubble-assistant";
      assistantBubble.setAttribute("dir", "auto");
      chatLog.appendChild(assistantBubble);
      chatLog.scrollTop = chatLog.scrollHeight;

      sendBtn.hidden = true;
      stopBtn.hidden = false;
      panelState.isStreaming = true;

      var controller = new AbortController();
      panelState.abortController = controller;

      var renderer = createThrottledRenderer(assistantBubble);
      var accumulatedText = "";

      var apiMessages = [];
      for (var mIdx = 0; mIdx < panelState.history.length; mIdx++) {
        var mItem = panelState.history[mIdx];
        if (mItem && typeof mItem === "object" && (mItem.role === "user" || mItem.role === "assistant") && typeof mItem.content === "string") {
          apiMessages.push({ role: mItem.role, content: mItem.content });
        }
      }

      complete({
        system: ASK_SYSTEM_PROMPT,
        messages: apiMessages,
        signal: controller.signal,
        onDelta: function (delta, fullText) {
          accumulatedText = fullText;
          renderer.update(fullText);
          chatLog.scrollTop = chatLog.scrollHeight;
        }
      })
      .then(function (finalText) {
        if (!finalText || !finalText.trim()) {
          throw new Error(t.emptyAnswer);
        }
        if (panelState.history.length > 0 && panelState.history[panelState.history.length - 1] === userTurn) {
          renderer.finish(finalText);
          panelState.history.push({
            role: "assistant",
            content: finalText
          });
          saveConversation(blockId, panelState.history);
        }
      })
      .catch(function (err) {
        renderer.cancel();
        var isAborted = (controller.signal && controller.signal.aborted) ||
                        (err && (err.name === "AbortError" || err.message === "Aborted"));
        var hasActiveUserTurn = (panelState.history.length > 0 &&
                                 panelState.history[panelState.history.length - 1] === userTurn);

        if (isAborted) {
          if (accumulatedText.trim() && hasActiveUserTurn) {
            renderer.finish(accumulatedText);
            panelState.history.push({
              role: "assistant",
              content: accumulatedText
            });
            saveConversation(blockId, panelState.history);
          } else {
            if (hasActiveUserTurn) {
              panelState.history.pop();
              saveConversation(blockId, panelState.history);
            }
            if (assistantBubble.parentNode) {
              assistantBubble.parentNode.removeChild(assistantBubble);
            }
          }
        } else {
          if (hasActiveUserTurn) {
            panelState.history.pop();
            saveConversation(blockId, panelState.history);
          }
          assistantBubble.classList.add("mx-ai-error");
          assistantBubble.textContent = (err && err.message) ? err.message : "Error generating answer.";
        }
      })
      .then(function () {
        if (panelState.abortController === controller) {
          panelState.isStreaming = false;
          panelState.abortController = null;
          sendBtn.hidden = false;
          stopBtn.hidden = true;
          textarea.disabled = !available();
          if (!textarea.disabled) {
            textarea.focus();
          }
        }
      });
    }

    sendBtn.addEventListener("click", triggerSend);

    stopBtn.addEventListener("click", function () {
      if (panelState.abortController) {
        try { panelState.abortController.abort(); } catch (e) {}
      }
    });

    clearBtn.addEventListener("click", function () {
      if (panelState.isStreaming && panelState.abortController) {
        try { panelState.abortController.abort(); } catch (e) {}
      }
      panelState.isStreaming = false;
      panelState.abortController = null;
      panelState.history = [];
      clearConversation(blockId);
      while (chatLog.firstChild) {
        chatLog.removeChild(chatLog.firstChild);
      }
      sendBtn.hidden = false;
      stopBtn.hidden = true;
      textarea.value = "";
      textarea.disabled = !available();
      if (!textarea.disabled) {
        textarea.focus();
      }
    });

    textarea.addEventListener("keydown", function (e) {
      if (e.isComposing || e.keyCode === 229) return;

      if (e.key === "Escape") {
        if (panelState.isStreaming && panelState.abortController) {
          try { panelState.abortController.abort(); } catch (err) {}
        }
        return;
      }

      var isEnterSubmit = (e.key === "Enter" && !e.shiftKey) ||
                          (e.key === "Enter" && (e.ctrlKey || e.metaKey));
      if (isEnterSubmit) {
        e.preventDefault();
        if (!sendBtn.disabled && !sendBtn.hidden) {
          triggerSend();
        }
      }
    });

    try { restoreChatLog(chatLog, blockId); } catch (e) {}
    updatePanelKeyNotice(panel);

    return panel;
  }

  function toggleAskPanel(block) {
    if (!block) return;
    var blockEl = (block.nodeType === 1) ? block : block.element;
    if (!blockEl) return;
    var blockId = (block.nodeType === 1) ? ((block.dataset && block.dataset.blockId) || block.id) : block.id;
    var blockDesc = (block.nodeType === 1) ? { id: blockId, element: blockEl, type: (block.dataset && block.dataset.blockType) || "block" } : block;
    var panel = blockEl.querySelector('.mx-ai-panel[data-mx-ai-panel="' + blockId + '"]');
    var askBtn = blockEl.querySelector('button[data-mx-action="ask"]');

    if (panel) {
      var willOpen = panel.hidden;
      panel.hidden = !willOpen;
      if (askBtn) {
        askBtn.setAttribute("aria-expanded", String(willOpen));
        askBtn.classList.toggle("mx-active", willOpen);
      }
      if (willOpen) {
        updatePanelKeyNotice(panel);
        var input = panel.querySelector(".mx-ai-input");
        if (input && !input.disabled) {
          input.focus();
        }
      }
      return;
    }

    panel = createAskPanel(blockDesc);
    blockEl.appendChild(panel);

    if (askBtn) {
      askBtn.setAttribute("aria-expanded", "true");
      askBtn.setAttribute("aria-controls", panel.id);
      askBtn.classList.add("mx-active");
    }

    var textarea = panel.querySelector(".mx-ai-input");
    if (textarea && !textarea.disabled) {
      textarea.focus();
    }
  }

  // =========================================================================
  // S4: "Illustrate" Action (explanatory SVG)
  // =========================================================================

  var ILLUSTRATE_SYSTEM_PROMPT =
    "You draw explanatory figures for a mathematics study guide. " +
    "Reply with exactly one self-contained SVG inside a ```svg fence, followed by a caption of at most three sentences. " +
    "The SVG must have a viewBox and the xmlns attribute, and must not contain scripts, style elements, " +
    "foreignObject, images, animations or references to external resources. " +
    "Use only basic shapes, paths, markers, gradients and text; use currentColor for strokes and text " +
    "and assume a white background. " +
    "LaTeX is not rendered inside the figure: write labels as plain Unicode text. " +
    "Write the labels and the caption in the language of the block; the caption may use $...$ for math.";

  var SVG_STORAGE_PREFIX = "mathia-ai-svg:";

  function fnv1a(text) {
    var h = 0x811c9dc5;
    for (var i = 0; i < text.length; i++) {
      h ^= text.charCodeAt(i);
      h = Math.imul(h, 0x01000193) >>> 0;
    }
    return ("0000000" + h.toString(16)).slice(-8);
  }

  function getSvgStoragePrefix(blockId) {
    var path = "";
    try {
      path = (window.location && window.location.pathname) ? window.location.pathname : "";
    } catch (e) {}
    return SVG_STORAGE_PREFIX + path + ":" + blockId + ":";
  }

  /* The key carries a hash of the block text, so a recompiled document with a
     changed block does not show a figure drawn for the old text. */
  function getSvgStorageKey(block) {
    return getSvgStoragePrefix(block.id) + fnv1a(extractCleanBlockText(block));
  }

  function loadFigure(block) {
    try {
      var raw = window.localStorage.getItem(getSvgStorageKey(block));
      if (!raw) return null;
      var parsed = JSON.parse(raw);
      if (!parsed || typeof parsed.svg !== "string") return null;
      /* Stored data is not trusted either: it goes through the sanitiser again. */
      var svg = sanitizeSvg(parsed.svg);
      if (!svg) return null;
      return { svg: svg, caption: typeof parsed.caption === "string" ? parsed.caption : "" };
    } catch (e) {
      return null;
    }
  }

  function saveFigure(block, figure) {
    try {
      var store = window.localStorage;
      var key = getSvgStorageKey(block);
      var prefix = getSvgStoragePrefix(block.id);
      var stale = [];
      for (var i = 0; i < store.length; i++) {
        var k = store.key(i);
        if (k && k !== key && k.indexOf(prefix) === 0) stale.push(k);
      }
      for (var j = 0; j < stale.length; j++) store.removeItem(stale[j]);
      store.setItem(key, JSON.stringify({ svg: figure.svg, caption: figure.caption }));
    } catch (e) {
      /* quota exceeded or private mode: the figure just is not cached */
    }
  }

  /* Model answer -> {svg, caption}, or null when there is no usable SVG. */
  function parseFigureResponse(text) {
    if (typeof text !== "string") return null;
    var match = text.match(/<svg\b[\s\S]*?<\/svg\s*>/i);
    if (!match) return null;
    var svg = sanitizeSvg(match[0]);
    if (!svg) return null;
    var caption = text.slice(match.index + match[0].length)
      .replace(/^\s*```[^\n]*/, "")
      .trim();
    return { svg: svg, caption: caption };
  }

  function svgDataUrl(svg) {
    return "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svg);
  }

  function assembleIllustratePrompt(block) {
    var title = getBlockTitle(block);
    var type = block.type || "block";
    var rtl = block.element && block.element.getAttribute("dir") === "rtl";
    return [
      "Block: " + (title ? type + " (" + title + ")" : type),
      truncateText(extractCleanBlockText(block), MAX_PRIMARY_CHARS),
      "Language of the block: " + (rtl ? "Persian" : "English") + ".",
      "Draw one figure that helps a student understand this block."
    ].join("\n\n");
  }

  function createFigureWrap(block) {
    var wrap = document.createElement("div");
    wrap.className = "mx-ai-figure-wrap";
    wrap.setAttribute("data-mx-ai-figure", block.id);
    wrap.id = "mx-ai-figure-" + block.id;
    wrap.setAttribute("role", "region");
    wrap.setAttribute("aria-label", t.figureAria + ": " + (getBlockTitle(block) || block.id));

    var state = { figure: loadFigure(block), controller: null };
    wrap._state = state;

    var status = document.createElement("div");
    status.className = "mx-ai-figure-status";
    status.setAttribute("role", "status");
    status.setAttribute("dir", "auto");
    status.hidden = true;
    wrap.appendChild(status);

    /* The figure is shown through <img>: an SVG loaded that way cannot run
       script or fetch anything, whatever got past the sanitiser. */
    var box = document.createElement("div");
    box.className = "mx-ai-figure-box";
    var img = document.createElement("img");
    img.className = "mx-ai-figure";
    box.appendChild(img);
    wrap.appendChild(box);

    var caption = document.createElement("div");
    caption.className = "mx-ai-figure-caption";
    wrap.appendChild(caption);

    var controls = document.createElement("div");
    controls.className = "mx-ai-figure-controls";
    var regenBtn = document.createElement("button");
    regenBtn.type = "button";
    regenBtn.className = "mx-ai-btn mx-ai-btn-sm mx-ai-btn-regenerate";
    regenBtn.setAttribute("data-mx-ai-action", "illustrate-regenerate");
    var stopBtn = document.createElement("button");
    stopBtn.type = "button";
    stopBtn.className = "mx-ai-btn mx-ai-btn-sm mx-ai-btn-stop";
    stopBtn.textContent = t.stop;
    controls.appendChild(regenBtn);
    controls.appendChild(stopBtn);
    wrap.appendChild(controls);

    function setStatus(text, isError) {
      status.textContent = text || "";
      status.hidden = !text;
      status.classList.toggle("mx-ai-error", Boolean(text && isError));
    }

    function refresh() {
      var busy = Boolean(state.controller);
      var avail = available();
      var fig = state.figure;
      if (fig) {
        var src = svgDataUrl(fig.svg);
        if (img.getAttribute("src") !== src) img.setAttribute("src", src);
        img.setAttribute("alt", fig.caption || t.figureAlt);
        if (caption._text !== fig.caption) {
          caption._text = fig.caption;
          renderRich(caption, fig.caption);
        }
      }
      box.hidden = !fig;
      caption.hidden = !(fig && fig.caption);
      regenBtn.textContent = fig ? t.regenerate : t.illustrateAction;
      regenBtn.hidden = busy || !avail;
      stopBtn.hidden = !busy;
      if (!avail && !fig && !busy) {
        showUnavailableNotice(wrap, wrap.firstChild);
      } else {
        hideUnavailableNotice(wrap);
      }
    }

    function generate() {
      if (state.controller) return;
      if (!available()) {
        refresh();
        return;
      }
      var controller = new AbortController();
      state.controller = controller;
      setStatus(t.drawing, false);
      refresh();

      complete({
        system: ILLUSTRATE_SYSTEM_PROMPT,
        messages: [{ role: "user", content: assembleIllustratePrompt(block) }],
        signal: controller.signal
      })
        .then(function (text) {
          var figure = parseFigureResponse(text);
          if (!figure) throw new Error(t.noFigure);
          state.figure = figure;
          saveFigure(block, figure);
          setStatus("", false);
        })
        .catch(function (err) {
          if (controller.signal.aborted) {
            setStatus("", false);
          } else {
            setStatus((err && err.message) ? err.message : t.noFigure, true);
          }
        })
        .then(function () {
          if (state.controller === controller) state.controller = null;
          refresh();
        });
    }

    regenBtn.addEventListener("click", generate);
    stopBtn.addEventListener("click", function () {
      if (state.controller) {
        try { state.controller.abort(); } catch (e) {}
      }
    });

    wrap._refresh = refresh;
    wrap._generate = generate;
    refresh();
    return wrap;
  }

  function toggleFigure(block) {
    if (!block || !block.element) return;
    var blockEl = block.element;
    var button = blockEl.querySelector('button[data-mx-action="illustrate"]');
    var wrap = null;
    for (var i = 0; i < blockEl.children.length; i++) {
      if (blockEl.children[i].classList.contains("mx-ai-figure-wrap")) wrap = blockEl.children[i];
    }

    if (wrap) {
      var willOpen = wrap.hidden;
      wrap.hidden = !willOpen;
      if (button) button.setAttribute("aria-expanded", String(willOpen));
      if (willOpen) {
        wrap._refresh();
        if (!wrap._state.figure) wrap._generate();
      }
      return;
    }

    wrap = createFigureWrap(block);
    var bodyEl = blockEl.querySelector(".mx-block-body");
    blockEl.insertBefore(wrap, bodyEl ? bodyEl.nextSibling : null);
    if (button) {
      button.setAttribute("aria-expanded", "true");
      button.setAttribute("aria-controls", wrap.id);
    }
    /* A cached figure shows at once; otherwise ask for one. */
    if (!wrap._state.figure) wrap._generate();
  }

  function tagAiActionButtons(actionId) {
    var buttons = document.querySelectorAll('button[data-mx-action="' + actionId + '"]');
    for (var i = 0; i < buttons.length; i++) {
      var btn = buttons[i];
      btn.setAttribute("data-mx-ai-action", actionId);
      if (!btn.hasAttribute("aria-expanded")) {
        btn.setAttribute("aria-expanded", "false");
      }
    }
  }

  if (window.Mathia && typeof window.Mathia.registerAction === "function") {
    window.Mathia.registerAction({
      id: "ask",
      label: t.askAction,
      run: function (block) {
        toggleAskPanel(block);
      }
    });
    tagAiActionButtons("ask");

    window.Mathia.registerAction({
      id: "illustrate",
      label: t.illustrateAction,
      run: function (block) {
        toggleFigure(block);
      }
    });
    tagAiActionButtons("illustrate");
  }

  // Expose Mathia.ai
  if (!window.Mathia) {
    window.Mathia = {};
  }

  window.Mathia.ai = {
    providers: enabledProviders,
    settings: settings,
    configure: configure,
    forgetKey: forgetKey,
    available: available,
    complete: complete,
    renderRich: renderRich,
    _internal: {
      parseSSE: parseSSE,
      buildRequest: buildRequest,
      extractDelta: extractDelta,
      sanitizeSvg: sanitizeSvg,
      markdownToNodes: markdownToNodes,
      assembleInitialPrompt: assembleInitialPrompt,
      extractCleanBlockText: extractCleanBlockText,
      getSemanticBlocks: getSemanticBlocks,
      truncateText: truncateText,
      createThrottledRenderer: createThrottledRenderer,
      ASK_SYSTEM_PROMPT: ASK_SYSTEM_PROMPT,
      getChatStorageKey: getChatStorageKey,
      MAX_CONTEXT_CHARS: MAX_CONTEXT_CHARS,
      MAX_PRIMARY_CHARS: MAX_PRIMARY_CHARS,
      getConversation: getConversation,
      loadConversation: getConversation,
      saveConversation: saveConversation,
      clearConversation: clearConversation,
      restoreChatLog: restoreChatLog,
      sanitizeConversationList: sanitizeConversationList,
      ILLUSTRATE_SYSTEM_PROMPT: ILLUSTRATE_SYSTEM_PROMPT,
      assembleIllustratePrompt: assembleIllustratePrompt,
      parseFigureResponse: parseFigureResponse,
      getSvgStorageKey: getSvgStorageKey,
      fnv1a: fnv1a
    }
  };
})();
