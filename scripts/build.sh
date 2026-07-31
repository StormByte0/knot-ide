#!/usr/bin/env bash
# Build the Rust LSP server only.
# The Tauri app build will be added once app/ is scaffolded (Phase 0/1).
set -euo pipefail

echo "=== Knot Server Build ==="
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_ROOT"

cargo build --release --manifest-path crates/server/Cargo.toml

echo ""
echo "=== Build complete ==="
echo "Server binary: target/release/knot-server"
