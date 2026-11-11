#!/usr/bin/env python3
"""Writes the Mathia app icon as two SVGs, light and dark, on Apple's macOS
icon grid: a 1024×1024 transparent canvas with the rounded-square body at
100…924 (824 pt) and a soft drop shadow beneath it.

The artwork (chalkboard with formulas, suit, shirt and tie) is drawn in a
680-unit box and scaled onto that grid. Only the palette differs between the
two variants: the dark one is a slate chalkboard, the light one a pale
whiteboard with graphite formulas.

Usage: scripts/icon-svg.py OUT_DIR   (writes mathia-light.svg, mathia-dark.svg)
Then scripts/make-icons.sh renders and bundles them.
"""

import sys
from pathlib import Path

PALETTES = {
    "dark": {
        "board": ("#2c3034", "#1b1e21", "#0f1113"),
        "smudge": ("#ffffff", "0.03", "0.025"),
        "chalk": "#a3a8af",
        "chalk_opacity": "0.78",
        "suit": ("#1e2024", "#0c0d0f", "#030304"),
        "suit_stroke": "#2a2d32",
        "shoulder": "#585d66",
        "suit_shadow": "0.65",
        "vignette": ("#000000", "0.55"),
        "bezel": ("#3c4046", "#1c1e22"),
        "drop": "0.45",
    },
    "light": {
        "board": ("#fbfaf6", "#eeece6", "#dddad2"),
        "smudge": ("#8a8f98", "0.05", "0.04"),
        "chalk": "#4d535c",
        "chalk_opacity": "0.82",
        "suit": ("#3a3f48", "#1d2026", "#0d0e11"),
        "suit_stroke": "#454a53",
        "shoulder": "#7d838d",
        "suit_shadow": "0.35",
        "vignette": ("#000000", "0.08"),
        "bezel": ("#d4d5d9", "#ffffff"),
        "drop": "0.28",
    },
}

# Apple grid: body 824 at offset 100. Artwork box is 680 at offset 60.
SCALE = 824 / 680
OFFSET = 100 - 60 * SCALE
RADIUS = 153  # 153 × 1.2118 ≈ 185, Apple's corner radius for an 824 body


