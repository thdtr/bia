"""Generates BIA's explorer icons into assets/icons/ui/.

Grid: 16x16, artwork inside the central 10x10, 1.2 px round strokes, one
color (BIA tints them with the theme). Languages get a tile with two letters
knocked out of it, set in Fira Sans Bold, so any language can have an icon
without new artwork.

    python3 -m venv .venv && .venv/bin/pip install fonttools
    .venv/bin/python tools/make_icons.py
"""

from pathlib import Path

from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "assets" / "icons" / "ui"
FONT = ROOT / "assets" / "fonts" / "FiraSans-Bold.ttf"

HEAD = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="16" height="16">'
STROKE = 'fill="none" stroke="#000" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"'


def stroked(*paths, extra=""):
    body = "".join(f'<path d="{d}"/>' for d in paths)
    return f"{HEAD}<g {STROKE}>{body}{extra}</g></svg>\n"


SYMBOLS = {
    "chevron-right": stroked("M6.5 4.5 10 8l-3.5 3.5"),
    "chevron-down": stroked("M4.5 6.5 8 10l3.5-3.5"),
    "folder": stroked(
        "M3.9 4H6.4l1.5 1.6h4.2a.9.9 0 0 1 .9.9v5.1a.9.9 0 0 1-.9.9H3.9a.9.9 0 0 1-.9-.9V4.9a.9.9 0 0 1 .9-.9Z"
    ),
    "folder-open": stroked(
        "M3 11.6V4.9a.9.9 0 0 1 .9-.9h2.5l1.5 1.6h3.7a.9.9 0 0 1 .9.9v.9",
        "M3 11.6 4.5 8a.9.9 0 0 1 .8-.6h7.9a.5.5 0 0 1 .5.7l-1.3 3.8a.9.9 0 0 1-.8.6H3.9a.9.9 0 0 1-.9-.9Z",
    ),
    "file": stroked(
        "M4.8 3.5h4.3l2.7 2.7V12a.5.5 0 0 1-.5.5H4.8a.5.5 0 0 1-.5-.5V4a.5.5 0 0 1 .5-.5Z",
        "M9.1 3.5v2.7h2.7",
    ),
    "text": stroked("M3.5 5h9", "M3.5 8h9", "M3.5 11h5.5"),
    "markdown": stroked(
        "M2.7 4.5h10.6a.7.7 0 0 1 .7.7v5.6a.7.7 0 0 1-.7.7H2.7a.7.7 0 0 1-.7-.7V5.2a.7.7 0 0 1 .7-.7Z",
        "M4.3 10V6.2l1.5 1.9 1.5-1.9V10",
        "M10.6 6.2v3.6M9.3 8.6l1.3 1.3 1.3-1.3",
    ),
    "braces": stroked(
        "M6 3.5c-1.2 0-1.6.5-1.6 1.5v1.3c0 .9-.4 1.5-1.4 1.7 1 .2 1.4.8 1.4 1.7V11c0 1 .4 1.5 1.6 1.5",
        "M10 3.5c1.2 0 1.6.5 1.6 1.5v1.3c0 .9.4 1.5 1.4 1.7-1 .2-1.4.8-1.4 1.7V11c0 1-.4 1.5-1.6 1.5",
    ),
    "markup": stroked("M5.5 5 2.8 8l2.7 3", "M10.5 5l2.7 3-2.7 3", "M9 4 7 12"),
    "config": stroked(
        "M3 5.5h3.5M9.5 5.5H13M3 10.5h5M11 10.5h2",
        extra='<circle cx="8" cy="5.5" r="1.5"/><circle cx="9.5" cy="10.5" r="1.5"/>',
    ),
    "lock": stroked(
        "M4.5 7.5h7a.5.5 0 0 1 .5.5v4a.5.5 0 0 1-.5.5h-7a.5.5 0 0 1-.5-.5V8a.5.5 0 0 1 .5-.5Z",
        "M5.8 7.5V5.8a2.2 2.2 0 0 1 4.4 0v1.7",
        "M8 9.6v.8",
    ),
    "git": stroked(
        "M5 5.7v4.6",
        "M11 7.7c0 2-2.6 2.3-5.4 3.1",
        extra='<circle cx="5" cy="4.5" r="1.2"/><circle cx="5" cy="11.5" r="1.2"/><circle cx="11" cy="6.5" r="1.2"/>',
    ),
    "image": stroked(
        "M3.5 3.5h9a.5.5 0 0 1 .5.5v8a.5.5 0 0 1-.5.5h-9a.5.5 0 0 1-.5-.5V4a.5.5 0 0 1 .5-.5Z",
        "M3.3 11.4 6.5 8l2.2 2.2 1.5-1.4 2.5 2.6",
        extra='<circle cx="10.3" cy="6" r=".9"/>',
    ),
    "package": stroked("M8 3l4.5 2.3v5.4L8 13l-4.5-2.3V5.3Z", "M3.5 5.3 8 7.6l4.5-2.3M8 7.6V13"),
    "book": stroked(
        "M8 5.2C6.8 4.2 5 3.9 3 4.1v7.6c2-.2 3.8.1 5 1.1 1.2-1 3-1.3 5-1.1V4.1c-2-.2-3.8.1-5 1.1Z",
        "M8 5.2v7.6",
    ),
    "license": stroked("M10 6.3a2.4 2.4 0 1 0 0 3.4", extra='<circle cx="8" cy="8" r="5"/>'),
    "terminal": stroked(
        "M3 3.5h10a.5.5 0 0 1 .5.5v8a.5.5 0 0 1-.5.5H3a.5.5 0 0 1-.5-.5V4a.5.5 0 0 1 .5-.5Z",
        "M5 6.5 6.8 8 5 9.5",
        "M8.4 10h2.6",
    ),
    "database": stroked(
        "M3.5 4.8c0-.9 2-1.6 4.5-1.6s4.5.7 4.5 1.6v6.4c0 .9-2 1.6-4.5 1.6s-4.5-.7-4.5-1.6Z",
        "M3.5 4.8c0 .9 2 1.6 4.5 1.6s4.5-.7 4.5-1.6",
        "M3.5 8c0 .9 2 1.6 4.5 1.6s4.5-.7 4.5-1.6",
    ),
    "plus": stroked("M8 4v8", "M4 8h8"),
    "close": stroked("M5 5l6 6", "M11 5l-6 6"),
    "sidebar": stroked(
        "M3.9 3.5h8.2a.9.9 0 0 1 .9.9v7.2a.9.9 0 0 1-.9.9H3.9a.9.9 0 0 1-.9-.9V4.4a.9.9 0 0 1 .9-.9Z",
        "M6.5 3.5v9",
    ),
    "split": stroked(
        "M3.9 3.5h8.2a.9.9 0 0 1 .9.9v7.2a.9.9 0 0 1-.9.9H3.9a.9.9 0 0 1-.9-.9V4.4a.9.9 0 0 1 .9-.9Z",
        "M8 3.5v9",
    ),
}

