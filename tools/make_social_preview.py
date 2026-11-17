"""Generates the image GitHub shows when someone shares a link to BIA.

Writes docs/social-preview.png (1280x640): the logo and tagline on the left,
the screenshot on the right. Upload it in the repository's Settings, under
"Social preview". Needs ImageMagick 7 (`magick`).

    python3 tools/make_social_preview.py
"""

import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "docs" / "social-preview.png"

# Colors from the Noite theme (assets/themes/noite.json).
PANEL = "#121116"
TEXT = "#dcd6cb"
MUTED = "#77717f"
INK = "#ece6da"
GREEN = "#3fb59b"


def main():
    template = (ROOT / "assets" / "icons" / "logo-template.svg").read_text()
    logo_svg = template.replace("{INK}", INK).replace("{BIRD}", GREEN)
    regular = ROOT / "assets" / "fonts" / "FiraSans-Regular.ttf"

    with tempfile.TemporaryDirectory() as tmp:
        logo = Path(tmp) / "logo.svg"
        logo.write_text(logo_svg)
        subprocess.run(
            [
                "magick",
                "-size", "1280x640", f"xc:{PANEL}",
                # Logo, rendered large and scaled down so the edges stay crisp.
                "(", "-background", "none", "-density", "600", str(logo),
                "-resize", "360x", ")",
                "-geometry", "+80+188", "-composite",
                "-density", "72",
                "-font", str(regular),
                "-fill", TEXT, "-pointsize", "34",
                "-annotate", "+84+386", "A happy minimalist editor.",
                "-fill", MUTED, "-pointsize", "23",
                "-annotate", "+84+430", "A code editor written in Rust,",
                "-annotate", "+84+462", "with no network code.",
                # The screenshot (a macOS window with its shadow) runs off the
                # right edge.
                "(", str(ROOT / "docs" / "screenshot.png"), "-resize", "930x", ")",
                "-geometry", "+600+60", "-composite",
                "-depth", "8",
                str(OUT),
            ],
            check=True,
        )
    print(f"Wrote {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
