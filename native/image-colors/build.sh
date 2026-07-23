#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
HELPER_DIR="$ROOT_DIR/native/image-colors"
OUT_BIN="$HELPER_DIR/image-colors-helper"

swiftc "$HELPER_DIR/main.swift" -O -framework AppKit -o "$OUT_BIN"
codesign --force --sign - "$OUT_BIN"

echo "Built image colors helper: $OUT_BIN"
