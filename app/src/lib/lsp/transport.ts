/**
 * LSP transport over Tauri IPC.
 *
 * Bridges `vscode-jsonrpc`'s `MessageReader` / `MessageWriter` interfaces to
 * Tauri's `invoke` (request/response) and `event` (notifications/streams)
 * primitives. The Rust backend owns the actual `knot-server` subprocess and
 * pipes stdin/stdout.
 *
 * ## Message flow
 *
 * - **Outgoing** (frontend → server): `TauriIpcWriter.write(msg)` calls
 *   `invoke('lsp_send', { payload: JSON.stringify(msg) })`. The Rust backend
 *   wraps it in a Content-Length frame and writes to `knot-server` stdin.
 *
 * - **Incoming** (server → frontend): the Rust backend reads `knot-server`
 *   stdout, parses Content-Length frames, and emits each body as an
 *   `lsp-message` Tauri event. `TauriIpcReader` listens for these events and
 *   dispatches to the `DataCallback`.
 */

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  AbstractMessageReader,
  AbstractMessageWriter,
  type DataCallback,
  type Message,
  Disposable,
} from 'vscode-jsonrpc';

/** Tauri event name carrying an LSP message body (JSON-RPC string). */
const LSP_MESSAGE_EVENT = 'lsp-message';

/** Tauri event name signaling the server process exited (will auto-restart). */
const LSP_EXITED_EVENT = 'lsp-exited';

/** Tauri event name signaling the supervisor gave up (max crashes reached). */
const LSP_FAILED_EVENT = 'lsp-failed';

/**
 * `MessageReader` backed by Tauri events.
 *
 * Extends `AbstractMessageReader` which provides `onError`, `onClose`, and
 * `onPartialMessage` event wiring. We only implement `listen()` and use the
 * protected `fireError` / `fireClose` helpers.
 */
export class TauriIpcReader extends AbstractMessageReader {
  private callback: DataCallback | null = null;
  private unlisten: UnlistenFn | null = null;
  private exitUnlisten: UnlistenFn | null = null;
  private disposed = false;

  listen(callback: DataCallback): Disposable {
    this.callback = callback;

    // Register the Tauri event listener. `listen` is async but we must return
    // a Disposable synchronously, so we handle the async registration
    // internally and capture the unlisten function when it resolves.
    listen<string>(LSP_MESSAGE_EVENT, (event) => {
      if (this.disposed || !this.callback) return;
      try {
        const message = JSON.parse(event.payload) as Message;
        console.log('[knot:lsp] ← server:', (message as { method?: string }).method ?? 'response', 'id:', (message as { id?: unknown }).id ?? '-');
        this.callback(message);
      } catch (err) {
        this.fireError(err instanceof Error ? err : new Error(String(err)));
      }
    })
      .then((unlisten) => {
        this.unlisten = unlisten;
      })
      .catch((err) => {
        this.fireError(err instanceof Error ? err : new Error(String(err)));
      });

    // On `lsp-exited`: do NOT fire close — the supervisor will auto-restart.
    // The LanguageClient stays alive across the restart; messages sent during
    // the downtime are silently dropped by the backend (state is still tracked).
    // On `lsp-failed`: the supervisor gave up (max crashes reached) — fire
    // close so the LanguageClient transitions to "stopped".
    listen<string>(LSP_FAILED_EVENT, () => {
      console.warn('[knot:lsp] supervisor gave up — firing close');
      this.fireClose();
    })
      .then((unlisten) => {
        this.exitUnlisten = unlisten;
      })
      .catch(() => {
        // Non-fatal — the close handler is a best-effort signal.
      });

    return Disposable.create(() => this.dispose());
  }

  override dispose(): void {
    this.disposed = true;
    this.callback = null;
    this.unlisten?.();
    this.unlisten = null;
    this.exitUnlisten?.();
    this.exitUnlisten = null;
    super.dispose();
  }
}

/**
 * `MessageWriter` backed by Tauri `invoke`.
 *
 * Extends `AbstractMessageWriter` which provides `onError` and `onClose`
 * event wiring. We implement `write()` and `end()`.
 */
export class TauriIpcWriter extends AbstractMessageWriter {
  private disposed = false;

  async write(msg: Message): Promise<void> {
    if (this.disposed) {
      throw new Error('TauriIpcWriter is disposed');
    }
    const payload = JSON.stringify(msg);
    console.log('[knot:lsp] → server:', (msg as { method?: string }).method ?? 'response', 'id:', (msg as { id?: unknown }).id ?? '-', 'len:', payload.length);
    try {
      await invoke('lsp_send', { payload });
    } catch (err) {
      const error = err instanceof Error ? err : new Error(String(err));
      console.error('[knot:lsp] invoke lsp_send failed:', error);
      this.fireError(error, msg, undefined);
      throw error;
    }
  }

  end(): void {
    // No-op — the underlying Tauri IPC doesn't have a "half-close" concept.
    // The Rust backend manages the subprocess lifecycle.
  }

  override dispose(): void {
    this.disposed = true;
    super.dispose();
  }
}

/**
 * Create a `MessageTransports` pair for `MonacoLanguageClient`.
 */
export function createTauriTransports() {
  return {
    reader: new TauriIpcReader(),
    writer: new TauriIpcWriter(),
  };
}
