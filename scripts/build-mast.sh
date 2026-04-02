#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
FFI_DIR="$ROOT_DIR/crates/mast-ffi"
SOMA_DIR="$ROOT_DIR/apps/Soma"
CMAST_DIR="$SOMA_DIR/MastFFI/Sources/CMast"

echo "Building mast-ffi (release, aarch64-apple-darwin)..."
cargo build -p mast-ffi --release --target aarch64-apple-darwin

echo "Copying static library..."
mkdir -p "$CMAST_DIR/lib"
cp "$ROOT_DIR/target/aarch64-apple-darwin/release/libmast_ffi.a" "$CMAST_DIR/lib/"

echo "Copying header..."
mkdir -p "$CMAST_DIR/include"
cp "$FFI_DIR/mast.h" "$CMAST_DIR/include/"

echo "Done. Static lib and header are in $CMAST_DIR"
