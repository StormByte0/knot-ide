/**
 * Knot LSP client — wires `monaco-languageclient` to `knot-server` via the
 * Tauri IPC transport.
 */

import { MonacoLanguageClient, type MonacoLanguageClientOptions } from 'monaco-languageclient';
import type { LanguageClientOptions } from 'vscode-languageclient';
import * as vscode from 'vscode';
import { createTauriTransports } from './transport';
import { TWEE_LANGUAGE_ID } from '$lib/editor/monaco-init';

let client: MonacoLanguageClient | null = null;

/**
 * Start the Knot language client with the given workspace root.
 *
 * @param rootPath - The project folder path (e.g. `D:\projects\my-game`)
 *
 * This does three things:
 * 1. Converts the root path to a `file://` URI and passes it as `workspaceFolder`
 *    so the server gets a proper `rootUri` in `initialize`
 * 2. Starts the LanguageClient
 * 3. Sends the custom `knot/clientReady` request after start — the server
 *    waits for this before indexing (eliminates a notification race)
 */
export async function startLanguageClient(rootPath: string): Promise<MonacoLanguageClient> {
  if (client) return client;

  // Convert to a file:// URI with forward slashes.
  // On Windows: `D:\path` → `file:///D:/path`
  // On Unix: `/path` → `file:///path`
  const normalized = rootPath.replace(/\\/g, '/');
  const rootUriStr = `file://${normalized.startsWith('/') ? '' : '/'}${normalized}`;
  const rootUri = vscode.Uri.parse(rootUriStr);

  console.log('[knot:lsp] workspace root:', rootUriStr);

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ language: TWEE_LANGUAGE_ID, scheme: 'file' }],
    workspaceFolder: {
      uri: rootUri,
      name: rootPath.split(/[/\\]/).pop() || 'workspace',
      index: 0,
    },
    markdown: { isTrusted: true },
  };

  const options: MonacoLanguageClientOptions = {
    id: 'knot',
    name: 'Knot Language Client',
    clientOptions,
    messageTransports: createTauriTransports(),
  };

  client = new MonacoLanguageClient(options);

  await client.start();
  console.log('[knot:lsp] LanguageClient started');
  // Don't await knot/clientReady — it can hang if the LanguageClient doesn't
  // recognize the custom method. Fire and forget; the server handles it.
  client.sendRequest('knot/clientReady', {}).then(
    () => console.log('[knot:lsp] knot/clientReady acknowledged'),
    (err) => console.warn('[knot:lsp] knot/clientReady failed (non-fatal):', err),
  );

  return client;
}

/**
 * Stop the language client (e.g. before app shutdown or server restart).
 */
export async function stopLanguageClient(): Promise<void> {
  if (!client) return;
  await client.stop();
  client = null;
}

/**
 * Get the current language client, or `null` if not started.
 */
export function getLanguageClient(): MonacoLanguageClient | null {
  return client;
}
