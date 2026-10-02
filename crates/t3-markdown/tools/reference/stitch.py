# Stitch unscaled 640x400 CSS-px tiles (1280x800 PNGs at DPR 2) captured from
# <theme>.html?tile into one full-height @2x image of the chat column.
#
# Usage: python3 stitch.py <theme> <x,y=path> ...
#   x,y = window scroll offset (CSS px) when the tile was captured.
# Writes tiles/<theme>/x<X>-y<Y>.png and reference-<theme>-full@2x.png.
# Crop: page x 316..1124 (the 768px column at 336..1104 plus 20px each side),
# y 0..document height. Pixel (px, py) in the output = page CSS point
# (316 + px/2, py/2). The .chat-markdown root sits at page (340, 18).
import json
import os
import shutil
import sys

from PIL import Image, ImageChops

OUT = os.environ.get("T3_MD_REF_OUT", "/tmp/markdown-reference")
DPR = 2
TILE_W, TILE_H = 640, 400
CROP_X0, CROP_X1 = 316, 1124
DOC_H = 2037

theme = sys.argv[1]
tiles = {}
os.makedirs(f"{OUT}/tiles/{theme}", exist_ok=True)
for arg in sys.argv[2:]:
    pos, path = arg.split("=", 1)
    x, y = map(int, pos.split(","))
    dest = f"{OUT}/tiles/{theme}/x{x}-y{y}.png"
    shutil.copyfile(path, dest)
    image = Image.open(dest).convert("RGB")
    assert image.size == (TILE_W * DPR, TILE_H * DPR), (dest, image.size)
    tiles[(x, y)] = image

canvas = Image.new("RGB", ((CROP_X1 - CROP_X0) * DPR, DOC_H * DPR))
coverage = Image.new("L", canvas.size, 0)
mismatches = []
for (x, y), image in sorted(tiles.items()):
    # Region of this tile inside the crop, in device px.
    left, top = (x - CROP_X0) * DPR, y * DPR
    src = image.crop((max(0, -left), 0, min(image.width, canvas.width - left), image.height))
    dst = (max(0, left), top)
    # Compare against already-painted pixels where coverage exists.
    region = (dst[0], dst[1], dst[0] + src.width, dst[1] + src.height)
    covered = coverage.crop(region)
    if covered.getbbox():
        diff = ImageChops.difference(canvas.crop(region), src)
        masked = ImageChops.multiply(diff.convert("L"), covered)
        if masked.getbbox():
            mismatches.append({"tile": [x, y], "bbox": masked.getbbox()})
    canvas.paste(src, dst)
    coverage.paste(255, region)

missing = coverage.point(lambda v: 255 - v).getbbox()
out = f"{OUT}/reference-{theme}-full@2x.png"
canvas.save(out, optimize=True)
print(json.dumps({"out": out, "size": canvas.size, "tiles": len(tiles), "mismatches": mismatches, "uncovered": missing}))
