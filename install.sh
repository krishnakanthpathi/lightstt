#!/usr/bin/env bash
set -e

echo "=== Building LightSTT (lstt) in release mode ==="
cargo build --release

DEST_DIR="$HOME/.local/bin"
mkdir -p "$DEST_DIR"

cp target/release/lstt "$DEST_DIR/lstt"
chmod +x "$DEST_DIR/lstt"

echo "=== Installed lstt to $DEST_DIR/lstt ==="
"$DEST_DIR/lstt" doctor
