#!/bin/sh
# Renders scripts/og.html to public/og.jpg (1200×630) with headless Chrome; sips (macOS) converts to JPEG.
set -e
cd "$(dirname "$0")/.."
CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
"$CHROME" --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=1 \
  --window-size=1200,630 --virtual-time-budget=3000 --allow-file-access-from-files \
  --screenshot="$PWD/public/og.png" "file://$PWD/scripts/og.html" >/dev/null 2>&1
sips -s format jpeg -s formatOptions 74 public/og.png --out public/og.jpg >/dev/null
rm public/og.png
echo "wrote public/og.jpg"
