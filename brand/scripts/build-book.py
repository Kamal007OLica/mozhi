#!/usr/bin/env python3
"""Builds brand/index.html — the self-contained brand book — from book.src.html.

  {{css:tokens.css}}           inline a stylesheet
  {{svg:patterns/kolam-7.svg}}  inline an SVG (ids made unique per use)
  {{uri:patterns/tile-alai.svg}} percent-encode an SVG for a CSS data: URL
"""
import re
import urllib.parse
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
count = 0


def inline_svg(path):
    global count
    count += 1
    s = (ROOT / path).read_text()
    s = re.sub(r'\s(width|height)="[\d.]+"', "", s, count=2)
    # keep filter/pattern ids unique when the same file is inlined twice
    s = re.sub(r'id="(\w+)"', lambda m: f'id="{m.group(1)}{count}"', s)
    s = re.sub(r"url\(#(\w+)\)", lambda m: f"url(#{m.group(1)}{count})", s)
    return s.strip()


def sub(m):
    kind, path = m.group(1), m.group(2)
    if kind == "css":
        return (ROOT / path).read_text()
    if kind == "svg":
        return inline_svg(path)
    if kind == "uri":
        return urllib.parse.quote((ROOT / path).read_text().strip(), safe="")
    raise ValueError(kind)


src = (ROOT / "book.src.html").read_text()
out = re.sub(r"\{\{(css|svg|uri):([^}]+)\}\}", sub, src)
(ROOT / "index.html").write_text(out)
print(f"wrote brand/index.html ({len(out)//1024} KB)")
