#!/usr/bin/env bash
# scripts/download_onnx_models.sh
# Downloads and extracts the pre-trained, quantized ONNX models for local STT and TTS.

set -euo pipefail

STT_MODEL="tiny"
TTS_MODEL="medium"

# Parse arguments
while [[ "$#" -gt 0 ]]; do
    case $1 in
        --stt) STT_MODEL="$2"; shift ;;
        --tts) TTS_MODEL="$2"; shift ;;
        *) echo "Unknown parameter passed: $1"; exit 1 ;;
    esac
    shift
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MODELS_DIR="$REPO_ROOT/models"

mkdir -p "$MODELS_DIR"
cd "$MODELS_DIR"

echo "==> Downloading and extracting Whisper ASR model (STT: $STT_MODEL)..."
if [ "$STT_MODEL" == "tiny" ]; then
    URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-whisper-tiny.en.tar.bz2"
    DIR="sherpa-onnx-whisper-tiny.en"
elif [ "$STT_MODEL" == "base" ]; then
    URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-whisper-base.en.tar.bz2"
    DIR="sherpa-onnx-whisper-base.en"
elif [ "$STT_MODEL" == "small" ]; then
    URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-whisper-small.en.tar.bz2"
    DIR="sherpa-onnx-whisper-small.en"
else
    echo "Invalid STT model: $STT_MODEL. Choose tiny, base, or small."
    exit 1
fi

if [ ! -d "$DIR" ]; then
    curl -L "$URL" -o whisper.tar.bz2
    tar -xjf whisper.tar.bz2
    rm whisper.tar.bz2
    echo "  ✓ Whisper ($STT_MODEL) ASR model downloaded successfully."
else
    echo "  ✓ Whisper ($STT_MODEL) ASR model already exists. Skipping."
fi

echo "==> Downloading and extracting Piper TTS model (TTS: $TTS_MODEL)..."
if [ "$TTS_MODEL" == "medium" ]; then
    URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/vits-piper-en_US-lessac-medium.tar.bz2"
    DIR="vits-piper-en_US-lessac-medium"
elif [ "$TTS_MODEL" == "high" ]; then
    URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/vits-piper-en_US-libritts-high.tar.bz2"
    DIR="vits-piper-en_US-libritts-high"
else
    echo "Invalid TTS model: $TTS_MODEL. Choose medium or high."
    exit 1
fi

if [ ! -d "$DIR" ]; then
    curl -L "$URL" -o piper.tar.bz2
    tar -xjf piper.tar.bz2
    rm piper.tar.bz2
    echo "  ✓ Piper ($TTS_MODEL) TTS model downloaded successfully."
else
    echo "  ✓ Piper ($TTS_MODEL) TTS model already exists. Skipping."
fi

echo "==> Pre-trained models ready in: $MODELS_DIR"
