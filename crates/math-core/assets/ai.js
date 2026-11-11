/* Mathia AI interactive runtime (S2: settings, providers, streaming, safe rendering). */
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
  }
  updateOnlineStatus();
  window.addEventListener("online", updateOnlineStatus);
  window.addEventListener("offline", updateOnlineStatus);

  var DEFAULT_MODELS = {
    anthropic: "claude-sonnet-5-5",
    openai: "gpt-5-mini",
    gemini: "gemini-3.8-flash"
  };

  var BUILTIN_ORIGINS = {
    anthropic: "https://api." + "anthropic." + "com",
    openai: "https://api.openai.com",
    gemini: "https://generativelanguage.googleapis.com"
  };

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

  function isLocalEndpoint(url) {
    if (!url) return false;
    try {
      var base = window.location && window.location.href ? window.location.href : "http://localhost";
      var u = new URL(url, base);
      return (
        u.hostname === "localhost" ||
        u.hostname === "127.0.0.1" ||
        u.hostname === "[::1]" ||
        (u.hostname.length >= 6 && u.hostname.slice(-6) === ".local")
      );
    } catch (e) {
      return false;
    }
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
    if (hasKeyFor(p)) return true;
    if (p !== "anthropic" && p !== "openai" && p !== "gemini") {
      var ep = getEndpoint(p);
      if (isLocalEndpoint(ep)) return true;
    }
    return false;
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
    var ep = (options.endpoint || (provider === "openai" ? "https://api.openai.com/v1" : getEndpoint(provider))).replace(/\/+$/, "");
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
  var ALLOWED_SVG_ELEMENTS = [
    "svg", "g", "defs", "title", "desc", "path", "rect", "circle", "ellipse",
    "line", "polyline", "polygon", "text", "tspan", "marker", "linearGradient",
    "radialGradient", "stop", "clipPath", "mask", "pattern", "symbol", "use"
  ];

  function sanitizeSvg(svgText) {
    if (typeof svgText !== "string" || !svgText.trim()) return null;

    var parser = new DOMParser();
    var doc = parser.parseFromString(svgText, "image/svg+xml");
    if (doc.querySelector("parsererror")) {
      return null;
    }

    var root = doc.documentElement;
    if (!root || root.tagName.toLowerCase() !== "svg") {
      return null;
    }

    var allowedTags = {};
    for (var i = 0; i < ALLOWED_SVG_ELEMENTS.length; i++) {
      allowedTags[ALLOWED_SVG_ELEMENTS[i].toLowerCase()] = true;
    }

    function sanitizeElement(el) {
      var tag = el.tagName.toLowerCase();
      if (!allowedTags[tag]) {
        el.parentNode.removeChild(el);
        return;
      }

      var attrs = Array.prototype.slice.call(el.attributes);
      for (var j = 0; j < attrs.length; j++) {
        var attr = attrs[j];
        var name = attr.name.toLowerCase();
        var val = attr.value;

        // Disallow on* event handlers
        if (name.indexOf("on") === 0) {
          el.removeAttribute(attr.name);
          continue;
        }

        // href and xlink:href must start with #
        if (name === "href" || name === "xlink:href" || name.slice(-5) === ":href") {
          if (!val.trim().startsWith("#")) {
            el.removeAttribute(attr.name);
            continue;
          }
        }

        // Check style and url(...)
        if (name === "style" || /url\s*\(/i.test(val)) {
          if (/url\s*\(/i.test(val)) {
            var urls = val.match(/url\s*\([^)]*\)/gi) || [];
            var safe = true;
            for (var u = 0; u < urls.length; u++) {
              if (!/^url\s*\(\s*['"]?#[^'"]*['"]?\s*\)$/i.test(urls[u])) {
                safe = false;
                break;
              }
            }
            if (!safe) {
              el.removeAttribute(attr.name);
              continue;
            }
          }
          if (name === "style" && /javascript:|expression|behavior/i.test(val)) {
            el.removeAttribute(attr.name);
            continue;
          }
        }
      }

      var children = Array.prototype.slice.call(el.childNodes);
      for (var k = 0; k < children.length; k++) {
        var child = children[k];
        if (child.nodeType === 1 /* Element */) {
          sanitizeElement(child);
        } else if (child.nodeType === 7 || child.nodeType === 8) {
          // comments and processing instructions
          child.parentNode.removeChild(child);
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
    if (window.Mathia && typeof window.Mathia.renderMath === "function") {
      window.Mathia.renderMath();
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
      endpointLabel: "API Endpoint URL",
      keyLabel: "API Key",
      noticePrefix: "Stored only in this browser; sent only to ",
      save: "Save",
      forgetKey: "Forget key",
      forgetAll: "Forget all",
      close: "Close"
    },
    fa: {
      toggleAria: "تنظیمات هوش مصنوعی",
      dialogTitle: "تنظیمات هوش مصنوعی",
      providerLabel: "ارائه‌دهنده",
      modelLabel: "مدل",
      endpointLabel: "نشانی وب‌سرویس",
      keyLabel: "کلید دسترسی (API Key)",
      noticePrefix: "تنها در این مرورگر ذخیره می‌شود؛ فقط به ",
      noticeSuffix: " ارسال می‌شود",
      save: "ذخیره",
      forgetKey: "فراموشی کلید",
      forgetAll: "فراموشی همه",
      close: "بستن"
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
    else if (pSlug === "gemini") opt.textContent = "Google Gemini";
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
    var ep = (p !== "anthropic" && p !== "openai" && p !== "gemini") ? endpointInput.value.trim() : "";
    var origin = getOrigin(p, ep);
    if (isRtl) {
      noticeEl.textContent = t.noticePrefix + origin + (t.noticeSuffix || "");
    } else {
      noticeEl.textContent = t.noticePrefix + origin;
    }
  }

  function syncDialogFields() {
    var p = providerSelect.value;
    var s = readStorage();
    modelInput.value = (s.models && s.models[p]) || DEFAULT_MODELS[p] || "";
    if (p !== "anthropic" && p !== "openai" && p !== "gemini") {
      endpointField.style.display = "";
      endpointInput.value = (s.endpoints && s.endpoints[p]) || defaultEndpoints[p] || "";
    } else {
      endpointField.style.display = "none";
      endpointInput.value = "";
    }
    keyInput.value = (s.keys && s.keys[p]) || "";
    updateDialogNotice();
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
  });

  forgetKeyBtn.addEventListener("click", function () {
    var p = providerSelect.value;
    forgetKey(p);
    keyInput.value = "";
  });

  forgetAllBtn.addEventListener("click", function () {
    forgetKey();
    keyInput.value = "";
  });

  closeBtn.addEventListener("click", closeSettings);

  dialog.addEventListener("close", function () {
    keyInput.value = "";
  });

  dialog.addEventListener("cancel", function () {
    keyInput.value = "";
  });

  dialog.addEventListener("click", function (e) {
    if (e.target === dialog) {
      closeSettings();
    }
  });

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
      markdownToNodes: markdownToNodes
    }
  };
})();
