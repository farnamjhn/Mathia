#!/usr/bin/env bash
# Fetches a pinned KaTeX release, verifies its SHA-256 checksum,
# extracts it to crates/math-core/assets/katex/, and prepares inlined font assets.

set -euo pipefail

KATEX_VERSION="0.16.22"
KATEX_SHA256="055cd79635251419ed908c334582a41dee341d8af05b4c5588277e145127a78f"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DEST_DIR="$REPO_ROOT/crates/math-core/assets/katex"

command -v curl >/dev/null 2>&1 || { echo >&2 "Error: curl is required"; exit 1; }
command -v tar >/dev/null 2>&1 || { echo >&2 "Error: tar is required"; exit 1; }

compute_sha256() {
    local file="$1"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$file" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$file" | awk '{print $1}'
    else
        echo >&2 "Error: neither sha256sum nor shasum found"
        exit 1
    fi
}

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

ARCHIVE="$TMP_DIR/katex.tar.gz"
URL="https://github.com/KaTeX/KaTeX/releases/download/v${KATEX_VERSION}/katex.tar.gz"

echo "Downloading KaTeX v${KATEX_VERSION}..."
curl -sSL --fail "$URL" -o "$ARCHIVE"

echo "Verifying SHA-256 checksum..."
ACTUAL_SHA256="$(compute_sha256 "$ARCHIVE")"
if [[ "$ACTUAL_SHA256" != "$KATEX_SHA256" ]]; then
    echo >&2 "Error: SHA-256 mismatch!"
    echo >&2 "  Expected: $KATEX_SHA256"
    echo >&2 "  Actual:   $ACTUAL_SHA256"
    exit 1
fi
echo "Checksum OK: $ACTUAL_SHA256"

echo "Extracting to $DEST_DIR..."
rm -rf "$DEST_DIR"
mkdir -p "$DEST_DIR"
tar -xzf "$ARCHIVE" --strip-components=1 -C "$DEST_DIR"

# Record checksum and version
echo "$KATEX_SHA256  katex.tar.gz" > "$DEST_DIR/SHA256SUMS"
echo "$KATEX_VERSION" > "$DEST_DIR/VERSION"

echo "Preparing inlined CSS (embedding WOFF2 fonts)..."
python3 - <<'PY' "$DEST_DIR"
import sys, re, base64
from pathlib import Path

katex_dir = Path(sys.argv[1])
css_path = katex_dir / "katex.min.css"
if not css_path.exists():
    sys.exit(f"Missing {css_path}")

css = css_path.read_text(encoding="utf-8")

out = []
rest = css
while "src:" in rest:
    at = rest.index("src:")
    out.append(rest[:at + 4])
    after = rest[at + 4:]
    end = len(after)
    for sep in (";", "}"):
        idx = after.find(sep)
        if idx != -1 and idx < end:
            end = idx
    sources = after[:end]
    tail = after[end:]

    woff2 = None
    for src in sources.split(","):
        if "url(" in src:
            open_idx = src.index("url(") + 4
            close_idx = src.find(")", open_idx)
            if close_idx != -1:
                p = src[open_idx:close_idx].strip().strip("\"'")
                if p.endswith(".woff2"):
                    woff2 = p
                    break
    if woff2:
        font_path = katex_dir / woff2
        b64 = base64.b64encode(font_path.read_bytes()).decode("ascii")
        out.append(f'url(data:font/woff2;base64,{b64}) format("woff2")')
    else:
        out.append(sources)
    rest = tail

out.append(rest)
prepared_css = "".join(out)
(katex_dir / "katex.prepared.css").write_text(prepared_css, encoding="utf-8")
print(f"Generated katex.prepared.css ({len(prepared_css)} bytes)")
PY

echo "Successfully vendored KaTeX v${KATEX_VERSION} in $DEST_DIR"
