#!/usr/bin/env python3
"""Build the bundled fonts in assets/fonts from their upstream sources.

The fork loads only upright faces: DM Sans Variable (wght axis, opsz pinned at 14) and
JetBrains Mono 400/500 (`apps/web/src/main.tsx:8-10`). Chromium synthesizes everything else,
and GPUI never synthesizes, so this script bakes Chromium's synthesis into static files:

- DM Sans 400/500/600/700: static instances at opsz 14, one family "DM Sans".
- DM Sans italics: each upright weight sheared like Skia's fake italic (skewX 0.25, ~14deg).
- JetBrains Mono 400/500: the upstream static files.
- JetBrains Mono "Bold": Chromium draws 700 as the 500 face plus Skia fake bold, which at the
  12px used for code and the terminal adds ~40 units of stroke (stems ~140/1000, between
  Bold 126 and ExtraBold 150). The variable font instanced at wght 760 matches that weight.
- JetBrains Mono italics (400/500/700): the matching upright face sheared, since the fork
  loads no italic and Chromium never shows the true cursive italic.

Usage (from the repo root, needs `pip install --user fonttools`):
    curl -sSLo /tmp/DMSans.ttf "https://github.com/google/fonts/raw/main/ofl/dmsans/DMSans%5Bopsz,wght%5D.ttf"
    curl -sSLo /tmp/jbm.zip https://github.com/JetBrains/JetBrainsMono/releases/download/v2.304/JetBrainsMono-2.304.zip
    unzip -q /tmp/jbm.zip -d /tmp/jbm
    python3 crates/t3-ui/tools/build_fonts.py /tmp/DMSans.ttf /tmp/jbm/fonts

Verify with `fc-scan --format "%{family[0]} | %{style[0]} | %{weight} | %{slant}\\n" assets/fonts/*.ttf`.
"""

import math
import sys
from pathlib import Path

from fontTools.pens.recordingPen import DecomposingRecordingPen
from fontTools.pens.transformPen import TransformPen
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "assets/fonts"
OPTICAL_SIZE = 14
# Skia's fake italic skew (SkPaint text skew -1/4 in y-down space).
SKEW = 0.25
# (wght, style name). The UI uses 400/500/600 everywhere and 700 once.
DM_SANS_WEIGHTS = [(400, "Regular"), (500, "Medium"), (600, "SemiBold"), (700, "Bold")]
# wght of the variable JetBrains Mono that matches Chromium's fake-bold 500 at 12px.
JETBRAINS_SYNTHETIC_BOLD = 760
VARIABLE_TABLES = ("STAT", "fvar", "avar", "gvar", "HVAR", "MVAR", "cvar")
HINTING_TABLES = ("fpgm", "prep", "cvt ", "gasp", "hdmx", "LTSH", "VDMX")


def italic_style(style: str) -> str:
    return "Italic" if style == "Regular" else f"{style} Italic"


def rename(font: TTFont, family: str, style: str, weight: int) -> None:
    """Name a static face so GPUI selects it from `family` by weight and style alone."""
    italic = "Italic" in style
    bold = style.startswith("Bold")
    postscript_family = family.replace(" ", "")
    full = family if style == "Regular" else f"{family} {style}"
    postscript = f"{postscript_family}-{style.replace(' ', '')}"
    names = {1: family, 2: style, 4: full, 6: postscript, 16: family, 17: style}
    table = font["name"]
    # Drop variable-font and instancer naming (e.g. "9pt") that would leak into the family.
    table.names = [n for n in table.names if n.nameID < 25 and n.nameID not in (3, 16, 17, 21, 22)]
    for name_id, value in names.items():
        table.setName(value, name_id, 3, 1, 0x409)
        table.setName(value, name_id, 1, 0, 0)
    table.setName(f"{postscript};{font['head'].fontRevision:.3f}", 3, 3, 1, 0x409)

    os2 = font["OS/2"]
    os2.usWeightClass = weight
    # fsSelection: keep USE_TYPO_METRICS (bit 7) etc., set ITALIC (0), BOLD (5), REGULAR (6).
    selection = os2.fsSelection & ~0b1100001
    if italic:
        selection |= 0b1
    if bold:
        selection |= 0b100000
    if not italic and not bold:
        selection |= 0b1000000
    os2.fsSelection = selection
    font["head"].macStyle = (0b1 if bold else 0) | (0b10 if italic else 0)


def strip(font: TTFont, tables: tuple[str, ...]) -> None:
    for table in tables:
        if table in font:
            del font[table]


def shear(font: TTFont) -> None:
    """Skew every glyph by SKEW (x += SKEW * y), like Chromium's synthetic oblique."""
    glyph_set = font.getGlyphSet()
    glyf = font["glyf"]
    hmtx = font["hmtx"]
    outlines = {}
    for name in font.getGlyphOrder():
        recording = DecomposingRecordingPen(glyph_set)
        glyph_set[name].draw(recording)
        pen = TTGlyphPen(None)
        recording.replay(TransformPen(pen, (1, 0, SKEW, 1, 0, 0)))
        outlines[name] = pen.glyph()
    for name, glyph in outlines.items():
        glyph.recalcBounds(glyf)
        glyf[name] = glyph
        advance, _ = hmtx[name]
        hmtx[name] = (advance, getattr(glyph, "xMin", 0))
    # Instructions were written for the upright outlines.
    strip(font, HINTING_TABLES)
    font["post"].italicAngle = -math.degrees(math.atan(SKEW))
    font["hhea"].caretSlopeRise = 1000
    font["hhea"].caretSlopeRun = round(1000 * SKEW)


def save(font: TTFont, file_name: str, note: str) -> None:
    path = OUT / file_name
    font.save(path)
    print(f"wrote {path.relative_to(ROOT)} ({note})")


def main() -> None:
    if len(sys.argv) != 3:
        print(__doc__)
        sys.exit(2)
    dm_sans = Path(sys.argv[1])
    jetbrains = Path(sys.argv[2])
    OUT.mkdir(parents=True, exist_ok=True)

    for weight, style in DM_SANS_WEIGHTS:
        static = instancer.instantiateVariableFont(
            TTFont(dm_sans), {"opsz": OPTICAL_SIZE, "wght": weight}, updateFontNames=False
        )
        strip(static, VARIABLE_TABLES)
        rename(static, "DM Sans", style, weight)
        save(static, f"DMSans-{style}.ttf", f"opsz {OPTICAL_SIZE}, wght {weight}")
        rename(static, "DM Sans", italic_style(style), weight)
        shear(static)
        save(static, f"DMSans-{style}Italic.ttf".replace("RegularItalic", "Italic"), "sheared")

    uprights = {
        "Regular": (400, TTFont(jetbrains / "ttf/JetBrainsMono-Regular.ttf")),
        "Medium": (500, TTFont(jetbrains / "ttf/JetBrainsMono-Medium.ttf")),
        "Bold": (
            700,
            instancer.instantiateVariableFont(
                TTFont(jetbrains / "variable/JetBrainsMono[wght].ttf"),
                {"wght": JETBRAINS_SYNTHETIC_BOLD},
                updateFontNames=False,
            ),
        ),
    }
    for style, (weight, font) in uprights.items():
        strip(font, VARIABLE_TABLES)
        rename(font, "JetBrains Mono", style, weight)
        save(font, f"JetBrainsMono-{style}.ttf", f"wght {weight}")
        rename(font, "JetBrains Mono", italic_style(style), weight)
        shear(font)
        save(
            font,
            f"JetBrainsMono-{style}Italic.ttf".replace("RegularItalic", "Italic"),
            "sheared",
        )


if __name__ == "__main__":
    main()
