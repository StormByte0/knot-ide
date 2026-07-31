#!/usr/bin/env bash
# Debug build of the Rust LSP server with watch mode.
# The Tauri dev loop (svelte dev + tauri dev) will be added once app/ is scaffolded.
set -euo pipefail

echo "=== Knot Server Dev (watch) ==="
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_ROOT"

cargo watch -x 'build --manifest-path crates/server/Cargo.toml' \
  || cargo watch -x "build --manifest-path crates/server/Cargo.toml" \
  || { echo "cargo-watch not installed. Install with: cargo install cargo-watch"; \
       echo "Falling back to one-shot debug build..."; \
       cargo build --manifest-path crates/server/Cargo.toml; }
