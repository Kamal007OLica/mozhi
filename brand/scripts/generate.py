#!/usr/bin/env python3
"""
Generates Mozhi's brand artwork as plain SVG — deterministic, no dependencies.

  python3 brand/scripts/generate.py            # writes brand/logo/* and brand/patterns/*
  python3 brand/scripts/generate.py --png      # also rasterises the app icon (needs PyMuPDF)

Motifs, all drawn from Tamil Nadu around 1000 CE:
  · ஃ (āytham)   three dots — the logo, the voice indicator, the three crowned kingdoms
  · sikku kolam  one unbroken line woven around a grid of dots (pulli)
  · vimana       Rajaraja Chola's Thanjavur tower (completed 1010 CE), set on a pixel grid
  · muvendar     the emblems of the three kingdoms: Chera bow, Chola tiger, Pandya twin fish
  · prabha       Nataraja's ring of fire — the "thinking" ring
  · kaavi        red-and-white temple-wall stripes
  · alai         the waves crossed by the Chola fleet in 1025 CE
"""
import math
import random
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LOGO = ROOT / "logo"
PAT = ROOT / "patterns"

KUMKUM = "#c3161c"
ARAKKU = "#6b0a0e"
MANJAL = "#f4c430"
OLAI = "#efe4cf"
PAPER = "#f7f0e3"
MAI = "#17110e"


def svg(w, h, body, extra=""):
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}"{extra}>\n'
        f"{body}\n</svg>\n"
    )


def write(path: Path, content: str):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content)
    print("  wrote", path.relative_to(ROOT.parent))


# ── ஃ  the mark ────────────────────────────────────────────────────────────
# Three dots in a triangle. Geometry is shared with the app's motion (overlay.ts):
# dot radius = 0.092 of the tile, triangle centred slightly low so it reads optically centred.
def aytham_dots(cx, cy, size):
    r = size * 0.092
    dx = size * 0.158
    up = size * 0.158
    down = size * 0.112
    return [(cx, cy - up, r), (cx - dx, cy + down, r), (cx + dx, cy + down, r)]


def mark(size=512, bg=KUMKUM, dot=MANJAL, ring=None, radius=0.225):
    rr = size * radius
    dots = "".join(f'<circle cx="{x:.2f}" cy="{y:.2f}" r="{r:.2f}" fill="{dot}"/>' for x, y, r in aytham_dots(size / 2, size / 2, size))
    stroke = f' stroke="{ring}" stroke-width="{size*0.02:.1f}"' if ring else ""
    inset = size * 0.01 if ring else 0
    return svg(size, size, f'<rect x="{inset}" y="{inset}" width="{size-2*inset}" height="{size-2*inset}" rx="{rr:.1f}" fill="{bg}"{stroke}/>{dots}')


def app_icon(size=1024):
    # macOS icon grid: 824px artwork on a 1024 canvas, with a printed offset shadow.
    s = size
    a = s * 0.805
    o = (s - a) / 2
    r = a * 0.225
    dots = "".join(
        f'<circle cx="{x:.2f}" cy="{y:.2f}" r="{rad:.2f}" fill="{MANJAL}" stroke="{MAI}" stroke-width="{a*0.012:.1f}"/>'
        for x, y, rad in aytham_dots(s / 2, s / 2 - s * 0.01, a)
    )
    body = (
        f'<defs><filter id="g"><feTurbulence type="fractalNoise" baseFrequency=".9" numOctaves="2" seed="7"/>'
        f'<feColorMatrix values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 0 .18 0"/><feComposite in2="SourceGraphic" operator="in"/></filter></defs>'
        f'<rect x="{o+s*0.016:.1f}" y="{o+s*0.022:.1f}" width="{a:.1f}" height="{a:.1f}" rx="{r:.1f}" fill="{MAI}"/>'
        f'<rect x="{o:.1f}" y="{o:.1f}" width="{a:.1f}" height="{a:.1f}" rx="{r:.1f}" fill="{KUMKUM}"/>'
        f'<rect x="{o:.1f}" y="{o:.1f}" width="{a:.1f}" height="{a:.1f}" rx="{r:.1f}" fill="{KUMKUM}" filter="url(#g)"/>'
        f"{dots}"
    )
    return svg(s, s, body)


