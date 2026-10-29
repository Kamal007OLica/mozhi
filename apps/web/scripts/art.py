#!/usr/bin/env python3
"""
Artwork for the landing page, drawn with the brand generators (brand/scripts/generate.py)
in the colours each section needs. Output is committed to public/art so the site builds
without Python:   npm run art --workspace apps/web
"""
import importlib.util
from pathlib import Path

HERE = Path(__file__).resolve().parent
OUT = HERE.parent / "public" / "art"
GEN = HERE.parents[2] / "brand" / "scripts" / "generate.py"

spec = importlib.util.spec_from_file_location("mozhi_brand", GEN)
g = importlib.util.module_from_spec(spec)
spec.loader.exec_module(g)

KUMKUM, MANJAL, PAPER, MAI, ARAKKU = g.KUMKUM, g.MANJAL, g.PAPER, g.MAI, g.ARAKKU


def put(name, content):
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / name).write_text(content)
    print("  art/" + name)


def pixel_blob(cols=22, rows=11, cell=40, seed=7):
    """The DIGIFEST-style stepped black field, with a 1px grid drawn through it."""
    import random

    rng = random.Random(seed)
    # Per-column top/bottom insets so the silhouette steps like a pixel cloud.
    tops, bots = [], []
    for c in range(cols):
        edge = min(c, cols - 1 - c)
        t = max(0, 3 - edge) + rng.choice([0, 0, 1, 1, 2])
        b = max(0, 3 - edge) + rng.choice([0, 0, 1, 1])
        tops.append(min(t, rows // 2 - 1))
        bots.append(min(b, rows // 2 - 1))
    body = ""
    for c in range(cols):
        for r in range(tops[c], rows - bots[c]):
            body += f'<rect x="{c*cell}" y="{r*cell}" width="{cell}" height="{cell}" fill="{MAI}"/>'
    lines = ""
    for c in range(1, cols):
        lines += f'<line x1="{c*cell}" y1="0" x2="{c*cell}" y2="{rows*cell}"/>'
    for r in range(1, rows):
        lines += f'<line x1="0" y1="{r*cell}" x2="{cols*cell}" y2="{r*cell}"/>'
    clip = "".join(
        f'<rect x="{c*cell}" y="{tops[c]*cell}" width="{cell}" height="{(rows-bots[c]-tops[c])*cell}"/>' for c in range(cols)
    )
    body += f'<clipPath id="b">{clip}</clipPath><g clip-path="url(#b)" stroke="{PAPER}" stroke-opacity=".22" stroke-width="1">{lines}</g>'
    return g.svg(cols * cell, rows * cell, body, ' preserveAspectRatio="none"')


FONTS = {
    "AnekTamil-normal-100-800-tamil.woff2": "AnekTamil-tamil.woff2",
    "AnekLatin-normal-100-800-latin.woff2": "AnekLatin-latin.woff2",
    "TiroTamil-normal-400-tamil.woff2": "TiroTamil-tamil.woff2",
    "TiroTamil-normal-400-latin.woff2": "TiroTamil-latin.woff2",
    "TiroTamil-italic-400-tamil.woff2": "TiroTamil-italic-tamil.woff2",
    "JetBrainsMono-normal-400-latin.woff2": "JetBrainsMono-latin.woff2",
}


def copy_fonts():
    """Self-host only the brand font subsets the site uses, under stable names (so they can be preloaded)."""
    import shutil

    src = HERE.parents[2] / "brand" / "fonts"
    dst = HERE.parent / "public" / "fonts"
    dst.mkdir(parents=True, exist_ok=True)
    for a, b in FONTS.items():
        shutil.copyfile(src / a, dst / b)
        print("  fonts/" + b)


def copy_tokens():
    """Vendor the brand tokens so apps/web builds on its own (e.g. as a Vercel project root)."""
    src = HERE.parents[2] / "brand" / "tokens.css"
    dst = HERE.parent / "src" / "tokens.css"
    dst.write_text("/* Copied from brand/tokens.css by scripts/art.py; edit the original. */\n" + src.read_text())
    print("  src/tokens.css")


def main():
    print("Mozhi web artwork")
    copy_fonts()
    copy_tokens()
    put("mark.svg", g.mark(size=96))

    k, _ = g.kolam(11, 11, seed=8, cell=36, line=MANJAL, dot=MANJAL, stroke=2.2, density=0.2)
    put("kolam-hero.svg", k)
    k, _ = g.kolam(7, 7, seed=11, cell=40, line=KUMKUM, dot=KUMKUM, stroke=3)
    put("kolam-kumkum.svg", k)

    put("vimana-ink.svg", g.vimana(fill=MAI))
    put("vimana-arakku.svg", g.vimana(fill=ARAKKU))

    put("tiger.svg", g.tiger(fill=MANJAL, stripe=MAI))
    put("bow.svg", g.bow(color=MANJAL))
    put("fish.svg", g.fish(color=MANJAL))

    put("band-kodi.svg", g.band_kodi(color=MANJAL))
    put("tile-alai.svg", g.tile_alai(color=MANJAL))
    put("pixel-blob.svg", pixel_blob())

    # ஃ on its own — solid brand dots, used where the font's hollow glyph would read as "oo".
    for name, color in (("aytham-manjal.svg", MANJAL), ("aytham-kumkum.svg", KUMKUM)):
        dots = "".join(f'<circle cx="{x:.2f}" cy="{y:.2f}" r="{r:.2f}" fill="{color}"/>' for x, y, r in g.aytham_dots(48, 52, 96))
        put(name, g.svg(96, 96, dots).replace('viewBox="0 0 96 96"', 'viewBox="24 26 48 48"'))

    # Social card / favicon sources
    put("favicon.svg", g.mark(size=64))

    # Raster touch icon (optional: needs PyMuPDF; the committed PNG is used otherwise).
    try:
        import fitz

        doc = fitz.open(stream=g.mark(size=180, radius=0).encode(), filetype="svg")
        doc[0].get_pixmap(alpha=False).save(str(OUT.parent / "apple-touch-icon.png"))
        print("  apple-touch-icon.png")
    except ImportError:
        print("  (skipped apple-touch-icon.png — pip install pymupdf)")


if __name__ == "__main__":
    main()
