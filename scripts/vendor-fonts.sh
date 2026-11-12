#!/usr/bin/env bash
# Fetches a pinned Vazirmatn release, verifies its SHA-256 checksum, subsets
# the variable font to Persian/Arabic + Latin, and writes it as WOFF2 to
# crates/math-core/assets/fonts/ (git-ignored), where the `bundled-fonts`
# cargo feature picks it up.
#
# Subsetting needs fontTools with WOFF2 support; the script installs a pinned
# copy into a throwaway virtualenv, so nothing is added to your Python.

set -euo pipefail

VAZIRMATN_VERSION="33.003"
VAZIRMATN_SHA256="0a9afd41967e6f57096a56a181a23f81a2b999b62f1f2a4e4b26736580854fdb"
FONTTOOLS_VERSION="4.60.1"
BROTLI_VERSION="1.1.0"

# Arabic script (Persian letters, Persian digits, presentation forms), the
# joiners and direction marks, Latin-1, general punctuation, and the few
# symbols prose uses. Mathematics is set by KaTeX's own fonts.
UNICODES="U+0020-007E,U+00A0-00FF,U+0131,U+0152-0153,U+02C6,U+02DA,U+02DC,\
U+0600-06FF,U+0750-077F,U+08A0-08FF,U+FB50-FDFF,U+FE70-FEFF,\
U+200C-200F,U+2010-2027,U+202A-202E,U+2030-205E,U+2066-2069,\
U+20AC,U+2122,U+2190-2193,U+2212,U+25CC"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DEST_DIR="$REPO_ROOT/crates/math-core/assets/fonts"

for tool in curl unzip python3; do
    command -v "$tool" >/dev/null 2>&1 || { echo >&2 "Error: $tool is required"; exit 1; }
done

compute_sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

TMP_DIR="$(mktemp -d)"
# Git Bash on Windows: native Python cannot resolve MSYS paths like /tmp/...
if command -v cygpath >/dev/null 2>&1; then
    TMP_DIR="$(cygpath -m "$TMP_DIR")"
    DEST_DIR="$(cygpath -m "$DEST_DIR")"
fi
trap 'rm -rf "$TMP_DIR"' EXIT

ARCHIVE="$TMP_DIR/vazirmatn.zip"
URL="https://github.com/rastikerdar/vazirmatn/releases/download/v${VAZIRMATN_VERSION}/vazirmatn-v${VAZIRMATN_VERSION}.zip"

echo "Downloading Vazirmatn v${VAZIRMATN_VERSION}..."
curl -sSL --fail "$URL" -o "$ARCHIVE"

echo "Verifying SHA-256 checksum..."
ACTUAL_SHA256="$(compute_sha256 "$ARCHIVE")"
if [[ "$ACTUAL_SHA256" != "$VAZIRMATN_SHA256" ]]; then
    echo >&2 "Error: SHA-256 mismatch!"
    echo >&2 "  Expected: $VAZIRMATN_SHA256"
    echo >&2 "  Actual:   $ACTUAL_SHA256"
    exit 1
fi
echo "Checksum OK: $ACTUAL_SHA256"

unzip -q "$ARCHIVE" -d "$TMP_DIR/src"

echo "Installing fontTools ${FONTTOOLS_VERSION} into a temporary virtualenv..."
python3 -m venv "$TMP_DIR/venv"
VENV_BIN="$TMP_DIR/venv/bin"
[[ -d "$VENV_BIN" ]] || VENV_BIN="$TMP_DIR/venv/Scripts" # Windows
"$VENV_BIN/python" -m pip install --quiet --disable-pip-version-check \
    "fonttools==${FONTTOOLS_VERSION}" "brotli==${BROTLI_VERSION}"

echo "Subsetting..."
mkdir -p "$DEST_DIR"
"$VENV_BIN/python" -m fontTools.subset "$TMP_DIR/src/fonts/variable/Vazirmatn[wght].ttf" \
    --unicodes="$UNICODES" \
    --layout-features='*' \
    --flavor=woff2 \
    --no-hinting \
    --desubroutinize \
    --output-file="$DEST_DIR/Vazirmatn.woff2"

cp "$TMP_DIR/src/OFL.txt" "$DEST_DIR/OFL.txt"
echo "$VAZIRMATN_SHA256  vazirmatn-v${VAZIRMATN_VERSION}.zip" > "$DEST_DIR/SHA256SUMS"
echo "$VAZIRMATN_VERSION" > "$DEST_DIR/VERSION"

SIZE=$(wc -c < "$DEST_DIR/Vazirmatn.woff2" | tr -d ' ')
echo "Done: $DEST_DIR/Vazirmatn.woff2 (${SIZE} bytes, SIL Open Font License in OFL.txt)"