def tray_template(size=44):
    # Monochrome template image for the macOS menu bar (black + alpha only).
    dots = "".join(f'<circle cx="{x:.2f}" cy="{y:.2f}" r="{r*1.25:.2f}" fill="#000"/>' for x, y, r in aytham_dots(size / 2, size / 2, size))
    return svg(size, size, dots)


def wordmark(color=MAI, accent=KUMKUM):
    # Live text so it stays editable; fonts.css must be loaded. The ழ — the sound only Tamil has — takes the accent.
    return svg(
        560,
        200,
        f'<style>.t{{font-family:"Anek Tamil",sans-serif;font-weight:800;font-variation-settings:"wdth" 75}}'
        f'.l{{font-family:"Anek Latin",sans-serif;font-weight:600;letter-spacing:.32em;font-variation-settings:"wdth" 110}}</style>'
        f'<text class="t" x="0" y="132" font-size="150" fill="{color}">மொ<tspan fill="{accent}">ழி</tspan></text>'
        f'<text class="l" x="6" y="188" font-size="30" fill="{color}">MOZHI</text>',
    )


# ── Sikku kolam · mirror-curve algorithm ───────────────────────────────────
# Dots sit at cell centres of an m×n grid. The line runs diagonally between the
# midpoints of cell edges; at an edge marked as a mirror (always the outer border)
# it bounces, otherwise it crosses straight through. Every diamond side is used once,
# so the result is one or more closed curves woven around every dot.
def kolam_paths(m, n, mirrors):
    def is_mirror(q):
        x, y = q
        if x == int(x):  # vertical edge at x, spanning y±0.5
            return x == 0 or x == m or ("v", int(x), int(y - 0.5)) in mirrors
        return y == 0 or y == n or ("h", int(x - 0.5), int(y)) in mirrors

    def seg_key(a, b):
        return (a, b) if a < b else (b, a)

    seen = set()
    total = 4 * m * n
    curves = []
    starts = [((i + 0.5, j), (0.5, 0.5)) for j in range(n) for i in range(m)]
    for start in starts:
        p, d = start
        if seg_key(p, (p[0] + d[0], p[1] + d[1])) in seen:
            continue
        pts = []
        while True:
            q = (p[0] + d[0], p[1] + d[1])
            k = seg_key(p, q)
            if k in seen:
                break
            seen.add(k)
            pts.append(p)
            if is_mirror(q):
                d = (-d[0], d[1]) if q[0] == int(q[0]) else (d[0], -d[1])
            p = q
        if pts:
            curves.append(pts)
        if len(seen) == total:
            break
    return curves


def smooth_closed(pts, s, ox, oy):
    def P(p):
        return (ox + p[0] * s, oy + p[1] * s)

    k = len(pts)
    mids = [((pts[i][0] + pts[(i + 1) % k][0]) / 2, (pts[i][1] + pts[(i + 1) % k][1]) / 2) for i in range(k)]
    x0, y0 = P(mids[-1])
    d = [f"M{x0:.1f} {y0:.1f}"]
    for i in range(k):
        cx, cy = P(pts[i])
        ex, ey = P(mids[i])
        d.append(f"Q{cx:.1f} {cy:.1f} {ex:.1f} {ey:.1f}")
    return "".join(d) + "Z"


def symmetric_mirrors(m, n, rng, density):
    # Four-fold symmetric mirror placement so kolams look hand-drawn, not random.
    mir = set()
    for i in range(m):
        for j in range(1, n):
            if rng.random() < density:
                for (a, b) in {(i, j), (m - 1 - i, j), (i, n - j), (m - 1 - i, n - j)}:
                    mir.add(("h", a, b))
    for i in range(1, m):
        for j in range(n):
            if rng.random() < density:
                for (a, b) in {(i, j), (m - i, j), (i, n - 1 - j), (m - i, n - 1 - j)}:
                    mir.add(("v", a, b))
    return mir


