//! LSP subprocess supervisor for `knot-server`.
//!
//! Spawns `knot-server` (bundled as a Tauri sidecar) with `tokio::process`,
//! pipes stdin/stdout, and bridges LSP JSON-RPC messages between the frontend
//! and the subprocess via Tauri events.
//!
//! ## Why not `tauri-plugin-shell`?
//!
//! `tauri-plugin-shell` splits subprocess stdout on newlines by default
//! (plugins-workspace#1632). LSP JSON-RPC messages are framed by
//! `Content-Length:` headers with no trailing newline, so the shell plugin
//! silently drops/buffers data. This module uses `tokio::process` directly
//! with manual Content-Length frame parsing.
//!
//! ## Message flow
//!
//! ```text
//! Frontend (Monaco)                Rust backend                  knot-server
//!      |                                |                             |
//!      | invoke('lsp_send', payload)    |                             |
//!      |------------------------------- >|                             |
//!      |                                | write Content-Length frame  |
//!      |                                |---------------------------- >|
//!      |                                |                             |
//!      |                                |<----------------------------|
//!      |                                | read Content-Length frame   |
//!      |<-------------------------------| emit('lsp-message', body)   |
//!      | listen('lsp-message')          |                             |
//!      |                                |                             |
//! ```
//!
//! ## Frame format
//!
//! ```text
//! Content-Length: <N>\r\n
//! \r\n
//! <N bytes of JSON-RPC>
//! ```

use std::sync::Arc;
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

/// LSP frame header key.
const CONTENT_LENGTH: &str = "Content-Length";

/// State held by the Tauri app for the LSP supervisor.
pub struct LspSupervisor {
    /// The child process handle. `None` when not running.
    child: Arc<Mutex<Option<Child>>>,
    /// Stdin pipe for writing JSON-RPC messages to the server.
    stdin: Arc<Mutex<Option<ChildStdin>>>,
}

impl LspSupervisor {
    pub fn new() -> Self {
        Self {
            child: Arc::new(Mutex::new(None)),
            stdin: Arc::new(Mutex::new(None)),
        }
    }
}

/// Resolve the path to the bundled `knot-server` sidecar binary.
///
/// In dev mode, this looks for the binary in `app/src-tauri/binaries/`.
/// In production (tauri build), Tauri resolves the sidecar via the
/// `externalBin` config and the target-triple suffix.
fn resolve_sidecar_path() -> Result<std::path::PathBuf, String> {
    // Tauri's sidecar resolution: the binary is named `<name>-<target-triple>`
    // and is placed next to the app executable at bundle time.
    // In dev, we fall back to the repo's `target/release/knot-server`.
    let target_triple = std::env::consts::ARCH.to_string()
        + "-"
        + match std::env::consts::OS {
            "linux" => "unknown-linux-gnu",
            "macos" => "apple-darwin",
            "windows" => "pc-windows-msvc",
            other => return Err(format!("unsupported OS: {other}")),
        };

    // Candidate 1: Tauri sidecar location (relative to the current exe).
    if let Ok(exe_dir) = std::env::current_exe() {
        if let Some(parent) = exe_dir.parent() {
            let sidecar = parent.join(format!("knot-server-{target_triple}"));
            if sidecar.exists() {
                return Ok(sidecar);
            }
            // Windows .exe
            let sidecar_exe = parent.join(format!("knot-server-{target_triple}.exe"));
            if sidecar_exe.exists() {
                return Ok(sidecar_exe);
            }
        }
    }

    // Candidate 2: dev fallback — repo target dir (relative to src-tauri).
    let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/release/knot-server");
    if dev_path.exists() {
        return Ok(dev_path);
    }
    let dev_path_exe = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/release/knot-server.exe");
    if dev_path_exe.exists() {
        return Ok(dev_path_exe);
    }

    // Candidate 3: PATH lookup (if user has knot-server installed).
    if let Ok(path) = which::which("knot-server") {
        return Ok(path);
    }

    Err(format!(
        "knot-server sidecar binary not found. Looked for:\n  \
         - <exe_dir>/knot-server-{target_triple}\n  \
         - <exe_dir>/knot-server-{target_triple}.exe\n  \
         - {}\n  \
         - knot-server on PATH\n  \
         Build it with: cargo build --release --manifest-path crates/server/Cargo.toml",
        dev_path.display()
    ))
}

