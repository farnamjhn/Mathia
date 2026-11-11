#!/usr/bin/env python3
"""Writes the Mathia app icon as two SVGs, light and dark, on Apple's macOS
icon grid: a 1024×1024 transparent canvas with the rounded-square body at
100…924 (824 pt, corner radius 185) and a soft drop shadow beneath it.

The picture is black on black: a headless black suit with a black tie, and
behind it one slanted slab M as wide as the icon, its notch where the head would be.
It is lit from above: every overlapping piece (collar on shirt, tie on shirt,
lapel on jacket) casts a small blurred shadow, drawn as its own shape so that
no large area passes through a filter. The two variants differ only in how
bright the M is: paper white for the light one, dimmed for the dark one.

Usage: scripts/icon-svg.py OUT_DIR   (writes mathia-light.svg, mathia-dark.svg)
Then scripts/make-icons.sh renders and bundles them.
"""

import sys
from pathlib import Path

PALETTES = {
    "dark": {
        "letter": ("#a9adb4", "#5f636a"),
        "rim": ("#c3c8d2", "0.5"),
        "edge": ("#4a4d53", "#000000"),
        "drop": "0.5",
    },
    "light": {
        "letter": ("#ffffff", "#d8d9d5"),
        "rim": ("#dfe3ea", "0.62"),
        "edge": ("#5d6066", "#000000"),
        "drop": "0.42",
    },
}

GROUND = ("#232427", "#0c0c0d", "#000000")
SUIT = ("#2c2d31", "#111113", "#040404")
LAPEL = ("#3b3d43", "#0d0d0f")
TIE = ("#050506", "#4a4d55", "#030303")

BODY = 'x="100" y="100" width="824" height="824" rx="185" ry="185"'

JACKET = (
    "M 150,940 C 152,800 168,690 196,632 C 214,596 300,566 418,508 "
    "C 450,499 574,499 606,508 C 724,566 810,596 828,632 C 856,690 872,800 874,940 Z"
)
SHIRT = "M 424,500 C 428,602 470,712 512,792 C 554,712 596,602 600,500 Z"
LAPEL_PATH = (
    "M 420,505 C 424,610 466,716 512,796 L 512,816 C 438,752 370,676 336,612 "
    "L 388,592 L 360,548 C 378,530 400,515 420,505 Z"
)
WING = "M 431,470 C 420,508 428,562 462,606 L 501,528 C 470,520 444,500 431,470 Z"
BLADE = "M 497,568 L 527,568 L 558,762 L 512,816 L 466,762 Z"
KNOT = "M 487,521 C 500,514 524,514 537,521 L 528,573 C 518,579 506,579 496,573 Z"
# A slab M: stems as wide as the diagonals, the notch just above the collar.
LETTER = (
    "M 190,960 L 190,190 L 352,190 L 512,398 L 672,190 L 834,190 L 834,960 "
    "L 684,960 L 684,372 L 512,596 L 340,372 L 340,960 Z"
)

# The M leans 12° to the right, pivoting at the height of the collar so it
# stays centred on the figure, and is drawn a little smaller so its leading
# corner clears the rounded corner of the icon.
SLANT = "translate(93.5 0) skewX(-12) translate(512 440) scale(0.92) translate(-512 -440)"

MIRROR = 'transform="translate(1024 0) scale(-1 1)"'