def kolam(m=7, n=7, seed=3, cell=40, line=MANJAL, dot=MANJAL, bg=None, stroke=3.2, want_single=True, density=0.22):
    rng = random.Random(seed)
    best = None
    for _ in range(400):
        mir = symmetric_mirrors(m, n, rng, density)
        curves = kolam_paths(m, n, mir)
        if best is None or len(curves) < len(best):
            best = curves
        if not want_single or len(curves) == 1:
            break
    pad = cell * 0.6
    w, h = m * cell + 2 * pad, n * cell + 2 * pad
    body = f'<rect width="{w}" height="{h}" fill="{bg}"/>' if bg else ""
    for c in best:
        body += f'<path d="{smooth_closed(c, cell, pad, pad)}" fill="none" stroke="{line}" stroke-width="{stroke}" stroke-linecap="round" stroke-linejoin="round"/>'
    for i in range(m):
        for j in range(n):
            body += f'<circle cx="{pad+(i+0.5)*cell:.1f}" cy="{pad+(j+0.5)*cell:.1f}" r="{stroke*0.95:.1f}" fill="{dot}"/>'
    return svg(round(w), round(h), body), len(best)


# ── Vimana · Thanjavur Periya Kovil on a pixel grid ─────────────────────────
def vimana_rows():
    W = 41
    rows = []

    def centered(width, pattern=None):
        width = max(1, min(W, width))
        left = (W - width) // 2
        r = [0] * W
        for i in range(width):
            r[left + i] = 1 if pattern is None else pattern(i, width)
        return r

    # kalasam (finial)
    rows += [centered(1), centered(3), centered(1), centered(3), centered(5)]
    # shikhara dome
    for w_ in (7, 11, 13, 13, 11):
        rows.append(centered(w_))
    rows.append(centered(15, lambda i, w: 0 if i in (1, w - 2) else 1))  # griva with nandis
    rows.append(centered(9))
    # thirteen talas (tiers), each: a row of kudu arches over a solid band
    for t in range(13):
        w_ = 13 + t * 2
        rows.append(centered(w_, lambda i, w: 0 if (i % 3 == 1 and 1 < i < w - 2) else 1))
        rows.append(centered(w_ + 1 if (w_ + 1) % 2 else w_))
    # two storeys of walls with deep niches (koshtams)
    for _ in range(2):
        rows.append(centered(37))
        for _r in range(3):
            rows.append(centered(37, lambda i, w: 0 if (i % 4 in (2,) and 1 < i < w - 2) else 1))
        rows.append(centered(39, lambda i, w: 0 if i in (0, w - 1) else 1))
    # adhishtana (moulded base)
    rows.append(centered(41))
    rows.append(centered(41, lambda i, w: 0 if i % 2 else 1))
    rows.append(centered(41))
    return rows


def vimana(cell=10, gap=1.6, fill=MANJAL, bg=None, ground=None):
    rows = vimana_rows()
    W, H = len(rows[0]), len(rows)
    w, h = W * cell, H * cell
    body = f'<rect width="{w}" height="{h}" fill="{bg}"/>' if bg else ""
    for y, row in enumerate(rows):
        for x, v in enumerate(row):
            if v:
                body += f'<rect x="{x*cell+gap/2:.1f}" y="{y*cell+gap/2:.1f}" width="{cell-gap:.1f}" height="{cell-gap:.1f}" fill="{fill}"/>'
    return svg(w, h, body)


# ── Muvendar · the three crowned kingdoms ──────────────────────────────────
TIGER = [
    "..#......................",
    ".#.......................",
    ".#................#..#...",
    ".#...............######..",
    "..#..............#######.",
    "...###########...##.####.",
    "...#.##.##.##.######.####",
    "...###########.##########",
    "...################......",
    "...###..######..###......",
    "...##...##.....##........",
    "..###..###....###........",
]


def tiger(cell=12, fill=MANJAL, stripe=None):
    W, H = len(TIGER[0]), len(TIGER)
    body = ""
    for y, row in enumerate(TIGER):
        for x, ch in enumerate(row):
            if ch == "#":
                body += f'<rect x="{x*cell}" y="{y*cell}" width="{cell}" height="{cell}" fill="{fill}"/>'
            elif stripe and ch == "." and 5 <= y <= 7 and 3 < x < 16:
                body += f'<rect x="{x*cell}" y="{y*cell}" width="{cell}" height="{cell}" fill="{stripe}"/>'
    return svg(W * cell, H * cell, body)


