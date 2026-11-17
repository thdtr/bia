"""Generates the card shown when someone shares a link to BIA.

Writes, at 1280x640 (GitHub's size, which also works as an og:image):

  docs/social-preview.png          English, for the repository's Settings,
                                   under "Social preview" (uploaded by hand)
  site/static/img/social-en.png    English, for the website
  site/static/img/social-pt-br.png Brazilian Portuguese, for the website

The card is read as a small thumbnail (about 500px wide in most chat apps), so
it holds the logo, one line set large and the editor itself, cropped, with
everything that matters at least 80px from the edges. The screenshot is
docs/screenshot.png. Needs Pillow and ImageMagick 7 (`magick`, for the logo).

    python3 tools/make_social_preview.py
"""

import subprocess
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
W, H = 1280, 640
MARGIN = 80

# Colors from the Noite theme (assets/themes/noite.json) and the logo.
PANEL = "#121116"
INK = "#ece6da"
GREEN = "#3fb59b"

LINES = {
    "en": ["A happy", "minimalist editor."],
    "pt-br": ["Um editor", "minimalista feliz."],
}
OUTPUTS = {
    "en": [ROOT / "docs" / "social-preview.png", ROOT / "site" / "static" / "img" / "social-en.png"],
    "pt-br": [ROOT / "site" / "static" / "img" / "social-pt-br.png"],
}

# Where the window sits inside docs/screenshot.png, as fractions of its size
# (the rest is the macOS shadow).
WINDOW_X, WINDOW_Y = 112 / 2584, 76 / 1808


def logo(width):
    template = (ROOT / "assets" / "icons" / "logo-template.svg").read_text()
    svg = template.replace("{INK}", INK).replace("{BIRD}", GREEN)
    with tempfile.TemporaryDirectory() as tmp:
        src, out = Path(tmp) / "logo.svg", Path(tmp) / "logo.png"
        src.write_text(svg)
        subprocess.run(
            ["magick", "-background", "none", "-density", "600", str(src), "-resize", f"{width}x", str(out)],
            check=True,
        )
        return Image.open(out).convert("RGBA")


def card(lines):
    canvas = Image.new("RGBA", (W, H), PANEL)

    # The editor, large, running off the right and bottom edges.
    shot = Image.open(ROOT / "docs" / "screenshot.png").convert("RGBA")
    scale = 1150 / shot.width
    shot = shot.resize((round(shot.width * scale), round(shot.height * scale)), Image.LANCZOS)
    left, top = 610, 150
    canvas.alpha_composite(shot, (left - round(shot.width * WINDOW_X), top - round(shot.height * WINDOW_Y)))

    canvas.alpha_composite(logo(250), (MARGIN, 150))

    font = ImageFont.truetype(str(ROOT / "tools" / "fonts" / "FiraSans-Light.ttf"), 58)
    draw = ImageDraw.Draw(canvas)
    y = 330
    for line in lines:
        draw.text((MARGIN - 3, y), line, font=font, fill=INK)
        y += 70
    return canvas.convert("RGB")


def main():
    for lang, lines in LINES.items():
        image = card(lines)
        for out in OUTPUTS[lang]:
            image.save(out, optimize=True)
            print(f"Wrote {out.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