def svg(p: dict) -> str:
    b0, b1, b2 = GROUND
    s0, s1, s2 = SUIT
    l0, l1 = LAPEL
    m0, m1 = p["letter"]
    rim, rim_o = p["rim"]
    e0, e1 = p["edge"]
    t0, t1, t2 = TIE
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">
  <defs>
    <clipPath id="body"><rect {BODY}/></clipPath>
    <clipPath id="jacket"><path d="{JACKET}"/></clipPath>
    <clipPath id="shirt"><path d="{SHIRT}"/></clipPath>
    <clipPath id="blade"><path d="{BLADE}"/></clipPath>

    <radialGradient id="board" gradientUnits="userSpaceOnUse" cx="512" cy="330" r="640">
      <stop offset="0" stop-color="{b0}"/><stop offset="0.55" stop-color="{b1}"/><stop offset="1" stop-color="{b2}"/>
    </radialGradient>
    <linearGradient id="suit" gradientUnits="userSpaceOnUse" x1="0" y1="500" x2="0" y2="924">
      <stop offset="0" stop-color="{s0}"/><stop offset="0.42" stop-color="{s1}"/><stop offset="1" stop-color="{s2}"/>
    </linearGradient>
    <!-- satin: a band of sheen across each lapel -->
    <linearGradient id="lapel" gradientUnits="userSpaceOnUse" x1="350" y1="540" x2="500" y2="660">
      <stop offset="0" stop-color="{l1}"/><stop offset="0.38" stop-color="{l0}"/><stop offset="0.55" stop-color="#5b5e66"/><stop offset="0.72" stop-color="{l0}"/><stop offset="1" stop-color="{l1}"/>
    </linearGradient>
    <linearGradient id="letter" gradientUnits="userSpaceOnUse" x1="0" y1="190" x2="0" y2="640">
      <stop offset="0" stop-color="{m0}"/><stop offset="1" stop-color="{m1}"/>
    </linearGradient>
    <linearGradient id="shirt-fill" gradientUnits="userSpaceOnUse" x1="0" y1="500" x2="0" y2="792">
      <stop offset="0" stop-color="#e3e6ea"/><stop offset="0.3" stop-color="#ffffff"/><stop offset="1" stop-color="#c3c8d0"/>
    </linearGradient>
    <linearGradient id="wing" gradientUnits="userSpaceOnUse" x1="424" y1="470" x2="500" y2="600">
      <stop offset="0" stop-color="#dfe2e7"/><stop offset="0.45" stop-color="#ffffff"/><stop offset="1" stop-color="#cdd1d8"/>
    </linearGradient>
    <linearGradient id="stand" gradientUnits="userSpaceOnUse" x1="0" y1="440" x2="0" y2="496">
      <stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#c6cad1"/>
    </linearGradient>
    <linearGradient id="hollow" gradientUnits="userSpaceOnUse" x1="0" y1="452" x2="0" y2="496">
      <stop offset="0" stop-color="#b9bec6"/><stop offset="0.5" stop-color="#555a62"/><stop offset="1" stop-color="#16181b"/>
    </linearGradient>
    <linearGradient id="tie" gradientUnits="userSpaceOnUse" x1="466" y1="0" x2="558" y2="0">
      <stop offset="0" stop-color="{t0}"/><stop offset="0.45" stop-color="{t1}"/><stop offset="1" stop-color="{t2}"/>
    </linearGradient>
    <linearGradient id="knot" gradientUnits="userSpaceOnUse" x1="487" y1="514" x2="537" y2="579">
      <stop offset="0" stop-color="{t1}"/><stop offset="1" stop-color="{t0}"/>
    </linearGradient>
    <linearGradient id="gloss" gradientUnits="userSpaceOnUse" x1="0" y1="100" x2="0" y2="520">
      <stop offset="0" stop-color="#ffffff" stop-opacity="0.14"/><stop offset="1" stop-color="#ffffff" stop-opacity="0"/>
    </linearGradient>
    <linearGradient id="edge" gradientUnits="userSpaceOnUse" x1="0" y1="100" x2="0" y2="924">
      <stop offset="0" stop-color="{e0}"/><stop offset="1" stop-color="{e1}"/>
    </linearGradient>

    <filter id="blur-4" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="4"/></filter>
    <filter id="blur-9" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="9"/></filter>
    <filter id="blur-22" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="22"/></filter>
    <!-- Wool: fine light grain over the jacket. -->
    <filter id="wool" x="0" y="0" width="100%" height="100%">
      <feTurbulence type="fractalNoise" baseFrequency="1.1" numOctaves="2" seed="3"/>
      <feColorMatrix type="matrix" values="0 0 0 0 1  0 0 0 0 1  0 0 0 0 1  0 0 0 0.13 -0.03"/>
    </filter>
  </defs>

  <!-- The macOS drop shadow -->
  <rect x="112" y="124" width="800" height="806" rx="180" fill="#000000" opacity="{p["drop"]}" filter="url(#blur-22)"/>

  <g clip-path="url(#body)">
    <rect {BODY} fill="url(#board)"/>

    <!-- The M -->
    <g transform="{SLANT}">
      <path d="{LETTER}" fill="url(#letter)"/>
      <path d="M 190,192 L 352,192 M 672,192 L 834,192" fill="none" stroke="#ffffff" stroke-width="3" opacity="0.5"/>
    </g>

    <!-- Shadow the figure throws on the M -->
    <path d="{JACKET}" fill="#000000" opacity="0.62" filter="url(#blur-22)" transform="translate(0 16)"/>

    <!-- Jacket -->
    <path d="{JACKET}" fill="url(#suit)"/>
    <rect x="150" y="490" width="724" height="440" clip-path="url(#jacket)" filter="url(#wool)"/>
    <g clip-path="url(#jacket)" fill="none" stroke-linecap="round">
      <!-- light catching the shoulders -->
      <g stroke="{rim}" opacity="{rim_o}" filter="url(#blur-4)">
        <path d="M 204,628 C 226,598 304,570 416,514" stroke-width="9"/>
        <path d="M 204,628 C 226,598 304,570 416,514" stroke-width="9" {MIRROR}/>
      </g>
      <!-- where the sleeves meet the body -->
      <g stroke="#000000" opacity="0.6" filter="url(#blur-4)">
        <path d="M 262,712 C 274,790 270,870 260,940" stroke-width="12"/>
        <path d="M 262,712 C 274,790 270,870 260,940" stroke-width="12" {MIRROR}/>
      </g>
      <g stroke="{rim}" opacity="0.14">
        <path d="M 272,716 C 284,792 280,870 270,940" stroke-width="2.5"/>
        <path d="M 272,716 C 284,792 280,870 270,940" stroke-width="2.5" {MIRROR}/>
      </g>
    </g>

    <!-- Shirt, with the shade of the lapels and collar on it -->
    <path d="{SHIRT}" fill="url(#shirt-fill)"/>
    <g clip-path="url(#shirt)">
      <path d="M 404,500 C 410,610 452,716 512,800" fill="none" stroke="#131417" stroke-width="30" opacity="0.3" filter="url(#blur-9)"/>
      <path d="M 404,500 C 410,610 452,716 512,800" fill="none" stroke="#131417" stroke-width="30" opacity="0.3" filter="url(#blur-9)" {MIRROR}/>
      <path d="{BLADE}" fill="#0b0b0d" opacity="0.45" filter="url(#blur-9)" transform="translate(9 6)"/>
    </g>

    <!-- Tie blade -->
    <g clip-path="url(#shirt)">
      <path d="{BLADE}" fill="url(#tie)"/>
      <g clip-path="url(#blade)" stroke="#ffffff" stroke-width="13" opacity="0.07">
        <path d="M 440,640 L 590,560"/><path d="M 440,700 L 590,620"/><path d="M 440,760 L 590,680"/><path d="M 440,820 L 590,740"/>
      </g>
      <!-- the dimple under the knot -->
      <path d="M 512,574 C 507,600 507,636 512,668 C 517,636 517,600 512,574 Z" fill="#000000" opacity="0.6" filter="url(#blur-4)"/>
      <path d="M 494,566 L 530,566 L 532,590 L 492,590 Z" fill="#000000" opacity="0.6" filter="url(#blur-4)"/>
    </g>

    <!-- Lapels: the wearer's left lies over the right -->
    <g clip-path="url(#jacket)">
      <path d="{LAPEL_PATH}" fill="#000000" opacity="0.62" filter="url(#blur-9)" transform="translate(-8 12)"/>
      <path d="{LAPEL_PATH}" fill="#000000" opacity="0.62" filter="url(#blur-9)" transform="translate(1032 12) scale(-1 1)"/>
    </g>
    <path d="{LAPEL_PATH}" fill="url(#lapel)"/>
    <path d="M 336,612 L 388,592 L 360,548 C 378,530 400,515 420,505" fill="none" stroke="{rim}" stroke-width="2.5" stroke-linejoin="round" opacity="0.4"/>
    <g {MIRROR}>
      <path d="{LAPEL_PATH}" fill="url(#lapel)"/>
      <path d="M 336,612 L 388,592 L 360,548 C 378,530 400,515 420,505" fill="none" stroke="{rim}" stroke-width="2.5" stroke-linejoin="round" opacity="0.4"/>
      <path d="M 420,505 C 424,610 466,716 512,796" fill="none" stroke="#000000" stroke-width="3" opacity="0.5"/>
    </g>
    <path d="M 420,505 C 424,610 466,716 512,796" fill="none" stroke="#000000" stroke-width="3" opacity="0.5"/>
    <g fill="none" stroke="{rim}" stroke-width="2" stroke-dasharray="7 7" stroke-linecap="round" opacity="0.3">
      <path d="M 352,620 C 388,682 446,748 504,800"/>
      <path d="M 352,620 C 388,682 446,748 504,800" {MIRROR}/>
    </g>

    <!-- The one fastened button -->
    <ellipse cx="514" cy="874" rx="17" ry="16" fill="#000000" opacity="0.6" filter="url(#blur-4)"/>
    <circle cx="512" cy="868" r="14" fill="{s1}" stroke="{l0}" stroke-width="2.5"/>
    <circle cx="507" cy="868" r="1.8" fill="#000000" opacity="0.7"/><circle cx="517" cy="868" r="1.8" fill="#000000" opacity="0.7"/>
    <path d="M 512,818 C 512,840 512,880 512,940" fill="none" stroke="#000000" stroke-width="3" opacity="0.45"/>

    <!-- Pocket square -->
    <path d="M 652,716 L 660,690 L 678,704 L 692,684 L 706,702 L 722,690 L 726,706 Z" fill="#f4f5f7"/>
    <path d="M 678,704 L 692,684 L 706,702 Z" fill="#d5d9df"/>
    <path d="M 646,718 L 730,705" fill="none" stroke="#000000" stroke-width="5" stroke-linecap="round" opacity="0.7"/>
    <path d="M 647,722 L 731,709" fill="none" stroke="{rim}" stroke-width="1.5" stroke-linecap="round" opacity="0.25"/>

    <!-- Collar: the stand, the empty inside, then the two wings and the knot -->
    <ellipse cx="512" cy="468" rx="82" ry="28" fill="url(#stand)"/>
    <ellipse cx="512" cy="475" rx="71" ry="20" fill="url(#hollow)"/>
    <path d="M 430,468 A 82,28 0 0 1 594,468" fill="none" stroke="#ffffff" stroke-width="3" stroke-linecap="round"/>
    <g clip-path="url(#shirt)">
      <path d="{WING}" fill="#131417" opacity="0.4" filter="url(#blur-9)" transform="translate(4 12)"/>
      <path d="{WING}" fill="#131417" opacity="0.4" filter="url(#blur-9)" transform="translate(1020 12) scale(-1 1)"/>
    </g>
    <path d="{KNOT}" fill="url(#knot)"/>
    <path d="M 493,523 C 503,518 521,518 531,523" fill="none" stroke="#ffffff" stroke-width="2" stroke-linecap="round" opacity="0.3"/>
    <path d="{WING}" fill="url(#wing)" stroke="#aab0b9" stroke-width="1.5" stroke-linejoin="round"/>
    <path d="{WING}" fill="url(#wing)" stroke="#aab0b9" stroke-width="1.5" stroke-linejoin="round" {MIRROR}/>

    <!-- Light from above on the body, and a darkened rim -->
    <rect {BODY} fill="url(#gloss)" opacity="0.5"/>
    <rect {BODY} fill="none" stroke="#000000" stroke-width="30" opacity="0.35" filter="url(#blur-9)"/>
  </g>

  <rect x="101.5" y="101.5" width="821" height="821" rx="183.5" fill="none" stroke="url(#edge)" stroke-width="3"/>
</svg>
"""


def main() -> None:
    out = Path(sys.argv[1] if len(sys.argv) > 1 else ".")
    out.mkdir(parents=True, exist_ok=True)
    for name, palette in PALETTES.items():
        (out / f"mathia-{name}.svg").write_text(svg(palette))


if __name__ == "__main__":
    main()