def bow(size=300, color=MANJAL, sw=14):
    s = size
    body = (
        f'<path d="M{s*0.3} {s*0.1} Q{s*0.95} {s*0.5} {s*0.3} {s*0.9}" fill="none" stroke="{color}" stroke-width="{sw}" stroke-linecap="round"/>'
        f'<line x1="{s*0.3}" y1="{s*0.1}" x2="{s*0.3}" y2="{s*0.9}" stroke="{color}" stroke-width="{sw*0.35}"/>'
        f'<line x1="{s*0.12}" y1="{s*0.5}" x2="{s*0.86}" y2="{s*0.5}" stroke="{color}" stroke-width="{sw*0.55}" stroke-linecap="round"/>'
        f'<path d="M{s*0.86} {s*0.5} l-{s*0.09} -{s*0.055} v{s*0.11}z" fill="{color}"/>'
        f'<path d="M{s*0.12} {s*0.5} l-{s*0.05} -{s*0.05} M{s*0.12} {s*0.5} l-{s*0.05} {s*0.05}" stroke="{color}" stroke-width="{sw*0.4}" stroke-linecap="round"/>'
    )
    return svg(s, s, body)


def fish(size=300, color=MANJAL):
    s = size

    def one(y, flip):
        sy = -1 if flip else 1
        body = (
            f'<path d="M{s*0.12} {y} Q{s*0.45} {y-sy*s*0.16} {s*0.74} {y} Q{s*0.45} {y+sy*s*0.12} {s*0.12} {y}Z" fill="{color}"/>'
            f'<path d="M{s*0.72} {y} L{s*0.9} {y-s*0.08} L{s*0.86} {y} L{s*0.9} {y+s*0.08}Z" fill="{color}"/>'
            f'<circle cx="{s*0.22}" cy="{y-sy*s*0.012}" r="{s*0.018}" fill="{KUMKUM}"/>'
        )
        return body

    return svg(s, s, one(s * 0.36, False) + one(s * 0.64, True))


# ── Prabha · Nataraja's ring of fire ───────────────────────────────────────
def prabha(size=240, color=MANJAL, flames=21, inner=0.34, outer=0.46, sw=5):
    c = size / 2
    body = f'<circle cx="{c}" cy="{c}" r="{size*inner}" fill="none" stroke="{color}" stroke-width="{sw}"/>'
    for k in range(flames):
        a = 2 * math.pi * k / flames
        a1, a2 = a - math.pi / flames * 0.8, a + math.pi / flames * 0.8
        r0, r1 = size * inner, size * outer
        x1, y1 = c + r0 * math.cos(a1), c + r0 * math.sin(a1)
        x2, y2 = c + r0 * math.cos(a2), c + r0 * math.sin(a2)
        tx, ty = c + r1 * math.cos(a + 0.12), c + r1 * math.sin(a + 0.12)
        mx, my = c + (r0 + (r1 - r0) * 0.55) * math.cos(a - 0.08), c + (r0 + (r1 - r0) * 0.55) * math.sin(a - 0.08)
        body += f'<path d="M{x1:.1f} {y1:.1f} Q{mx:.1f} {my:.1f} {tx:.1f} {ty:.1f} Q{(x2+mx)/2:.1f} {(y2+my)/2:.1f} {x2:.1f} {y2:.1f}Z" fill="{color}"/>'
    return svg(size, size, body)


# ── Repeating tiles ────────────────────────────────────────────────────────
def tile_alai(w=80, h=40, color=MANJAL, sw=2.4):
    # Fish-scale waves, the sea of the Chola fleet.
    r = w / 4
    p = ""
    for row, yo in ((0, 0), (1, h / 2)):
        xo = 0 if row == 0 else -w / 4
        for k in range(-1, 3):
            x = xo + k * (w / 2)
            p += f'<path d="M{x:.1f} {yo+h/2:.1f} a{r:.1f} {r:.1f} 0 0 1 {w/2:.1f} 0" fill="none" stroke="{color}" stroke-width="{sw}"/>'
            p += f'<path d="M{x+w*0.07:.1f} {yo+h/2:.1f} a{r*0.72:.1f} {r*0.72:.1f} 0 0 1 {w/2-w*0.14:.1f} 0" fill="none" stroke="{color}" stroke-width="{sw*0.6}" opacity=".6"/>'
    return svg(w, h, p)


