# Phase 0 Spike — Crash-During-Edit Sequence Diagram

**Status:** Design (not yet implemented). Phase 3 (Process Supervisor) implements this.

## Scenario

The user is actively typing in a Monaco editor. Mid-keystroke, `knot-server`
crashes (segfault, panic, or OOM kill). The supervisor must detect the crash,
restart the server, restore state, and resume editing with minimal disruption
and no data loss.

## Participants

- **User** — types in the editor
- **Monaco** (frontend) — captures keystrokes, sends `textDocument/didChange`
- **LanguageClient** (frontend) — queues outgoing LSP messages
- **TauriIpcWriter** (frontend) — `invoke('lsp_send')` to Rust backend
- **Rust Supervisor** — owns the `knot-server` subprocess, tracks open docs
- **knot-server** (subprocess) — crashes
- **Rust CrashCapture** — writes anonymized crash report, triggers restart

## Sequence

```
User          Monaco        LanguageClient   TauriIpcWriter   Rust Supervisor    knot-server
 |               |                 |                |                |                 |
 |--type 'a'--->|                 |                |                |                 |
 |               |--didChange----->|                |                |                 |
 |               |                 |--write(msg)--->|                |                 |
 |               |                 |                |--invoke-------->|                 |
 |               |                 |                |  ('lsp_send')  |                 |
 |               |                 |                |                |--write frame--->|
 |               |                 |                |                |                 |--CRASH!
 |               |                 |                |                |                 |
 |               |                 |                |                |<--child exit----|
 |               |                 |                |                |   (signal/exit) |
 |               |                 |                |                |                 |
 |               |                 |                |                | emit('lsp-exited', code)
 |               |                 |                |                |                 |
 |               |                 |                |                | [CrashCapture]  |
 |               |                 |                |                | write report    |
 |               |                 |                |                | to crash-reports/|
 |               |                 |                |                |                 |
 |               |                 |                |                | [State snapshot]|
 |               |                 |                |                | docs = open_docs|
 |               |                 |                |                | pending = queue |
 |               |                 |                |                |                 |
 |               |                 |                |                | [Backoff]       |
 |               |                 |                |                | sleep(2s)       |
 |               |                 |                |                |                 |
 |               |                 |                |                | [Respawn]       |
 |               |                 |                |                | spawn()-------->| (new process)
 |               |                 |                |                |                 |
 |               |                 |                |                | emit('lsp-started')
 |               |                 |                |                |                 |
 |               |                 |                |                | [State restore] |
 |               |                 |                |                | re-initialize-->|
 |               |                 |                |                | re-didOpen----->|  (for each open doc)
 |               |                 |                |                | replay pending->|  (queued didChange)
 |               |                 |                |                |                 |
 |               |                 |                |                |<--ready---------|
 |               |                 |                |                |                 |
 |               |                 |                |<--emit('lsp-message')-----------|
 |               |                 |<--dispatch-----|                |                 |
 |               |<--diagnostics---|                |                |                 |
 |<--redraw------|                 |                |                |                 |
 |               |                 |                |                |                 |
```

## Key design decisions

### 1. The frontend LanguageClient must NOT auto-disconnect on `lsp-exited`

`monaco-languageclient`'s default behavior on transport close is to mark the
client as "stopped" and stop accepting messages. For the supervisor model, we
need the client to stay alive across restarts so the user doesn't lose LSP
features for several seconds.

**Approach:** intercept the `onClose` event from `TauriIpcReader`. Instead of
letting it propagate to the LanguageClient, suppress it and wait for the
`lsp-started` event (which means the Rust backend has respawned the server).
Then re-send `initialize` + `didOpen` for all open documents.

This requires a **reconnection wrapper** around the transport that:
- Holds a queue of messages written during the disconnect window
- Flushes the queue when the new server is ready
- Re-initializes the LSP session on reconnect

### 2. State tracking in the Rust supervisor

The Rust backend must track:
- **Open documents** — URI + language id + current content (updated on each
  `didOpen` / `didChange` / `didClose`)
- **In-flight edits** — `didChange` messages that were sent but not yet
  acknowledged by the server (via `$/ping` response)

On restart, the supervisor replays:
1. `initialize` (with the same capabilities as before)
2. `initialized` notification
3. `didOpen` for each tracked open document (with the latest content)
4. Any in-flight `didChange` messages that weren't acknowledged

### 3. Crash report content (anonymized)

Written to `<appData>/crash-reports/<timestamp>.json`:

```json
{
  "timestamp": "2026-08-01T12:34:56Z",
  "knot_version": "0.1.0",
  "server_version": "2.0.0-preview",
  "os": "linux",
  "arch": "x86_64",
  "exit_code": -11,
  "signal": "SIGSEGV",
  "open_documents": 3,
  "last_stderr": [
    "INFO knot_server: Starting...",
    "ERROR panic at src/parser.rs:42"
  ],
  "uptime_seconds": 1234
}
```

**Wiped:** usernames, file paths (beyond project root basename), system
identifiers (hostname, MAC, etc.).

### 4. Backoff strategy

| Restart # | Delay |
|---|---|
| 1 | 2s |
| 2 | 4s |
| 3 | 8s |
| 4 | 16s |
| 5 | 16s (cap) |
| 6+ | Give up — show user dialog: "Server keeps crashing. [Report] [Restart] [Ignore]" |

### 5. Race condition: user types during restart window

The risk: user types 'a' → `didChange` sent → server already dead → message
lost. When the server restarts, it has the pre-'a' content.

**Mitigation:** the Rust supervisor queues ALL `lsp_send` calls that arrive
while the server is down. When the new server is `initialized`, the supervisor
flushes the queue in order. The frontend never sees the disconnect — it just
experiences a brief delay in diagnostics/completion.

This means `lsp_send` must be **async and buffered**, not fire-and-forget.
The Rust `lsp_send` command should:
1. If server is alive: write to stdin immediately
2. If server is dead/restarting: push to `pending_queue`
3. On restart completion: flush `pending_queue` to the new stdin

### 6. What the user sees

During a crash-restart cycle (typically 2-5 seconds):
- **Status bar:** "LSP: restarting…" (amber)
- **Editor:** continues to accept input normally (Monaco is local)
- **Diagnostics:** stale for a few seconds, then refresh
- **Completion:** may show "Loading…" briefly, then works

The user should never see an error dialog for a single crash — only for
repeated crashes (5+ in a row) that indicate a real bug.

## Open questions for Phase 3 implementation

1. Should `$/ping` be the health check, or should we use process-alive +
   a short timeout? LSP `$/ping` is cleaner but requires server cooperation.
2. How long to wait before declaring the server "hung" (not crashed)? 30s
   ping timeout is the LSP convention.
3. Should we snapshot open document content to disk (for crash recovery
   across app restarts), or only keep it in memory (for in-session restarts)?
   In-memory is simpler; cross-restart recovery is a separate feature.
