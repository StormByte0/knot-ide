//! LSP subprocess supervisor for `knot-server`.
//!
//! Spawns `knot-server` (bundled as a Tauri sidecar) with `tokio::process`,
//! pipes stdin/stdout, and bridges LSP JSON-RPC messages between the frontend
//! and the subprocess via Tauri events.
//!
//! ## Crash-during-edit handling
//!
//! When `knot-server` crashes (segfault, panic, OOM kill), the supervisor:
//! 1. Captures the exit code + last N stderr lines → writes an anonymized
//!    crash report to `<appData>/crash-reports/<timestamp>.json`
//! 2. Emits `lsp-exited` (payload: exit code) — the frontend uses this to
//!    show a "restarting…" status but does NOT close the LanguageClient
//! 3. Waits for exponential backoff (2s → 4s → 8s → 16s cap)
//! 4. Re-spawns the server
//! 5. Re-sends `initialize` + `initialized` + `didOpen` for each tracked
//!    document (using the stored initialize params and document content)
//! 6. Emits `lsp-started` — the frontend resumes normal operation
//!
//! The `initialize` response from step 5 is intercepted and dropped by the
//! stdout reader (tagged with a supervisor-only request ID) so the frontend
//! LanguageClient never sees a duplicate response. This makes the
//! reconnection transparent — the user just experiences a brief delay in
//! diagnostics/completion.
//!
//! After 5 consecutive crashes, the supervisor gives up and emits `lsp-failed`
//! (payload: error message). The frontend then shows an error dialog.
//!
//! ## Why not `tauri-plugin-shell`?
//!
//! `tauri-plugin-shell` splits subprocess stdout on newlines by default
//! (plugins-workspace#1632). LSP JSON-RPC messages are framed by
//! `Content-Length:` headers with no trailing newline, so the shell plugin
//! silently drops/buffers data. This module uses `tokio::process` directly
//! with manual Content-Length frame parsing.
//!
//! ## Frame format
//!
//! ```text
//! Content-Length: <N>\r\n
//! \r\n
//! <N bytes of JSON-RPC>
//! ```

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

/// LSP frame header key.
const CONTENT_LENGTH: &str = "Content-Length";

/// Request ID used by the supervisor for its own `initialize` request during
/// reconnection. The stdout reader drops responses with this ID so the
/// frontend LanguageClient never sees a duplicate `initialize` response.
const SUPERVISOR_INIT_ID: &str = "__supervisor_reinit__";

/// Maximum consecutive restart attempts before giving up.
const MAX_RESTARTS: u32 = 5;

/// Maximum number of stderr lines to retain for crash reports.
const STDERR_BUFFER_SIZE: usize = 50;

/// State held by the Tauri app for the LSP supervisor.
pub struct LspSupervisor {
    /// The child process handle. `None` when not running.
    child: Arc<Mutex<Option<Child>>>,
    /// Stdin pipe for writing JSON-RPC messages to the server.
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    /// Stored `initialize` request params — captured from the frontend's
    /// first `initialize` request, replayed on reconnection.
    initialize_params: Arc<std::sync::Mutex<Option<serde_json::Value>>>,
    /// Open documents tracked from `didOpen` / `didChange` / `didClose`.
    /// Keyed by document URI. Used to replay `didOpen` after restart.
    open_documents: Arc<std::sync::Mutex<HashMap<String, TrackedDocument>>>,
    /// Ring buffer of recent stderr lines for crash reports.
    stderr_lines: Arc<std::sync::Mutex<VecDeque<String>>>,
    /// Consecutive restart count. Reset to 0 on successful reconnection.
    restart_count: Arc<std::sync::Mutex<u32>>,
    /// Server start time for uptime in crash reports.
    start_time: Arc<std::sync::Mutex<Option<std::time::Instant>>>,
}

/// A document tracked by the supervisor for state restore after crash.
#[derive(Clone)]
struct TrackedDocument {
    language_id: String,
    content: String,
    version: i32,
}

