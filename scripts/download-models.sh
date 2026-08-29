#!/usr/bin/env bash
# Downloads sensible default models for OpenWhisper into ./models/.
# - Whisper: ggml-small.en.bin (~466 MB), default for regular laptops
# - Qwen3 4B Instruct 2507, Q4_K_M GGUF (~2.3 GB), local transcript refiner
set -euo pipefail

DIR="$(cd "$(dirname "$0")/.." && pwd)/models"
mkdir -p "$DIR"

WHISPER_URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin"
WHISPER_OUT="$DIR/ggml-small.en.bin"

# Qwen3 4B Instruct 2507 (GGUF, Q4_K_M) — unsloth's anonymous-downloadable
# mirror. Non-thinking instruct checkpoint.
LLM_URL="https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF/resolve/main/Qwen3-4B-Instruct-2507-Q4_K_M.gguf"
LLM_OUT="$DIR/Qwen3-4B-Instruct-2507-Q4_K_M.gguf"

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
fetch "$LLM_URL"     "$LLM_OUT"

echo
echo "Models ready in $DIR"
echo "  Whisper: $WHISPER_OUT"
echo "  LLM:     $LLM_OUT"
echo
echo "Open the Settings window in OpenWhisper and point the model paths there,"
echo "or copy these paths."
