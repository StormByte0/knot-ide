# Phase 0 — LSP Crash Supervisor

**Status:** Implemented. Completes the Phase 0 "crash-during-edit sequence spike" deliverable from PLAN.md §8.

## What it does

When `knot-server` crashes (segfault, panic, OOM kill) while the user is editing, the supervisor:

1. **Detects the crash** — the child-wait task sees the process exit
2. **Writes a crash report** — anonymized JSON to `<appData>/crash-reports/<timestamp>.json`
3. **Emits `lsp-exited`** — frontend shows "restarting…" status (amber) but does NOT close the LanguageClient
4. **Waits for backoff** — exponential: 2s → 4s → 8s → 16s (cap)
5. **Re-spawns the server** — new process, new stdin/stdout pipes
6. **Reinitializes transparently** — sends `initialize` + `initialized` + `didOpen` for each tracked document, using the stored params and latest content
7. **Emits `lsp-started`** — frontend resumes normal operation, diagnostics refresh

After 5 consecutive crashes, the supervisor gives up and emits `lsp-failed` — the frontend shows an error and the LanguageClient closes.

## Design: transparent reconnection

The key insight is that the frontend LanguageClient must NOT see the disconnect. If it did, it would transition to "stopped" and lose all LSP features for several seconds.

### How it works

```
User types 'a' → Monaco → didChange → LanguageClient → invoke('lsp_send')
                                                          ↓
Supervisor: track_lsp_state() updates doc content
             write to stdin (if alive) OR drop (if down — state still tracked)
                                                          ↓
Server CRASHES → child exit → handle_exit()
                   ├─ write crash report
                   ├─ emit('lsp-exited')  ← frontend shows "restarting…"
                   ├─ sleep(backoff)
                   ├─ spawn new server
                   ├─ reinitialize():
                   │    ├─ send initialize (id: "__supervisor_reinit__")
                   │    ├─ send initialized
                   │    └─ send didOpen for each tracked doc (latest content)
                   ├─ stdout reader drops the initialize response (by ID)
                   └─ emit('lsp-started')  ← frontend shows "ready"
                                                          ↓
New server sends diagnostics → stdout reader forwards → frontend LanguageClient
```

### The `SUPERVISOR_INIT_ID` trick

The supervisor sends `initialize` with a special request ID (`"__supervisor_reinit__"`). The stdout reader checks every outgoing message — if it's a response with that ID, it's dropped. This prevents the frontend LanguageClient from receiving a duplicate `initialize` response (which would cause a "duplicate response" error).

### State tracking

The supervisor parses every `lsp_send` payload to track LSP state:

| Method | Action |
|---|---|
| `initialize` | Store params (first time only) — replayed on restart |
| `textDocument/didOpen` | Store document (URI, language, content, version) |
| `textDocument/didChange` | Update content (apply range edits) + version |
| `textDocument/didClose` | Remove tracked document |

**Critical:** state tracking runs BEFORE the message is forwarded to stdin. This means even when the server is down (stdin is `None`), `didChange` messages still update the tracked content. When the server restarts, `reinitialize()` sends `didOpen` with the latest content — no data loss.

### Range edit application

The `apply_range_edit()` function applies LSP `Range` edits to tracked content:
- 0-based line/character positions (per LSP spec)
- Handles single-line and multi-line ranges
- Falls back to full document sync if no range is provided

## Crash report format

Written to `<appData>/crash-reports/crash-<timestamp>.json`:

```json
{
  "timestamp": "18646_22-34-56",
  "knot_version": "0.1.0",
  "exit_code": -11,
  "signal": "signal",
  "open_documents": 3,
  "uptime_seconds": 1234,
  "last_stderr": [
    "INFO knot_server: Starting...",
    "ERROR panic at src/parser.rs:42"
  ],
  "os": "windows",
  "arch": "x86_64"
}
```

- **Anonymized:** no usernames, no file paths (beyond project root basename), no system identifiers
- **Rotated:** only the 20 most recent crash reports are kept; older ones are deleted
- **Location:** `<appData>/crash-reports/` — on Windows this is `C:\Users\<user>\AppData\Roaming\dev.knot.ide\crash-reports\`

## Backoff schedule

| Restart # | Delay |
|---|---|
| 1 | 2s |
| 2 | 4s |
| 3 | 8s |
| 4 | 16s |
| 5 | 16s (cap) |
| 6+ | Give up — emit `lsp-failed` |

The restart count resets to 0 on successful reconnection (i.e., if the server stays alive long enough for the next crash to be a fresh incident).

## What the user sees

During a crash-restart cycle (typically 2-5 seconds):

- **Status bar:** "LSP: restarting…" (amber)
- **Editor:** continues to accept input normally (Monaco is local, unaffected)
- **Diagnostics:** stale for a few seconds, then refresh when the new server processes `didOpen`
- **Completion:** may show "Loading…" briefly, then works

The user should never see an error dialog for a single crash — only for 5+ consecutive crashes.

## What's NOT included (deferred to Phase 3)

- **Watchdog ping** — proactive `$/ping` every 10s with 30s timeout. Currently the supervisor only detects crashes via process exit, not hangs. A hung server (infinite loop, deadlock) won't be detected until the user notices diagnostics stopped updating.
- **Cursor position restore** — the supervisor tracks document content but not cursor position. After a restart, the cursor stays where Monaco has it (which is correct — Monaco is local), but any in-flight completion/hover requests are lost.
- **Pending message queue** — messages sent during downtime are dropped (state is still tracked). The original design proposed a queue that flushes after restart, but this would send stale `didChange` messages after `didOpen`, confusing the server. The current approach (drop + reinit with latest content) is cleaner.
- **Cross-restart recovery** — tracked state is in-memory only. If the Tauri app itself crashes, all state is lost. Cross-restart recovery (snapshot to disk) is a separate feature.