impl LspSupervisor {
    pub fn new() -> Self {
        Self {
            child: Arc::new(Mutex::new(None)),
            stdin: Arc::new(Mutex::new(None)),
            initialize_params: Arc::new(std::sync::Mutex::new(None)),
            open_documents: Arc::new(std::sync::Mutex::new(HashMap::new())),
            stderr_lines: Arc::new(std::sync::Mutex::new(VecDeque::with_capacity(STDERR_BUFFER_SIZE))),
            restart_count: Arc::new(std::sync::Mutex::new(0)),
            start_time: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Extract owned clones of all Arc fields into a `LspArcs` bundle.
    ///
    /// Used by callers that need to pass the Arcs into `spawn_server_impl`
    /// (which requires owned `Send` types, not `tauri::State<'_>` borrows).
    pub fn arcs(&self) -> LspArcs {
        LspArcs {
            child: self.child.clone(),
            stdin: self.stdin.clone(),
            initialize_params: self.initialize_params.clone(),
            open_documents: self.open_documents.clone(),
            stderr_lines: self.stderr_lines.clone(),
            restart_count: self.restart_count.clone(),
            start_time: self.start_time.clone(),
        }
    }
}

/// Resolve the path to the bundled `knot-server` sidecar binary.
fn resolve_sidecar_path() -> Result<std::path::PathBuf, String> {
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
            let sidecar_exe = parent.join(format!("knot-server-{target_triple}.exe"));
            if sidecar_exe.exists() {
                return Ok(sidecar_exe);
            }
        }
    }

    // Candidate 2: dev fallback — repo target dir.
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

    // Candidate 3: PATH lookup.
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

/// Owned bundle of all `Arc`s from `LspSupervisor`.
///
/// Passed to `spawn_server_impl` so it never needs to call `app.state()` —
/// keeping its future `Send`.
pub struct LspArcs {
    child: Arc<Mutex<Option<Child>>>,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    initialize_params: Arc<std::sync::Mutex<Option<serde_json::Value>>>,
    open_documents: Arc<std::sync::Mutex<HashMap<String, TrackedDocument>>>,
    stderr_lines: Arc<std::sync::Mutex<VecDeque<String>>>,
    restart_count: Arc<std::sync::Mutex<u32>>,
    start_time: Arc<std::sync::Mutex<Option<std::time::Instant>>>,
}

/// Internal spawn implementation — takes owned Arcs, never calls
/// `app.state()`.
///
/// Returns a **boxed** future (`Pin<Box<dyn Future + Send>>`) instead of
/// `impl Future` to break a type-level cycle: `spawn_server_impl` spawns
/// `handle_exit`, and `handle_exit` spawns `spawn_server_impl`. With opaque
/// `impl Future` types, the compiler can't compute either type because each
/// depends on the other. Boxing erases the concrete type at the call
/// boundary, so `handle_exit`'s generator state only references the concrete
/// `Pin<Box<dyn ...>>` type — breaking the cycle.
pub fn spawn_server_impl(app: AppHandle, arcs: LspArcs) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
    Box::pin(async move {
        let LspArcs {
            child: child_arc,
            stdin: stdin_arc,
        initialize_params: init_params_arc,
        open_documents: open_docs_arc,
        stderr_lines: stderr_lines_arc,
        restart_count: restart_count_arc,
        start_time: start_time_arc,
    } = arcs;

    // Guard: don't double-spawn.
    {
        let child_guard = child_arc.lock().await;
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
        let mut child_guard = child_arc.lock().await;
        *child_guard = Some(child);
    }
    {
        let mut stdin_guard = stdin_arc.lock().await;
        *stdin_guard = Some(stdin);
    }
    *start_time_arc.lock().unwrap() = Some(std::time::Instant::now());

    // Determine if this is a reconnection.
    let is_reconnection = {
        let count = restart_count_arc.lock().unwrap();
        *count > 0
    };

    // If reconnecting, replay initialize + didOpen before emitting lsp-started.
    if is_reconnection {
        reinitialize(&init_params_arc, &open_docs_arc, &stdin_arc).await;
        // Reset restart count on successful reconnection.
        *restart_count_arc.lock().unwrap() = 0;
    }

    // Start the stdout reader loop — parses frames, drops supervisor responses.
    let app_clone = app.clone();
    tokio::spawn(async move {
        if let Err(e) = stdout_reader_loop(stdout, app_clone).await {
            error!(error = %e, "stdout reader loop exited with error");
        }
    });

    // Start the stderr reader loop — logs + captures for crash reports.
    let stderr_arc = stderr_lines_arc.clone();
    tokio::spawn(async move {
        stderr_reader_loop(stderr, stderr_arc).await;
    });

    // Start the child-wait loop — handles crash on exit.
    // Clones the Arcs that handle_exit needs; the rest stay owned here.
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
                    break; // Child was taken (shutdown).
                }
            };