/// Spawn the `knot-server` subprocess and start the stdout reader loop.
///
/// Emits `lsp-message` events (payload: `String` JSON-RPC body) to all windows
/// as frames arrive. Emits `lsp-exited` events (payload: `i32` exit code, or
/// `-1` for signal death) when the subprocess dies.
pub async fn spawn_server(app: tauri::AppHandle, state: tauri::State<'_, LspSupervisor>) -> Result<(), String> {
    // Guard: don't double-spawn.
    {
        let child_guard = state.child.lock().await;
        if child_guard.is_some() {
            return Ok(());
        }
    }

    let sidecar_path = resolve_sidecar_path()?;
    info!(path = %sidecar_path.display(), "spawning knot-server");

    let mut child = tokio::process::Command::new(&sidecar_path)
        .arg("--stdio")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("failed to spawn knot-server: {e}"))?;

    let stdin = child.stdin.take().ok_or("failed to capture stdin")?;
    let stdout = child.stdout.take().ok_or("failed to capture stdout")?;
    let stderr = child.stderr.take().ok_or("failed to capture stderr")?;

    // Store the handles.
    {
        let mut child_guard = state.child.lock().await;
        *child_guard = Some(child);
    }
    {
        let mut stdin_guard = state.stdin.lock().await;
        *stdin_guard = Some(stdin);
    }

    // Start the stdout reader loop — parses Content-Length frames and emits.
    let app_clone = app.clone();
    tokio::spawn(async move {
        if let Err(e) = stdout_reader_loop(stdout, app_clone).await {
            error!(error = %e, "stdout reader loop exited with error");
        }
    });

    // Start the stderr reader loop — logs and captures for crash reports.
    tokio::spawn(async move {
        stderr_reader_loop(stderr).await;
    });

    // Start the child-wait loop — emits `lsp-exited` when the process dies.
    let child_arc = state.child.clone();
    let app_clone = app.clone();
    tokio::spawn(async move {
        loop {
            let exit_status = {
                let mut child_guard = child_arc.lock().await;
                if let Some(child) = child_guard.as_mut() {
                    match child.wait().await {
                        Ok(status) => Some(status),
                        Err(e) => {
                            error!(error = %e, "failed to wait on child");
                            None
                        }
                    }
                } else {
                    // Child was taken (shutdown). Exit the loop.
                    break;
                }
            };

            if let Some(status) = exit_status {
                let code = status.code().unwrap_or(-1);
                warn!(exit_code = code, "knot-server exited");
                let _ = app_clone.emit("lsp-exited", code);

                // Clear the child handle so a restart can spawn a new one.
                let mut child_guard = child_arc.lock().await;
                *child_guard = None;
                break;
            }
        }
    });

    let _ = app.emit("lsp-started", ());
    Ok(())
}

/// Read `knot-server` stdout, parse LSP Content-Length frames, emit each body
/// as an `lsp-message` Tauri event.
///
/// LSP frame format:
/// ```text
/// Content-Length: <N>\r\n
/// \r\n
/// <N bytes of JSON-RPC>
/// ```
///
/// Headers are read one line at a time until a blank line (`\r\n`), then
/// exactly `Content-Length` bytes of body are read.
async fn stdout_reader_loop(
    stdout: ChildStdout,
    app: tauri::AppHandle,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut reader = BufReader::new(stdout);
    let mut line_buf = String::new();

    loop {
        let mut content_length: Option<usize> = None;

        // Read headers until blank line.
        loop {
            line_buf.clear();
            let n = reader.read_line(&mut line_buf).await?;
            if n == 0 {
                // EOF — server closed stdout.
                info!("knot-server stdout closed (EOF)");
                return Ok(());
            }

            let line = line_buf.trim_end_matches(['\r', '\n']);

            // Empty line signals end of headers.
            if line.is_empty() {
                break;
            }

            // Parse Content-Length header. The line looks like:
            // `Content-Length: 1863` — strip the prefix and the colon separator.
            if let Some(rest) = line.strip_prefix(CONTENT_LENGTH) {
                // rest is `: 1863` — strip leading colon and whitespace.
                let value = rest.trim_start_matches([':', ' ']).trim();
                match value.parse::<usize>() {
                    Ok(len) => content_length = Some(len),
                    Err(_) => {
                        warn!(header = %line, "invalid Content-Length value");
                    }
                }
            }
            // Other headers (Content-Type, etc.) are ignored for framing.
        }

        let content_length = content_length.ok_or_else(|| {
            "LSP frame ended without Content-Length header".to_string()
        })?;

        // Read exactly `content_length` bytes of body.
        let mut body = vec![0u8; content_length];
        reader.read_exact(&mut body).await?;

        let body_str = String::from_utf8_lossy(&body).into_owned();
        let _ = app.emit("lsp-message", body_str);
    }
}

/// Read `knot-server` stderr, log it for crash reports.
async fn stderr_reader_loop(stderr: tokio::process::ChildStderr) {
    let mut reader = BufReader::new(stderr);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break, // EOF
            Ok(_) => {
                let trimmed = line.trim_end_matches(['\r', '\n']);
                if !trimmed.is_empty() {
                    info!(target: "knot_server::stderr", "{}", trimmed);
                }
            }
            Err(e) => {
                warn!(error = %e, "error reading knot-server stderr");
                break;
            }
        }
    }
}

/// Tauri command: send a JSON-RPC message to `knot-server` via stdin.
///
/// The frontend calls this with the serialized JSON-RPC payload; this function
/// wraps it in a Content-Length frame and writes it to the subprocess stdin.
#[tauri::command]
pub async fn lsp_send(
    payload: String,
    state: tauri::State<'_, LspSupervisor>,
) -> Result<(), String> {
    let frame = format!(
        "{CONTENT_LENGTH}: {len}\r\n\r\n{body}",
        len = payload.len(),
        body = payload
    );

    let mut stdin_guard = state.stdin.lock().await;
    if let Some(stdin) = stdin_guard.as_mut() {
        stdin.write_all(frame.as_bytes()).await.map_err(|e| e.to_string())?;
        stdin.flush().await.map_err(|e| e.to_string())?;
        Ok(())
    } else {
        Err("knot-server stdin not available (server not running)".into())
    }
}

/// Tauri command: explicitly start the LSP server (normally auto-started on
/// app launch, but exposed for manual restart after crash).
#[tauri::command]
pub async fn lsp_start(
    app: tauri::AppHandle,
    state: tauri::State<'_, LspSupervisor>,
) -> Result<(), String> {
    spawn_server(app, state).await
}
