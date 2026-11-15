"""Generates BIA's logo and app icon from one source of truth.

Writes:
  assets/logo.svg                 hummingbird + "bia", adapts to light/dark
  assets/icons/logo-template.svg  same, with {INK}/{BIRD} for the app to fill
  assets/icons/bia.svg            app icon: the hummingbird on the night tile
  assets/icons/bia.png            512 px, for the macOS bundle and Linux
  assets/icons/bia.ico            16-256 px, embedded in bia.exe
  assets/icons/bia-64.rgba        raw 64x64 pixels for the window icon

The wordmark is "bia" in Fira Sans Light (tools/fonts/), converted to
outlines so the logo needs no font. Rasters need ImageMagick 7 (`magick`).

    python3 -m venv .venv && .venv/bin/pip install fonttools
    .venv/bin/python tools/make_logo.py
"""

import shutil
import struct
import subprocess
import tempfile
from pathlib import Path

from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "assets"
FONT = ROOT / "tools" / "fonts" / "FiraSans-Light.ttf"

# The hummingbird, on a 64x64 grid.
BEAK = "M21 30 L8 33"
BODY = "M20 27 C 22 20, 30 20, 32 26 C 36 34, 44 42, 52 52 C 40 48, 28 42, 22 35 C 20 33, 19 30, 20 27 Z"
EYE = "M26.1 26.5 a1.6 1.6 0 1 0 -3.2 0 a1.6 1.6 0 1 0 3.2 0 Z"  # cut out of the body
WING = "M29 30 C 34 20, 44 13, 56 11 C 50 20, 42 30, 34 36 Z"

NIGHT = "#121116"
INK_DARK = "#ece6da"
INK_LIGHT = "#23211e"
GREEN = "#3fb59b"
GREEN_ON_LIGHT = "#23927a"


def wordmark(text="bia", x0=64.0, baseline=50.0, b_height=38.0):
    """`text` as one SVG path, scaled so the 'b' is `b_height` tall."""
    font = TTFont(FONT)
    glyphs = font.getGlyphSet()
    cmap = font.getBestCmap()
    bounds = BoundsPen(glyphs)
    glyphs[cmap[ord("b")]].draw(bounds)
    scale = b_height / bounds.bounds[3]
    pen = SVGPathPen(glyphs)
    x = 0.0
    for ch in text:
        glyph = glyphs[cmap[ord(ch)]]
        glyph.draw(TransformPen(pen, (scale, 0, 0, -scale, x0 + x * scale, baseline)))
        x += glyph.width
    return pen.getCommands(), x0 + x * scale


def bird(ink, green, extra=""):
    return (
        f'  <path d="{BEAK}" fill="none" stroke="{ink}" stroke-width="2.2" stroke-linecap="round"{extra}/>\n'
        f'  <path d="{BODY} {EYE}" fill="{green}" fill-rule="evenodd"/>\n'
        f'  <path d="{WING}" fill="{ink}"/>\n'
    )


def write_svgs():
    word, right = wordmark()
    width = right  # viewBox starts at x = 4 and adds 4 of margin on the right
    box = f"4 6 {width:.1f} 50"

    (ASSETS / "logo.svg").write_text(
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{box}" width="{width * 3:.0f}" height="150" role="img" aria-label="BIA">\n'
        "  <style>\n"
        f"    .ink {{ fill: {INK_LIGHT}; stroke: {INK_LIGHT}; }}\n"
        f"    .bird {{ fill: {GREEN_ON_LIGHT}; }}\n"
        "    @media (prefers-color-scheme: dark) {\n"
        f"      .ink {{ fill: {INK_DARK}; stroke: {INK_DARK}; }}\n"
        f"      .bird {{ fill: {GREEN}; }}\n"
        "    }\n"
        "  </style>\n"
        f'  <path class="ink" d="{BEAK}" fill="none" stroke-width="2.2" stroke-linecap="round"/>\n'
        f'  <path class="bird" d="{BODY} {EYE}" fill-rule="evenodd"/>\n'
        f'  <path class="ink" d="{WING}" stroke="none"/>\n'
        f'  <path class="ink" d="{word}" stroke="none"/>\n'
        "</svg>\n"
    )
    (ASSETS / "icons" / "logo-template.svg").write_text(
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{box}">\n'
        + bird("{INK}", "{BIRD}")
        + f'  <path d="{word}" fill="{{INK}}"/>\n'
        "</svg>\n"
    )
    (ASSETS / "icons" / "bia.svg").write_text(
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">\n'
        f'  <rect width="64" height="64" rx="14" fill="{NIGHT}"/>\n'
        + bird(INK_DARK, GREEN)
        + "</svg>\n"
    )


def render(svg: Path, size: int, out: str):
    subprocess.run(
        ["magick", "-background", "none", "-density", str(size * 18), str(svg),
         "-resize", f"{size}x{size}", "-depth", "8", out],
        check=True,
    )


def write_rasters():
    if not shutil.which("magick"):
        raise SystemExit("ImageMagick 7 (`magick`) is needed for the PNG, ICO and RGBA icons.")
    icon = ASSETS / "icons" / "bia.svg"
    render(icon, 512, f"png32:{ASSETS / 'icons' / 'bia.png'}")
    render(icon, 64, f"rgba:{ASSETS / 'icons' / 'bia-64.rgba'}")

    # A Vista-style ICO: one PNG per size, so Windows never has to scale.
    sizes = [16, 24, 32, 48, 64, 256]
    with tempfile.TemporaryDirectory() as tmp:
        pngs = []
        for size in sizes:
            out = Path(tmp) / f"{size}.png"
            render(icon, size, f"png32:{out}")
            pngs.append(out.read_bytes())
    header = struct.pack("<HHH", 0, 1, len(sizes))
    offset = 6 + 16 * len(sizes)
    entries, data = b"", b""
    for size, png in zip(sizes, pngs):
        dim = 0 if size == 256 else size  # 0 means 256 in the ICO format
        entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(png), offset + len(data))
        data += png
    (ASSETS / "icons" / "bia.ico").write_bytes(header + entries + data)


if __name__ == "__main__":
    write_svgs()
    write_rasters()
    print("logo and icons written to", ASSETS)