def tile_pulli(s=28, color=MANJAL, r=1.6):
    return svg(s, s, f'<circle cx="{s/2}" cy="{s/2}" r="{r}" fill="{color}"/>')


def band_kodi(w=96, h=40, color=MANJAL, bg=None):
    # Temple cornice band: lotus petals between beaded rules.
    b = f'<rect width="{w}" height="{h}" fill="{bg}"/>' if bg else ""
    b += f'<rect y="2" width="{w}" height="2.5" fill="{color}"/><rect y="{h-4.5}" width="{w}" height="2.5" fill="{color}"/>'
    for k in range(4):
        cx = w / 8 + k * w / 4
        b += f'<path d="M{cx-9} {h-8} Q{cx-9} {h*0.32} {cx} {9} Q{cx+9} {h*0.32} {cx+9} {h-8}Z" fill="none" stroke="{color}" stroke-width="2"/>'
        b += f'<path d="M{cx} {h-8} V{15}" stroke="{color}" stroke-width="1.5"/>'
        b += f'<circle cx="{cx + w/8}" cy="{h/2}" r="2.2" fill="{color}"/>'
    return svg(w, h, b)


def main():
    print("Mozhi brand artwork")
    write(LOGO / "mozhi-mark.svg", mark())
    write(LOGO / "mozhi-mark-ink.svg", mark(bg=MAI))
    write(LOGO / "mozhi-mark-paper.svg", mark(bg=PAPER, dot=KUMKUM))
    write(LOGO / "mozhi-app-icon.svg", app_icon())
    write(LOGO / "mozhi-tray-template.svg", tray_template())
    write(LOGO / "mozhi-wordmark.svg", wordmark())
    write(LOGO / "mozhi-wordmark-paper.svg", wordmark(color=PAPER, accent=MANJAL))

    k, nc = kolam(7, 7, seed=11)
    write(PAT / "kolam-7.svg", k)
    print("    kolam-7 curves:", nc)
    k, nc = kolam(5, 5, seed=4, cell=36)
    write(PAT / "kolam-5.svg", k)
    k, nc = kolam(9, 5, seed=21, cell=34, line=MAI, dot=MAI, stroke=2.6)
    write(PAT / "kolam-9x5-ink.svg", k)
    k, nc = kolam(13, 13, seed=5, cell=30, line=MANJAL, dot=MANJAL, stroke=2.4, density=0.18)
    write(PAT / "kolam-13.svg", k)

    write(PAT / "vimana.svg", vimana())
    write(PAT / "vimana-ink.svg", vimana(fill=MAI))
    write(PAT / "muvendar-tiger.svg", tiger(stripe=KUMKUM))
    write(PAT / "muvendar-bow.svg", bow())
    write(PAT / "muvendar-fish.svg", fish())
    write(PAT / "prabha.svg", prabha())
    write(PAT / "tile-alai.svg", tile_alai())
    write(PAT / "tile-pulli.svg", tile_pulli())
    write(PAT / "band-kodi.svg", band_kodi())
    write(PAT / "band-kodi-ink.svg", band_kodi(color=MAI))

    if "--png" in sys.argv:
        import fitz  # PyMuPDF

        for name, out, px in (("mozhi-app-icon.svg", "mozhi-app-icon-1024.png", 1024), ("mozhi-tray-template.svg", "mozhi-tray-template@2x.png", 44)):
            doc = fitz.open(str(LOGO / name))
            page = doc[0]
            zoom = px / page.rect.width
            pix = page.get_pixmap(matrix=fitz.Matrix(zoom, zoom), alpha=True)
            pix.save(str(LOGO / out))
            print("  rasterised", out)


if __name__ == "__main__":
    main()
