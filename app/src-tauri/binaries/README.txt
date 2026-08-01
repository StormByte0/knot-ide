# Sidecar binaries live here.
#
# Build the server and copy it with the target-triple suffix:
#   cargo build --release --manifest-path crates/server/Cargo.toml
#   $target = rustc -vV | Select-String "host:" | % { $_.ToString().Split(' ')[1] }
#   Copy-Item "target\release\knot-server.exe" "app\src-tauri\binaries\knot-server-$target.exe"
#
# The binary files themselves are gitignored (they're build artifacts).
