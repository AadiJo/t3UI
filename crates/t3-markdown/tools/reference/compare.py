#!/usr/bin/env python3
"""Side-by-side and diff of a GPUI snapshot against the fork reference (both @2x).

Usage: compare.py <snapshot.png> <reference@2x.png> <out-prefix> [y0 y1]
The reference covers page x 316..1124 (CSS px) from y 0; the snapshot is the full window.
Writes <out-prefix>-side-<n>.png (reference left, ours right) in 1400px-tall bands, and
<out-prefix>-diff.png (absolute difference, amplified).
"""
import sys
from PIL import Image, ImageChops

snap = Image.open(sys.argv[1]).convert("RGB")
ref = Image.open(sys.argv[2]).convert("RGB")
out = sys.argv[3]
scale = snap.width / 1440
print("snapshot", snap.size, "scale", scale, "reference", ref.size)
x0 = int(316 * scale)
ours = snap.crop((x0, 0, x0 + ref.width, min(snap.height, ref.height)))
if scale != 2:
    ours = ours.resize((ref.width, int(ours.height * 2 / scale)))
height = min(ours.height, ref.height)
y0 = int(sys.argv[4]) if len(sys.argv) > 4 else 0
y1 = int(sys.argv[5]) if len(sys.argv) > 5 else height
band = 1400
n = 0
for top in range(y0, y1, band):
    bottom = min(top + band, y1)
    side = Image.new("RGB", (ref.width * 2 + 16, bottom - top), (255, 0, 255))
    side.paste(ref.crop((0, top, ref.width, bottom)), (0, 0))
    side.paste(ours.crop((0, top, ref.width, bottom)), (ref.width + 16, 0))
    side.save(f"{out}-side-{n}.png")
    n += 1
diff = ImageChops.difference(ref.crop((0, 0, ref.width, height)), ours.crop((0, 0, ref.width, height)))
diff = diff.point(lambda v: min(255, v * 4))
diff.save(f"{out}-diff.png")
print("bands", n)