# Two letters per language. That is all that fits at 16 px.
TILES = {
    "rs": "RS", "py": "PY", "js": "JS", "ts": "TS", "go": "GO", "c": "C", "h": "H",
    "cpp": "C+", "java": "JV", "kt": "KT", "swift": "SW", "rb": "RB", "php": "PH",
    "cs": "C#", "fs": "F#", "lua": "LU", "dart": "DA", "zig": "ZG", "ex": "EX",
    "erl": "ER", "hs": "HS", "ml": "ML", "scala": "SC", "r": "R", "jl": "JL",
    "nim": "NM", "elm": "EL", "vue": "VU", "svelte": "SV", "css": "CS", "scss": "SA",
    "less": "LE", "vim": "VI", "pl": "PL", "clj": "CL", "gd": "GD", "font": "Aa",
}


def tile(letters, font):
    glyphs = font.getGlyphSet()
    cmap = font.getBestCmap()
    names = [cmap[ord(ch)] for ch in letters]
    cap = BoundsPen(glyphs)
    glyphs[cmap[ord("H")]].draw(cap)
    cap_height = cap.bounds[3]
    scale = 4.6 / cap_height  # letters 4.6 px tall
    width = sum(glyphs[n].width for n in names) * scale
    # Tighten wide pairs so they keep a margin inside the tile.
    scale = min(scale, 7.4 / (width / scale))
    width = sum(glyphs[n].width for n in names) * scale
    x = 8 - width / 2
    baseline = 8 + cap_height * scale / 2
    pen = SVGPathPen(glyphs)
    for n in names:
        glyphs[n].draw(TransformPen(pen, (scale, 0, 0, -scale, x, baseline)))
        x += glyphs[n].width * scale
    box = "M4.6 3h6.8A1.6 1.6 0 0 1 13 4.6v6.8a1.6 1.6 0 0 1-1.6 1.6H4.6A1.6 1.6 0 0 1 3 11.4V4.6A1.6 1.6 0 0 1 4.6 3Z"
    return f'{HEAD}<path fill="#000" fill-rule="evenodd" d="{box} {pen.getCommands()}"/></svg>\n'


def main():
    for old in OUT.glob("*.svg"):
        old.unlink()
    (OUT / "lang").mkdir(parents=True, exist_ok=True)
    for old in (OUT / "lang").glob("*.svg"):
        old.unlink()
    for name, svg in SYMBOLS.items():
        (OUT / f"{name}.svg").write_text(svg)
    font = TTFont(FONT)
    for key, letters in TILES.items():
        (OUT / "lang" / f"{key}.svg").write_text(tile(letters, font))
    print(f"{len(SYMBOLS)} symbols, {len(TILES)} language tiles -> {OUT}")


if __name__ == "__main__":
    main()