            if let Some(status) = exit_status {
                let code = status.code().unwrap_or(-1);
                warn!(exit_code = code, "knot-server exited");

                // Clear handles so a restart can spawn new ones.
                *child_arc.lock().await = None;
                *stdin_arc.lock().await = None;

                // Handle the crash: write report, backoff, restart.
                // Pass all Arcs so handle_exit can call spawn_server_impl
                // (which takes owned Arcs, keeping the future `Send`).
                handle_exit(
                    app_clone.clone(),
                    code,
                    LspArcs {
                        child: child_arc.clone(),
                        stdin: stdin_arc.clone(),
                        initialize_params: init_params_arc.clone(),
                        open_documents: open_docs_arc.clone(),
                        stderr_lines: stderr_lines_arc.clone(),
                        restart_count: restart_count_arc.clone(),
                        start_time: start_time_arc.clone(),
                    },
                )
                .await;
                break;
            }
        }
    });

    let _ = app.emit("lsp-started", ());
        Ok(())
    })
}

/// Handle a server exit: write crash report, backoff, restart.
///
/// After `MAX_RESTARTS` consecutive crashes, emits `lsp-failed` and gives up.
///
/// Takes owned Arcs (in `LspArcs`) so the future is `Send` — and calls
/// `spawn_server_impl` (not `spawn_server`) for the restart, which avoids
/// the `app.state()` call that would make the future non-`Send`.
async fn handle_exit(
    app: AppHandle,
    exit_code: i32,
    arcs: LspArcs,
) {
    // Snapshot crash info while holding locks briefly — borrow, don't move.
    let (stderr_snapshot, doc_count, uptime_secs) = {
        let stderr = arcs.stderr_lines.lock().unwrap();
        let stderr_vec: Vec<String> = stderr.iter().cloned().collect();
        let docs = arcs.open_documents.lock().unwrap();
        let doc_count = docs.len();
        let uptime = arcs.start_time.lock().unwrap().map(|t| t.elapsed().as_secs()).unwrap_or(0);
        (stderr_vec, doc_count, uptime)
    };

    // Write crash report to <appData>/crash-reports/<timestamp>.json.
    write_crash_report(&app, exit_code, &stderr_snapshot, doc_count, uptime_secs).await;

    // Emit lsp-exited — frontend shows "restarting…" but does NOT close the client.
    let _ = app.emit("lsp-exited", exit_code);

    // Check restart count.
    let count = {
        let mut c = arcs.restart_count.lock().unwrap();
        *c += 1;
        *c
    };

    if count > MAX_RESTARTS {
        error!(restart_count = count, "knot-server crashed too many times, giving up");
        let _ = app.emit("lsp-failed", format!(
            "knot-server crashed {count} times. Giving up. Check crash reports in the app data directory."
        ));
        return;
    }

    // Exponential backoff: 2s, 4s, 8s, 16s, 16s (cap).
    let delay_secs = backoff_seconds(count);
    info!(restart_count = count, delay_secs, "scheduling restart");
    tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;

    // Spawn the restart as a NEW task.
    //
    // `spawn_server_impl` returns a boxed `Pin<Box<dyn Future + Send>>`, so
    // awaiting it here doesn't create a type dependency on its concrete
    // generator state — breaking the opaque-type cycle between
    // `spawn_server_impl` and `handle_exit`.
    info!(restart_count = count, "restarting knot-server");
    let app_clone = app.clone();
    let restart = spawn_server_impl(app_clone.clone(), arcs);
    tokio::spawn(async move {
        if let Err(e) = restart.await {
            error!(error = %e, "failed to restart knot-server");
            let _ = app_clone.emit("lsp-failed", e);
        }
    });
}

