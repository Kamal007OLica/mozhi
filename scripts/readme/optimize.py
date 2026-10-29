#!/usr/bin/env python3
"""Shrinks the README PNGs in .github/readme/: 2x captures down to 1.5x, then a 256-colour palette
(with dithering, which hides the brand's canvas grain well). Raw app captures are deleted."""
from pathlib import Path

from PIL import Image

OUT = Path(__file__).resolve().parents[2] / ".github" / "readme"
PHOTOS = {"plate-maker.png"}  # a palette would posterise the portrait: these become JPEGs

for p in sorted(OUT.glob("*.png")):
    if p.name.endswith(".raw.png"):
        p.unlink()
        continue
    before = p.stat().st_size
    im = Image.open(p).convert("RGBA")
    if im.width > 1920:
        im = im.resize((1920, round(im.height * 1920 / im.width)), Image.LANCZOS)
    if p.name in PHOTOS:
        jpg = p.with_suffix(".jpg")
        im.convert("RGB").save(jpg, quality=86, optimize=True, progressive=True)
        p.unlink()
        print(f"  {p.name}: {before // 1024} KB -> {jpg.name} {jpg.stat().st_size // 1024} KB")
        continue
    im.quantize(colors=256, method=Image.Quantize.FASTOCTREE, dither=Image.Dither.FLOYDSTEINBERG).save(p, optimize=True)
    print(f"  {p.name}: {before // 1024} KB -> {p.stat().st_size // 1024} KB")