def svg(p: dict) -> str:
    b0, b1, b2 = p["board"]
    sm, sm1, sm2 = p["smudge"]
    s0, s1, s2 = p["suit"]
    v, vo = p["vignette"]
    z0, z1 = p["bezel"]
    c = p["chalk"]
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">
  <defs>
    <clipPath id="icon-clip"><rect x="60" y="60" width="680" height="680" rx="{RADIUS}" ry="{RADIUS}"/></clipPath>
    <radialGradient id="board" cx="50%" cy="35%" r="65%" fx="50%" fy="35%">
      <stop offset="0%" stop-color="{b0}"/><stop offset="60%" stop-color="{b1}"/><stop offset="100%" stop-color="{b2}"/>
    </radialGradient>
    <linearGradient id="suit-grad" x1="0%" y1="0%" x2="0%" y2="100%">
      <stop offset="0%" stop-color="{s0}"/><stop offset="35%" stop-color="{s1}"/><stop offset="100%" stop-color="{s2}"/>
    </linearGradient>
    <linearGradient id="shirt-grad" x1="0%" y1="0%" x2="0%" y2="100%">
      <stop offset="0%" stop-color="#ffffff"/><stop offset="65%" stop-color="#dcdde0"/><stop offset="100%" stop-color="#9ea2a8"/>
    </linearGradient>
    <linearGradient id="collar-left" x1="0%" y1="0%" x2="100%" y2="80%">
      <stop offset="0%" stop-color="#d4d7dc"/><stop offset="40%" stop-color="#ffffff"/><stop offset="100%" stop-color="#b8bcc2"/>
    </linearGradient>
    <linearGradient id="collar-right" x1="100%" y1="0%" x2="0%" y2="80%">
      <stop offset="0%" stop-color="#d4d7dc"/><stop offset="40%" stop-color="#ffffff"/><stop offset="100%" stop-color="#b8bcc2"/>
    </linearGradient>
    <linearGradient id="neck-hole" x1="0%" y1="0%" x2="0%" y2="100%">
      <stop offset="0%" stop-color="#3d4047"/><stop offset="50%" stop-color="#1e2024"/><stop offset="100%" stop-color="#0b0c0e"/>
    </linearGradient>
    <linearGradient id="tie-grad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#32353b"/><stop offset="45%" stop-color="#16181b"/><stop offset="100%" stop-color="#08090b"/>
    </linearGradient>
    <filter id="drop-shadow" x="-15%" y="-15%" width="130%" height="130%">
      <feDropShadow dx="0" dy="8" stdDeviation="8" flood-color="#000000" flood-opacity="{p["suit_shadow"]}"/>
    </filter>
    <filter id="chalk-effect" x="-10%" y="-10%" width="120%" height="120%">
      <feTurbulence type="fractalNoise" baseFrequency="0.8" numOctaves="3" result="noise"/>
      <feDisplacementMap in="SourceGraphic" in2="noise" scale="1.6" xChannelSelector="R" yChannelSelector="G"/>
    </filter>
    <filter id="icon-shadow" x="-10%" y="-10%" width="120%" height="125%">
      <feDropShadow dx="0" dy="10" stdDeviation="12" flood-color="#000000" flood-opacity="{p["drop"]}"/>
    </filter>
  </defs>

  <g transform="translate({OFFSET:.3f} {OFFSET:.3f}) scale({SCALE:.6f})">
    <!-- Body with the macOS drop shadow -->
    <rect x="60" y="60" width="680" height="680" rx="{RADIUS}" ry="{RADIUS}" fill="url(#board)" filter="url(#icon-shadow)"/>

    <g clip-path="url(#icon-clip)">
      <ellipse cx="400" cy="240" rx="260" ry="120" fill="{sm}" opacity="{sm1}" filter="url(#chalk-effect)"/>
      <ellipse cx="260" cy="180" rx="120" ry="70" fill="{sm}" opacity="{sm2}"/>
      <ellipse cx="560" cy="240" rx="140" ry="90" fill="{sm}" opacity="{sm2}"/>

      <!-- Formulas: e^(iπ) = -1, ∞, π, Av = λv, Gaussian integral, ℵ₀ -->
      <g fill="{c}" stroke="{c}" font-family="'Chalkboard SE', 'Bradley Hand', 'Segoe Print', 'Comic Sans MS', cursive, serif" font-style="italic" opacity="{p["chalk_opacity"]}" filter="url(#chalk-effect)">
        <g transform="translate(175, 195) rotate(-10)">
          <text x="0" y="0" font-size="52" stroke-width="0.8">e</text>
          <text x="28" y="-26" font-size="34" stroke-width="0.6">iπ</text>
          <text x="68" y="-4" font-size="44" font-style="normal" stroke-width="0.6">=</text>
          <text x="102" y="-6" font-size="44" font-style="normal" stroke-width="0.6">-1</text>
        </g>
        <g transform="translate(212, 268) rotate(-8)">
          <path d="M 18,12 C 6,0 0,24 14,26 C 24,27 32,4 44,12 C 54,19 46,34 35,26 C 26,20 24,18 18,12 Z" fill="none" stroke-width="3.5" stroke-linecap="round"/>
        </g>
        <g transform="translate(278, 240) rotate(-12)">
          <path d="M 4,2 Q 22,-2 40,0 M 14,1 L 10,32 M 30,1 C 29,16 28,28 34,31 C 38,32 41,28 42,25" fill="none" stroke-width="3.6" stroke-linecap="round"/>
        </g>
        <g transform="translate(368, 224) rotate(-14)">
          <text x="0" y="0" font-size="50" stroke-width="0.7">Av</text>
          <text x="64" y="-4" font-size="42" font-style="normal" stroke-width="0.6">=</text>
          <text x="96" y="0" font-size="50" stroke-width="0.7">λv</text>
        </g>
        <g transform="translate(515, 165) rotate(3)">
          <path d="M 32,2 C 23,-2 19,8 18,22 L 15,62 C 13,78 8,86 0,82" fill="none" stroke-width="3.5" stroke-linecap="round"/>
          <path d="M 39,4 C 33,-2 30,10 37,11 C 42,12 46,0 52,4 C 57,8 53,15 47,11 C 43,8 42,7 39,4 Z" fill="none" stroke-width="2"/>
          <line x1="19" y1="81" x2="29" y2="81" stroke-width="2.2" stroke-linecap="round"/>
          <path d="M 38,77 C 32,71 29,83 36,84 C 41,85 45,73 51,77 C 56,81 52,88 46,84 C 42,81 41,80 38,77 Z" fill="none" stroke-width="2"/>
          <text x="55" y="45" font-size="46" stroke-width="0.7">e</text>
          <text x="80" y="24" font-size="28" font-style="normal" stroke-width="0.5">-</text>
          <text x="93" y="24" font-size="30" stroke-width="0.5">x</text>
          <text x="110" y="10" font-size="22" font-style="normal" stroke-width="0.5">2</text>
          <text x="116" y="48" font-size="38" stroke-width="0.6">dx</text>
        </g>
        <g transform="translate(535, 255) rotate(-5)">
          <path d="M 4,4 L 26,38 M 4,36 C 4,24 10,18 14,19 M 26,4 C 26,16 20,22 16,21" fill="none" stroke-width="3.8" stroke-linecap="round"/>
          <text x="30" y="48" font-size="28" font-style="normal" stroke-width="0.6">0</text>
        </g>
      </g>

      <!-- Suit, shirt and tie -->
      <g filter="url(#drop-shadow)">
        <path d="M 105,750 C 105,600 120,440 142,382 C 155,348 250,318 308,292 L 492,292 C 550,318 645,348 658,382 C 680,440 695,600 695,750 Z" fill="url(#suit-grad)" stroke="{p["suit_stroke"]}" stroke-width="2"/>
        <path d="M 142,382 C 155,348 250,318 306,293" fill="none" stroke="{p["shoulder"]}" stroke-width="3.5" stroke-linecap="round" opacity="0.7"/>
        <path d="M 658,382 C 645,348 550,318 494,293" fill="none" stroke="{p["shoulder"]}" stroke-width="3.5" stroke-linecap="round" opacity="0.7"/>
        <path d="M 295,295 C 285,395 340,485 400,542 C 460,485 515,395 505,295 Z" fill="#070809"/>
        <path d="M 304,310 C 304,400 348,478 400,532 C 452,478 496,400 496,310 Z" fill="url(#shirt-grad)"/>
        <path d="M 335,440 Q 355,410 365,370" fill="none" stroke="#b0b5bd" stroke-width="3" stroke-linecap="round" opacity="0.6"/>
        <path d="M 465,440 Q 445,410 435,370" fill="none" stroke="#b0b5bd" stroke-width="3" stroke-linecap="round" opacity="0.6"/>
        <path d="M 368,340 L 352,415 L 400,536 L 448,415 L 432,340 Z" fill="#000000" opacity="0.35"/>
        <path d="M 384,375 L 366,488 L 400,534 L 434,488 L 416,375 Z" fill="url(#tie-grad)" stroke="#2e3136" stroke-width="1.5"/>
        <path d="M 378,425 L 415,400 L 420,418 L 373,448 Z" fill="#2c3036" opacity="0.65"/>
        <path d="M 370,468 L 424,435 L 428,455 L 368,486 Z" fill="#2c3036" opacity="0.55"/>
        <path d="M 382,504 L 430,474 L 420,498 L 392,516 Z" fill="#2c3036" opacity="0.45"/>
        <path d="M 365,335 L 382,382 L 418,382 L 435,335 Z" fill="#060708"/>
        <path d="M 372,326 C 386,322 414,322 428,326 L 418,378 C 406,383 394,383 382,378 Z" fill="url(#tie-grad)" stroke="#383c42" stroke-width="1.5"/>
        <path d="M 378,332 C 390,328 405,330 412,338 L 406,370 C 396,370 388,366 384,354 Z" fill="#3a3e45" opacity="0.5"/>
        <path d="M 312,288 C 312,238 488,238 488,288 C 488,318 450,336 400,336 C 350,336 312,318 312,288 Z" fill="url(#neck-hole)"/>
        <path d="M 312,288 C 312,238 488,238 488,288" fill="none" stroke="#ffffff" stroke-width="4.5" stroke-linecap="round"/>
        <path d="M 304,310 C 304,365 348,408 354,412 L 375,345 L 400,332 L 425,345 L 446,412 C 452,408 496,365 496,310 Z" fill="#000000" opacity="0.4"/>
        <path d="M 312,288 C 300,322 315,368 352,404 L 374,332 C 342,324 320,306 312,288 Z" fill="url(#collar-left)" stroke="#b0b4ba" stroke-width="1.5"/>
        <path d="M 310,304 C 306,334 322,368 350,396" fill="none" stroke="#7c8087" stroke-width="1.8"/>
        <path d="M 488,288 C 500,322 485,368 448,404 L 426,332 C 458,324 480,306 488,288 Z" fill="url(#collar-right)" stroke="#b0b4ba" stroke-width="1.5"/>
        <path d="M 490,304 C 494,334 478,368 450,396" fill="none" stroke="#7c8087" stroke-width="1.8"/>
        <path d="M 298,302 C 292,388 342,478 400,536" fill="none" stroke="#2c3036" stroke-width="2.5" opacity="0.8"/>
        <path d="M 502,302 C 508,388 458,478 400,536" fill="none" stroke="#2c3036" stroke-width="2.5" opacity="0.8"/>
        <path d="M 195,470 C 205,530 200,620 190,720" fill="none" stroke="#000000" stroke-width="6" opacity="0.6"/>
        <path d="M 605,470 C 595,530 600,620 610,720" fill="none" stroke="#000000" stroke-width="6" opacity="0.6"/>
      </g>

      <rect x="60" y="60" width="680" height="680" rx="{RADIUS}" ry="{RADIUS}" fill="none" stroke="{v}" stroke-width="18" opacity="{vo}"/>
    </g>

    <rect x="60" y="60" width="680" height="680" rx="{RADIUS}" ry="{RADIUS}" fill="none" stroke="{z0}" stroke-width="4"/>
    <rect x="62" y="62" width="676" height="676" rx="{RADIUS - 2}" ry="{RADIUS - 2}" fill="none" stroke="{z1}" stroke-width="1.5" opacity="0.6"/>
  </g>
</svg>
"""


def main() -> None:
    out = Path(sys.argv[1] if len(sys.argv) > 1 else ".")
    out.mkdir(parents=True, exist_ok=True)
    for name, palette in PALETTES.items():
        (out / f"mathia-{name}.svg").write_text(svg(palette))


if __name__ == "__main__":
    main()
