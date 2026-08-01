# Knot App — Phase 0 Spike

Minimal Tauri 2 + Svelte 5 + Monaco app that validates the core architecture
before building the full IDE. See `PLAN.md` §8 (Phase 0) and
`docs/phase0-crash-during-edit.md`.

## What the spike validates

1. **Monaco + Vite production build** — `vite build` succeeds with Monaco
   workers, TextMate grammar, and `@codingame/monaco-vscode-api`. ✅ validated
2. **LSP transport over Tauri IPC** — `knot-server` spawned as subprocess,
   JSON-RPC bridged via `invoke` + `event`.
3. **TextMate grammar** — minimal Twee/SugarCube grammar renders syntax
   highlighting in Monaco.
4. **Crash-during-edit sequence** — design in `docs/phase0-crash-during-edit.md`.

## Prerequisites

### Windows (primary dev machine)

Tauri 2 on Windows uses **WebView2** (preinstalled on Windows 10/11) and the
**MSVC toolchain** — no WebKitGTK or GTK deps.

1. **Microsoft C++ Build Tools** — install via
   [Visual Studio Build Tools 2022](https://visualstudio.microsoft.com/visual-cpp-build-tools/).
   Select the "Desktop development with C++" workload (includes MSVC, Windows
   SDK, and the linker Tauri needs).

2. **WebView2 Runtime** — preinstalled on Windows 10 1809+ and all Windows 11.
   If missing: [download from Microsoft](https://developer.microsoft.com/microsoft-edge/webview2/).

3. **Rust (MSVC target)** — `rustup` should default to
   `stable-x86_64-pc-windows-msvc`. Verify:
   ```powershell
   rustc -vV
   # host: x86_64-pc-windows-msvc
   ```

### Linux (for reference / CI)

```bash
sudo apt install libwebkit2gtk-4.1-dev build-essential \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libgtk-3-dev
```

### macOS (for reference)

```bash
# Xcode Command Line Tools (provides everything Tauri needs)
xcode-select --install
```

## Build the knot-server sidecar

The Tauri app bundles `knot-server` as a sidecar. You must build it for your
platform and copy it to `app/src-tauri/binaries/` with the target-triple
suffix that Tauri expects.

### Windows (PowerShell)

```powershell
# From repo root
cargo build --release --manifest-path crates/server/Cargo.toml

# Copy with the target-triple suffix Tauri expects
$target = rustc -vV | Select-String "host:" | ForEach-Object { $_.ToString().Split(' ')[1] }
Copy-Item "target\release\knot-server.exe" "app\src-tauri\binaries\knot-server-$target.exe"
```

The resulting file should be named exactly:
`app\src-tauri\binaries\knot-server-x86_64-pc-windows-msvc.exe`

### Linux / macOS (for reference)

```bash
cargo build --release --manifest-path crates/server/Cargo.toml
TARGET=$(rustc -vV | grep host | awk '{print $2}')
cp target/release/knot-server app/src-tauri/binaries/knot-server-$TARGET
```

> **Why the suffix?** Tauri's `bundle.externalBin` config names the binary
> without a suffix; at bundle time it appends `-<target-triple>` (and `.exe`
> on Windows) to pick the right binary per platform. The sidecar resolver in
> `src-tauri/src/lsp.rs` mirrors this convention for dev mode.

## Running

```powershell
cd app
npm install

# Dev mode (hot reload)
npm run tauri:dev

# Production build (validates the critical Vite+Monaco build path)
npm run tauri:build
```

`tauri:dev` opens a window with a Monaco editor showing a sample Twee file.
The status bar should show "LSP: ready" once `knot-server` connects. Try
typing in a passage — you should get SugarCube completion from the server.

## Troubleshooting

### "knot-server sidecar binary not found"

The resolver in `lsp.rs` checks three locations in order:
1. Next to the app exe: `knot-server-<target-triple>[.exe]`
2. Repo dev path: `../../target/release/knot-server[.exe]`
3. System PATH

For dev mode, build the sidecar (above) or rely on location #2 if you ran
`cargo build --release` from the repo root.

### "failed to spawn knot-server" on Windows

- Verify the binary exists at `app\src-tauri\binaries\knot-server-x86_64-pc-windows-msvc.exe`
- Check Windows Defender / antivirus didn't quarantine the exe (Rust binaries
  from `cargo build` sometimes trigger SmartScreen)
- Run from a terminal with normal user privileges — don't run as admin unless
  the repo is in an admin-only location

### LSP shows "failed" or "exited" immediately

Check the terminal output — the Rust backend logs `knot-server` stderr. A
common cause is a version mismatch between the sidecar and the server's
expected LSP capabilities. The spike uses the server's current `main` branch;
if you've modified the server, rebuild the sidecar.

### Monaco renders but no syntax highlighting

The TextMate grammar needs the oniguruma WASM to load. Check the browser
devtools console (right-click → Inspect in the Tauri window) for WASM errors.
The CSP in `tauri.conf.json` includes `'wasm-unsafe-eval'` for this.

## Architecture

```
app/
├── src/                          # Svelte 5 frontend
│   ├── main.ts                   # entry — mounts App.svelte
│   ├── App.svelte                # toolbar + editor + status bar
│   ├── app.css                   # global styles
│   ├── vite-env.d.ts             # Vite client types (?worker imports)
│   └── lib/
│       ├── editor/
│       │   ├── workers.ts        # MonacoEnvironment.getWorker (?worker imports)
│       │   ├── monaco-init.ts    # monaco-vscode-api initialize() + grammar
│       │   └── Editor.svelte     # Monaco editor Svelte component
│       └── lsp/
│           ├── transport.ts      # TauriIpcReader/Writer (vscode-jsonrpc)
│           └── client.ts         # MonacoLanguageClient setup
├── src-tauri/                    # Tauri 2 Rust backend
│   ├── Cargo.toml
│   ├── tauri.conf.json           # CSP, window, sidecar config
│   ├── capabilities/main.json    # permissions for the main window
│   ├── binaries/                 # sidecar binaries (target-triple-suffixed)
│   └── src/
│       ├── main.rs               # entry (windows_subsystem = "windows")
│       ├── lib.rs                # Tauri builder + plugin registration
│       └── lsp.rs                # LSP supervisor (tokio::process + framing)
├── docs/
│   └── phase0-crash-during-edit.md
├── package.json
├── vite.config.ts
├── svelte.config.js
├── tsconfig.json
└── index.html
```

## Key design points

### LSP transport (the novel part)

No off-the-shelf Tauri+LSP template exists. The transport is a custom
`MessageReader` / `MessageWriter` pair wrapping Tauri IPC:

- **`TauriIpcWriter`** — `write(msg)` → `invoke('lsp_send', { payload })` →
  Rust wraps in Content-Length frame → writes to `knot-server` stdin
- **`TauriIpcReader`** — listens to `lsp-message` Tauri events (emitted by
  Rust backend reading `knot-server` stdout) → dispatches to LanguageClient

### Why NOT `tauri-plugin-shell`

`tauri-plugin-shell` splits subprocess stdout on newlines by default
(plugins-workspace#1632). LSP JSON-RPC messages are framed by Content-Length
headers with no trailing newline, so the shell plugin silently drops data.
The supervisor uses `tokio::process::Command` directly with manual frame
parsing.

### CSP

```
default-src 'self';
script-src 'self' 'wasm-unsafe-eval';   # oniguruma WASM for TextMate
worker-src 'self' blob:;                  # Monaco workers
connect-src ipc: http://ipc.localhost;    # Tauri IPC
```

### Monaco worker setup

Manual `?worker` imports + `MonacoEnvironment.getWorker` map. Do NOT use
`vite-plugin-monaco-editor` — it conflicts with `@codingame/monaco-vscode-api`.

The TextMate worker ships in
`@codingame/monaco-vscode-textmate-service-override/worker`, NOT in base
`monaco-editor`.

## What's NOT in the spike

- Multi-window (Phase 1)
- Story Map v2 (Phase 4)
- Asset manager (Phase 5)
- Process supervisor with crash capture/restart (Phase 3 — design only)
- Full TextMate grammars (Phase 2 — minimal grammar only)
- Auto-updater (Phase 9)
