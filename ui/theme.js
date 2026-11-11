(() => {
  "use strict";

  const STORAGE_KEY = "color-scheme";
  const meta = document.querySelector('meta[name="color-scheme"]');
  const darkMedia = window.matchMedia
    ? window.matchMedia("(prefers-color-scheme: dark)")
    : null;

  function readStored() {
    try {
      const value = localStorage.getItem(STORAGE_KEY);
      return value === "light" || value === "dark" ? value : null;
    } catch {
      return null;
    }
  }

  function writeStored(value) {
    try {
      if (value === "light" || value === "dark") {
        localStorage.setItem(STORAGE_KEY, value);
      } else {
        localStorage.removeItem(STORAGE_KEY);
      }
    } catch {
      /* Storage may be blocked; keep applying the override in memory. */
    }
  }

  // Apply synchronously before the stylesheet and body are parsed so there is
  // no flash of the wrong scheme on startup.
  let activeChoice = readStored();
  if (meta) {
    meta.content = activeChoice ?? "light dark";
  }

  function mediaOsTheme() {
    return darkMedia && darkMedia.matches ? "dark" : "light";
  }

  let osTheme = mediaOsTheme();

  function setButtonTitle(button, renderedTheme) {
    if (!button) return;
    button.title = renderedTheme === "dark"
      ? "Switch to light theme"
      : "Switch to dark theme";
  }

  async function syncWindowTheme(choice) {
    const invoke = window.__TAURI__?.core?.invoke;
    if (typeof invoke !== "function") return mediaOsTheme();
    try {
      const reportedOs = await invoke("set_window_theme", { theme: choice });
      if (reportedOs === "light" || reportedOs === "dark") {
        osTheme = reportedOs;
      }
    } catch {
      /* Ignore if running outside Tauri or before native sync is ready. */
    }
    return osTheme;
  }

  async function applyTheme(choice, button) {
    activeChoice = choice;
    if (meta) {
      meta.content = choice ?? "light dark";
    }
    setButtonTitle(button, choice ?? osTheme);
    const reportedOs = await syncWindowTheme(choice);
    const effective = choice ?? reportedOs;
    setButtonTitle(button, effective);
  }

  document.addEventListener("DOMContentLoaded", () => {
    const button = document.getElementById("theme-toggle");

    // Apply initial button label synchronously, then sync the native window frame.
    applyTheme(activeChoice, button);

    let toggling = false;
    if (button) {
      button.addEventListener("click", async () => {
        if (toggling) return;
        toggling = true;
        try {
          const stored = readStored();
          // Refresh ground-truth OS theme before deciding whether toggling reverts to system.
          const currentOs = await syncWindowTheme(stored);
          const currentRendered = stored ?? currentOs;
          const target = currentRendered === "dark" ? "light" : "dark";
          const next = target === currentOs ? null : target;
          writeStored(next);
          await applyTheme(next, button);
        } finally {
          toggling = false;
        }
      });
    }

    window.addEventListener("storage", (event) => {
      if (event.key === STORAGE_KEY || event.key === null) {
        applyTheme(readStored(), button);
      }
    });

    if (darkMedia) {
      darkMedia.addEventListener("change", (event) => {
        // Only treat media changes as OS changes when not pinned to an override,
        // because NSWindow appearance overrides also affect WKWebView's media query.
        if (readStored() === null) {
          osTheme = event.matches ? "dark" : "light";
          setButtonTitle(button, osTheme);
        }
      });
    }
  });
})();
