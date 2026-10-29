#!/usr/bin/env bash
# Convert a Hugging Face Whisper fine-tune (e.g. a Tamil model) to a ggml file Mozhi can load.
#
#   scripts/convert-tamil-model.sh vasista22/whisper-tamil-large-v2 [quant]
#
# quant (optional): q5_0 | q5_1 | q8_0 — quantise with whisper.cpp after converting (smaller, faster).
#
# The result is written straight into Mozhi's models folder as ggml-<name>[-quant].bin and shows
# up in Mozhi → Models as a custom model. Files whose name contains "tamil" are decoded as Tamil only.
#
# Needs: git, git-lfs, python3 (with torch, transformers, numpy), cmake + a C++ compiler for quantising.
set -euo pipefail

repo="${1:?usage: $0 <huggingface-repo> [q5_0|q5_1|q8_0]}"
quant="${2:-}"
name="$(basename "$repo")"

case "$(uname -s)" in
  Darwin) models="$HOME/Library/Application Support/app.mozhi.desktop/models" ;;
  MINGW*|MSYS*|CYGWIN*) models="$APPDATA/app.mozhi.desktop/models" ;;
  *) models="$HOME/.local/share/app.mozhi.desktop/models" ;;
esac
mkdir -p "$models"

work="$(mktemp -d "${TMPDIR:-/tmp}/mozhi-convert.XXXXXX")"
trap 'rm -rf "$work"' EXIT
cd "$work"

echo "› fetching whisper.cpp (converter + quantiser)"
git clone --depth 1 https://github.com/ggml-org/whisper.cpp.git
echo "› fetching OpenAI whisper (mel filters + tokenizer assets)"
git clone --depth 1 https://github.com/openai/whisper.git
echo "› fetching $repo"
git lfs install --skip-repo >/dev/null
git clone --depth 1 "https://huggingface.co/$repo" model

echo "› converting to ggml (f16)"
python3 whisper.cpp/models/convert-h5-to-ggml.py model whisper .
out="ggml-$name.bin"
mv ggml-model.bin "$out"

if [[ -n "$quant" ]]; then
  echo "› quantising to $quant"
  cmake -S whisper.cpp -B whisper.cpp/build -DCMAKE_BUILD_TYPE=Release >/dev/null
  cmake --build whisper.cpp/build --target quantize -j >/dev/null
  qout="ggml-$name-$quant.bin"
  whisper.cpp/build/bin/quantize "$out" "$qout" "$quant"
  out="$qout"
fi

mv "$out" "$models/"
echo "✓ installed $models/$out — pick it in Mozhi → Models"