/// Exponential backoff delay in seconds.
/// Restart 1 → 2s, 2 → 4s, 3 → 8s, 4 → 16s, 5 → 16s (cap).
fn backoff_seconds(restart_count: u32) -> u64 {
    let base = 2u64.pow(restart_count.min(4));
    base.min(16)
}

/// Write an anonymized crash report to `<appData>/crash-reports/<timestamp>.json`.
async fn write_crash_report(
    app: &AppHandle,
    exit_code: i32,
    stderr_lines: &[String],
    open_documents: usize,
    uptime_secs: u64,
) {
    let app_data = match app.path().app_data_dir() {
        Ok(dir) => dir,
        Err(e) => {
            warn!(error = %e, "failed to resolve app_data_dir for crash report");
            return;
        }
    };

    let crash_dir = app_data.join("crash-reports");
    if let Err(e) = tokio::fs::create_dir_all(&crash_dir).await {
        warn!(error = %e, "failed to create crash-reports directory");
        return;
    }

    // Timestamp for filename: YYYY-MM-DD_HH-MM-SS
    let timestamp = {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // Simple formatting without chrono — UTC timestamp.
        let secs = now % 60;
        let mins = (now / 60) % 60;
        let hours = (now / 3600) % 24;
        let days = now / 86400;
        // Approximate date from days since epoch (1970-01-01).
        // Good enough for crash report filenames.
        format!("{days}_{hours:02}-{mins:02}-{secs:02}")
    };

    let report_path = crash_dir.join(format!("crash-{timestamp}.json"));

    let report = serde_json::json!({
        "timestamp": timestamp,
        "knot_version": env!("CARGO_PKG_VERSION"),
        "exit_code": exit_code,
        "signal": if exit_code < 0 { "signal" } else { "normal" },
        "open_documents": open_documents,
        "uptime_seconds": uptime_secs,
        "last_stderr": stderr_lines,
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    });

    match serde_json::to_string_pretty(&report) {
        Ok(json) => {
            if let Err(e) = tokio::fs::write(&report_path, json).await {
                warn!(error = %e, "failed to write crash report");
            } else {
                info!(path = %report_path.display(), "crash report written");
            }
        }
        Err(e) => warn!(error = %e, "failed to serialize crash report"),
    }

    // Cap crash reports at 20 — rotate old ones.
    rotate_crash_reports(&crash_dir).await;
}

/// Keep only the 20 most recent crash reports.
async fn rotate_crash_reports(crash_dir: &std::path::Path) {
    let mut entries = match tokio::fs::read_dir(crash_dir).await {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut reports: Vec<(std::path::PathBuf, std::time::SystemTime)> = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        if let Ok(metadata) = entry.metadata().await {
            if let Ok(modified) = metadata.modified() {
                reports.push((entry.path(), modified));
            }
        }
    }

    if reports.len() <= 20 {
        return;
    }

    // Sort by modified time, oldest first.
    reports.sort_by_key(|(_, time)| *time);

    // Delete oldest until we have 20.
    for (path, _) in reports.iter().take(reports.len() - 20) {
        let _ = tokio::fs::remove_file(path).await;
    }
}

