#!/usr/bin/env python3
"""Builds the Tauri updater manifest (latest.json) from signed release assets.

The release workflow downloads every platform's artifacts into one directory,
then runs this script. Each ``*.sig`` file is paired with the artifact next to
it, mapped to a Tauri platform key, and written to ``latest.json`` with the
GitHub download URL for the tag. The app reads that manifest from
``releases/latest/download/latest.json``.
"""

import argparse
import json
import sys
from datetime import datetime, timezone
from pathlib import Path


def is_nsis(name: str) -> bool:
    lower = name.lower()
    return lower.endswith("setup.exe.zip") or lower.endswith(".nsis.zip")


def platform_for(name: str):
    lower = name.lower()
    if lower.endswith(".app.tar.gz"):
        if "aarch64" in lower or "arm64" in lower:
            return "darwin-aarch64"
        if "x64" in lower or "x86_64" in lower:
            return "darwin-x86_64"
        return None
    if lower.endswith(".appimage.tar.gz"):
        return "linux-x86_64"
    if is_nsis(lower) or lower.endswith(".msi.zip"):
        return "windows-x86_64"
    return None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dir", default="dist", help="directory with the downloaded artifacts")
    parser.add_argument("--repo", required=True, help="owner/name of the GitHub repository")
    parser.add_argument("--tag", required=True, help="release tag, e.g. v0.2.0")
    parser.add_argument("--out", default=None, help="output path (default: <dir>/latest.json)")
    parser.add_argument("--notes", default="", help="optional release notes")
    args = parser.parse_args()

    root = Path(args.dir)
    out = Path(args.out) if args.out else root / "latest.json"
    version = args.tag[1:] if args.tag.startswith("v") else args.tag
    base = f"https://github.com/{args.repo}/releases/download/{args.tag}"

    candidates = {}
    for sig in sorted(root.rglob("*.sig")):
        artifact = sig.with_suffix("")
        if not artifact.is_file():
            continue
        key = platform_for(artifact.name)
        if key is None:
            continue
        signature = sig.read_text(encoding="utf-8").strip()
        if not signature:
            continue
        previous = candidates.get(key)
        # Prefer the NSIS installer over the MSI when both were built.
        if previous and is_nsis(previous[0]) and not is_nsis(artifact.name):
            continue
        candidates[key] = (artifact.name, signature)

    if not candidates:
        print("error: no signed updater artifacts found", file=sys.stderr)
        return 1

    platforms = {
        key: {"signature": signature, "url": f"{base}/{name}"}
        for key, (name, signature) in candidates.items()
    }

    manifest = {
        "version": version,
        "pub_date": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": platforms,
    }
    if args.notes:
        manifest["notes"] = args.notes

    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {out} for {version}: {', '.join(sorted(platforms))}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
