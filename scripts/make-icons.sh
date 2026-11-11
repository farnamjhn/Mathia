#!/usr/bin/env bash
# Regenerates every app icon from scripts/icon-svg.py.
#
#   src-tauri/icons/source/mathia-{light,dark}.svg   the masters
#   src-tauri/icons/{32x32,128x128,128x128@2x,icon}.png, icon.ico, icon.icns
#                                                    bundle icons (light)
#   src-tauri/icons/icon-dark.icns                   dark .icns, for reference
#   src-tauri/icons/dock-{light,dark}.png            Dock icons the app swaps
#                                                    between at runtime (macOS)
#   ui/assets/mathia-{light,dark}.png                in-app logo and favicon
#
# Needs node/npx (resvg for rendering, @tauri-apps/cli for icns/ico) and, on
# macOS, iconutil for the dark .icns.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
icons="$root/src-tauri/icons"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

python3 "$root/scripts/icon-svg.py" "$icons/source"

(cd "$tmp" && npm init -y >/dev/null && npm install --silent @resvg/resvg-js >/dev/null)
cat >"$tmp/render.mjs" <<'EOF'
import { Resvg } from '@resvg/resvg-js';
import fs from 'node:fs';
const [src, out, size] = process.argv.slice(2);
const svg = new Resvg(fs.readFileSync(src), {
  fitTo: { mode: 'width', value: Number(size) },
  font: { loadSystemFonts: true },
});
fs.writeFileSync(out, svg.render().asPng());
EOF
render() { node "$tmp/render.mjs" "$icons/source/mathia-$1.svg" "$2" "$3"; }

# Bundle icons from the light master.
render light "$tmp/light-1024.png" 1024
npx --yes @tauri-apps/cli icon "$tmp/light-1024.png" -o "$tmp/bundle" >/dev/null
for f in 32x32.png 128x128.png 128x128@2x.png icon.png icon.ico icon.icns; do
  cp "$tmp/bundle/$f" "$icons/$f"
done

# Runtime Dock icons.
render light "$icons/dock-light.png" 1024
render dark "$icons/dock-dark.png" 1024

# Dark .icns.
if command -v iconutil >/dev/null; then
  set_dir="$tmp/dark.iconset"
  mkdir -p "$set_dir"
  for s in 16 32 128 256 512; do
    render dark "$set_dir/icon_${s}x${s}.png" "$s"
    render dark "$set_dir/icon_${s}x${s}@2x.png" "$((s * 2))"
  done
  iconutil -c icns "$set_dir" -o "$icons/icon-dark.icns"
fi

# In-app logo (also the favicon).
mkdir -p "$root/ui/assets"
render light "$root/ui/assets/mathia-light.png" 128
render dark "$root/ui/assets/mathia-dark.png" 128

echo "Icons written to $icons and $root/ui/assets"
