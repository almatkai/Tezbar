#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
HELPER_DIR="$ROOT_DIR/native/screenocr"
OUT_BIN="$HELPER_DIR/screenocr-helper"

swiftc "$HELPER_DIR/main.swift" -O -framework Cocoa -framework Vision -o "$OUT_BIN"
codesign --force --sign - "$OUT_BIN"

echo "Built ScreenOCR helper: $OUT_BIN"
