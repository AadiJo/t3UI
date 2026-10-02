#!/usr/bin/env python3
"""Build the bundled fonts in assets/fonts from their upstream sources.

The web UI loads DM Sans Variable (wght axis, opsz pinned at 14) and JetBrains Mono 400/500;
the terminal and code blocks also need JetBrains Mono bold and italics.
GPUI needs static TTFs, so this cuts static DM Sans instances at opsz=14 and renames them to
one family, "DM Sans", which `t3_ui::fonts` registers.

Usage (from the repo root, needs `pip install --user fonttools`):
    curl -sSLo /tmp/DMSans.ttf "https://github.com/google/fonts/raw/main/ofl/dmsans/DMSans%5Bopsz,wght%5D.ttf"
    curl -sSLo /tmp/jbm.zip https://github.com/JetBrains/JetBrainsMono/releases/download/v2.304/JetBrainsMono-2.304.zip
    unzip -q /tmp/jbm.zip -d /tmp/jbm
    python3 crates/t3-ui/tools/build_fonts.py /tmp/DMSans.ttf /tmp/jbm/fonts/ttf

Verify with `fc-scan --format "%{family} | %{style} | %{weight}\\n" assets/fonts/*.ttf`.
"""

import shutil
import sys
from pathlib import Path

from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "assets/fonts"
FAMILY = "DM Sans"
OPTICAL_SIZE = 14
# (wght, style name). The UI uses 400/500/600 everywhere and 700 once.
WEIGHTS = [(400, "Regular"), (500, "Medium"), (600, "SemiBold"), (700, "Bold")]


def rename(font: TTFont, style: str) -> None:
    """Give every instance the same family so GPUI selects them by weight alone."""
    full = f"{FAMILY} {style}" if style != "Regular" else FAMILY
    postscript = f"DMSans-{style}"
    names = {1: FAMILY, 2: style, 4: full, 6: postscript, 16: FAMILY, 17: style}
    table = font["name"]
    # Drop variable-font naming (instance names, STAT labels like "9pt") that would leak
    # into the static family.
    table.names = [n for n in table.names if n.nameID < 25 and n.nameID not in (3, 21, 22)]
    for name_id, value in names.items():
        table.setName(value, name_id, 3, 1, 0x409)
        table.setName(value, name_id, 1, 0, 0)
    table.setName(f"{postscript};{font['head'].fontRevision:.3f}", 3, 3, 1, 0x409)

    os2 = font["OS/2"]
    head = font["head"]
    bold = style == "Bold"
    # fsSelection: keep USE_TYPO_METRICS (bit 7), set REGULAR (6) or BOLD (5).
    os2.fsSelection = (os2.fsSelection & ~0b1100001) | (0b100000 if bold else 0b1000000)
    head.macStyle = 0b1 if bold else 0


def main() -> None:
    if len(sys.argv) != 3:
        print(__doc__)
        sys.exit(2)
    variable = Path(sys.argv[1])
    jetbrains = Path(sys.argv[2])
    OUT.mkdir(parents=True, exist_ok=True)

    for weight, style in WEIGHTS:
        font = TTFont(variable)
        static = instancer.instantiateVariableFont(
            font, {"opsz": OPTICAL_SIZE, "wght": weight}, updateFontNames=False
        )
        for table in ("STAT", "fvar", "avar", "gvar", "HVAR", "MVAR", "cvar"):
            if table in static:
                del static[table]
        rename(static, style)
        path = OUT / f"DMSans-{style}.ttf"
        static.save(path)
        print(f"wrote {path.relative_to(ROOT)} (opsz {OPTICAL_SIZE}, wght {weight})")

    # Code and the terminal need bold and italics too (400, 500, 700, 400i, 700i).
    for style in ("Regular", "Medium", "Bold", "Italic", "BoldItalic"):
        source = jetbrains / f"JetBrainsMono-{style}.ttf"
        shutil.copy(source, OUT / source.name)
        print(f"copied {source.name}")


if __name__ == "__main__":
    main()