/// After a crash, re-send `initialize` + `initialized` + `didOpen` for each
/// tracked document. The `initialize` response is tagged with
/// `SUPERVISOR_INIT_ID` so the stdout reader can drop it.
///
/// Takes owned `Arc` references (not `app.state()`) so the function's future
/// is `Send` — safe to call from inside a `tokio::spawn` task.
async fn reinitialize(
    init_params: &Arc<std::sync::Mutex<Option<serde_json::Value>>>,
    open_documents: &Arc<std::sync::Mutex<HashMap<String, TrackedDocument>>>,
    stdin: &Arc<Mutex<Option<ChildStdin>>>,
) {
    let init_params = init_params.lock().unwrap().clone();
    let open_docs = open_documents.lock().unwrap().clone();

    let Some(params) = init_params else {
        warn!("cannot reinitialize — no stored initialize params");
        return;
    };

    // Build the initialize request with supervisor ID.
    let init_request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": SUPERVISOR_INIT_ID,
        "method": "initialize",
        "params": params
    });

    let initialized_notif = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "initialized",
        "params": {}
    });

    // Build didOpen for each tracked document.
    let did_opens: Vec<serde_json::Value> = open_docs
        .iter()
        .map(|(uri, doc)| {
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {
                    "textDocument": {
                        "uri": uri,
                        "languageId": doc.language_id,
                        "version": doc.version,
                        "text": doc.content
                    }
                }
            })
        })
        .collect();

    // Write all messages to stdin in order.
    let mut stdin_guard = stdin.lock().await;
    let Some(stdin) = stdin_guard.as_mut() else {
        warn!("cannot reinitialize — stdin not available");
        return;
    };

    info!("reinitializing LSP session (sending initialize + {} didOpen)", did_opens.len());

    if let Err(e) = write_jsonrpc(stdin, &init_request).await {
        error!(error = %e, "failed to send initialize during reinit");
        return;
    }
    if let Err(e) = write_jsonrpc(stdin, &initialized_notif).await {
        error!(error = %e, "failed to send initialized during reinit");
        return;
    }
    for did_open in &did_opens {
        if let Err(e) = write_jsonrpc(stdin, did_open).await {
            error!(error = %e, "failed to send didOpen during reinit");
        }
    }

    info!("reinitialize complete");
}

/// Write a JSON-RPC message as a Content-Length frame to stdin.
async fn write_jsonrpc(stdin: &mut ChildStdin, msg: &serde_json::Value) -> Result<(), String> {
    let body = serde_json::to_string(msg).map_err(|e| e.to_string())?;
    let frame = format!("{CONTENT_LENGTH}: {len}\r\n\r\n{body}", len = body.len());
    stdin.write_all(frame.as_bytes()).await.map_err(|e| e.to_string())?;
    stdin.flush().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Read `knot-server` stdout, parse LSP Content-Length frames, emit each body
/// as an `lsp-message` Tauri event — except responses to supervisor-initiated
/// requests (tagged with `SUPERVISOR_INIT_ID`), which are dropped.
async fn stdout_reader_loop(
    stdout: ChildStdout,
    app: AppHandle,
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
                info!("knot-server stdout closed (EOF)");
                return Ok(());
            }

            let line = line_buf.trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                break;
            }

            if let Some(rest) = line.strip_prefix(CONTENT_LENGTH) {
                let value = rest.trim_start_matches([':', ' ']).trim();
                match value.parse::<usize>() {
                    Ok(len) => content_length = Some(len),
                    Err(_) => warn!(header = %line, "invalid Content-Length value"),
                }
            }
        }

        let content_length = content_length
            .ok_or_else(|| "LSP frame ended without Content-Length header".to_string())?;

        let mut body = vec![0u8; content_length];
        reader.read_exact(&mut body).await?;

        let body_str = String::from_utf8_lossy(&body).into_owned();

        // Drop supervisor-initiated responses (e.g., the reinitialize
        // `initialize` response) so the frontend LanguageClient doesn't
        // receive a duplicate.
        if is_supervisor_response(&body_str) {
            info!("dropping supervisor-initiated response");
            continue;
        }

        let _ = app.emit("lsp-message", body_str);
    }
}

