#!/usr/bin/env bash
# Downloads sensible default models for OpenWhisper into ./models/.
# - Whisper: ggml-base.en.bin (~140 MB), good speed/quality tradeoff on CPU
# - Gemma 4 E2B Instruct, Q4_K_M GGUF (~3.1 GB), edge-optimized refiner
set -euo pipefail

DIR="$(cd "$(dirname "$0")/.." && pwd)/models"
mkdir -p "$DIR"

WHISPER_URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin"
WHISPER_OUT="$DIR/ggml-base.en.bin"

# Gemma 4 E2B Instruct (GGUF, Q4_K_M) — unsloth's anonymous-downloadable
# mirror (the official google/gemma-4-E2B-it and ggml-org repos are gated).
GEMMA_URL="https://huggingface.co/unsloth/gemma-4-E2B-it-GGUF/resolve/main/gemma-4-E2B-it-Q4_K_M.gguf"
GEMMA_OUT="$DIR/gemma-4-E2B-it-Q4_K_M.gguf"

fetch() {
  local url="$1" out="$2"
  if [ -f "$out" ]; then
    echo "✓ already have $(basename "$out")"
    return
  fi
  echo "↓ downloading $(basename "$out")"
  curl -L --fail --progress-bar -o "$out.tmp" "$url"
  mv "$out.tmp" "$out"
}

fetch "$WHISPER_URL" "$WHISPER_OUT"
fetch "$GEMMA_URL"   "$GEMMA_OUT"

echo
echo "Models ready in $DIR"
echo "  Whisper: $WHISPER_OUT"
echo "  Gemma:   $GEMMA_OUT"
echo
echo "Open the Settings window in OpenWhisper and point the model paths there,"
echo "or copy these paths."