/// Check if a JSON-RPC message is a response to a supervisor-initiated request
/// (by matching the request ID against `SUPERVISOR_INIT_ID`).
fn is_supervisor_response(body: &str) -> bool {
    // Quick string check before full JSON parse — perf optimization.
    if !body.contains(SUPERVISOR_INIT_ID) {
        return false;
    }
    // Full parse to confirm it's a response (has "result" or "error") with
    // the supervisor ID.
    if let Ok(msg) = serde_json::from_str::<serde_json::Value>(body) {
        if msg.get("id").and_then(|v| v.as_str()) == Some(SUPERVISOR_INIT_ID) {
            return msg.get("result").is_some() || msg.get("error").is_some();
        }
    }
    false
}

/// Read `knot-server` stderr, log it, and retain the last N lines for crash
/// reports.
async fn stderr_reader_loop(
    stderr: tokio::process::ChildStderr,
    stderr_buffer: Arc<std::sync::Mutex<VecDeque<String>>>,
) {
    let mut reader = BufReader::new(stderr);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim_end_matches(['\r', '\n']);
                if !trimmed.is_empty() {
                    info!(target: "knot_server::stderr", "{}", trimmed);

                    // Push to ring buffer for crash reports.
                    let mut buf = stderr_buffer.lock().unwrap();
                    if buf.len() >= STDERR_BUFFER_SIZE {
                        buf.pop_front();
                    }
                    buf.push_back(trimmed.to_string());
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
/// Also parses the message to track LSP state (open documents, initialize
/// params) for transparent reconnection after a crash.
#[tauri::command]
pub async fn lsp_send(
    payload: String,
    state: tauri::State<'_, LspSupervisor>,
) -> Result<(), String> {
    // Parse and track state BEFORE forwarding — this ensures tracked content
    // is up-to-date even if the server is down (stdin is None).
    track_lsp_state(&payload, &state);

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
        // Server is down — the message is silently dropped.
        // State tracking above still captured any didOpen/didChange/didClose,
        // so when the server restarts, reinitialize() will have fresh content.
        Ok(())
    }
}

/// Parse a JSON-RPC payload and track LSP state for reconnection.
///
/// Tracks:
/// - `initialize` → stores the params for replay on restart
/// - `textDocument/didOpen` → stores the document (URI, language, content, version)
/// - `textDocument/didChange` → updates tracked content + version
/// - `textDocument/didClose` → removes the tracked document
fn track_lsp_state(payload: &str, state: &LspSupervisor) {
    let Ok(msg) = serde_json::from_str::<serde_json::Value>(payload) else {
        return;
    };

    let Some(method) = msg.get("method").and_then(|m| m.as_str()) else {
        return;
    };

    let Some(params) = msg.get("params") else {
        return;
    };

    match method {
        "initialize" => {
            let mut init = state.initialize_params.lock().unwrap();
            if init.is_none() {
                *init = Some(params.clone());
                info!("captured initialize params for reconnection");
            }
        }
        "textDocument/didOpen" => {
            if let Some(td) = params.get("textDocument") {
                if let (Some(uri), Some(lang), Some(text), Some(version)) = (
                    td.get("uri").and_then(|v| v.as_str()),
                    td.get("languageId").and_then(|v| v.as_str()),
                    td.get("text").and_then(|v| v.as_str()),
                    td.get("version").and_then(|v| v.as_i64()),
                ) {
                    let mut docs = state.open_documents.lock().unwrap();
                    docs.insert(
                        uri.to_string(),
                        TrackedDocument {
                            language_id: lang.to_string(),
                            content: text.to_string(),
                            version: version as i32,
                        },
                    );
                }
            }
        }
        "textDocument/didChange" => {
            if let Some(td) = params.get("textDocument") {
                if let (Some(uri), Some(version)) = (
                    td.get("uri").and_then(|v| v.as_str()),
                    td.get("version").and_then(|v| v.as_i64()),
                ) {
                    let mut docs = state.open_documents.lock().unwrap();
                    if let Some(doc) = docs.get_mut(uri) {
                        doc.version = version as i32;
                        // Apply text changes to tracked content.
                        if let Some(changes) = params.get("contentChanges").and_then(|c| c.as_array()) {
                            for change in changes {
                                if let (Some(range), Some(new_text)) = (
                                    change.get("range"),
                                    change.get("text").and_then(|t| t.as_str()),
                                ) {
                                    doc.content = apply_range_edit(&doc.content, range, new_text);
                                } else if let Some(new_text) = change.get("text").and_then(|t| t.as_str()) {
                                    // Full document sync (no range) — replace all.
                                    doc.content = new_text.to_string();
                                }
                            }
                        }
                    }
                }
            }
        }
        "textDocument/didClose" => {
            if let Some(td) = params.get("textDocument") {
                if let Some(uri) = td.get("uri").and_then(|v| v.as_str()) {
                    let mut docs = state.open_documents.lock().unwrap();
                    docs.remove(uri);
                }
            }
        }
        _ => {}
    }
}

/// Apply a single LSP `Range` edit to a string.
///
/// Returns the new string. Uses 0-based line/character positions per the LSP spec.
/// If the range is malformed, returns the original content unchanged.
fn apply_range_edit(content: &str, range: &serde_json::Value, new_text: &str) -> String {
    let Some(start) = range.get("start") else { return content.to_string(); };
    let Some(end) = range.get("end") else { return content.to_string(); };
    let Some(start_line) = start.get("line").and_then(|v| v.as_u64()) else { return content.to_string(); };
    let Some(start_char) = start.get("character").and_then(|v| v.as_u64()) else { return content.to_string(); };
    let Some(end_line) = end.get("line").and_then(|v| v.as_u64()) else { return content.to_string(); };
    let Some(end_char) = end.get("character").and_then(|v| v.as_u64()) else { return content.to_string(); };

    let lines: Vec<&str> = content.split('\n').collect();
    if start_line as usize >= lines.len() || end_line as usize >= lines.len() {
        return content.to_string();
    }

    let mut result = String::new();

    // Lines before the edit.
    for i in 0..start_line as usize {
        result.push_str(lines[i]);
        result.push('\n');
    }

    // The edited line: prefix of start line + new text + suffix of end line.
    let start_line_str = lines[start_line as usize];
    let end_line_str = lines[end_line as usize];
    let start_byte = std::cmp::min(start_char as usize, start_line_str.len());
    let end_byte = std::cmp::min(end_char as usize, end_line_str.len());

    result.push_str(&start_line_str[..start_byte]);
    result.push_str(new_text);
    result.push_str(&end_line_str[end_byte..]);

    // Lines after the edit.
    for i in (end_line as usize + 1)..lines.len() {
        result.push('\n');
        result.push_str(lines[i]);
    }

    result
}

/// Tauri command: explicitly start the LSP server (normally auto-started on
/// app launch, but exposed for manual restart).
#[tauri::command]
pub async fn lsp_start(
    app: AppHandle,
    state: tauri::State<'_, LspSupervisor>,
) -> Result<(), String> {
    let arcs = state.arcs();
    // `state` is dropped here (end of borrow) — `spawn_server_impl` returns
    // a boxed `Send` future, so this await is `Send`-safe.
    spawn_server_impl(app, arcs).await
}
