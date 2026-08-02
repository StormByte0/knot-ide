/**
 * Multi-window manager — parent-side child window lifecycle.
 *
 * Creates, tracks, and closes child OS windows for detached panels. Each
 * child window loads the same `index.html` with `?window=<label>` so the
 * frontend can detect child-window mode and render {@link WindowHost.svelte}
 * instead of the full app shell.
 *
 * ## Architecture (PLAN.md §6.1 — parent/child deck model)
 *
 * - **Parent window** (`?window` absent): full app shell — toolbar, welcome
 *   screen, layout tree, status bar. Owns the window registry.
 * - **Child window** (`?window=<label>`): `WindowHost` — a `LayoutRoot` +
 *   `StatusBar` only. Receives a detached tab via the `tab-detached` event.
 *
 * ## Tab detach flow
 *
 * 1. Parent calls {@link detachTab} with the source tab id.
 * 2. `detachTab` looks up the tab + its panel in `layoutStore`, serializes
 *    the tab to a plain object, creates a `WebviewWindow` with a unique
 *    label, and emits `tab-detached` (targeted at the new window's label)
 *    with the serialized tab + the workspace folder.
 * 3. Parent removes the tab from its layout store (`layoutStore.forceCloseTab`
 *    — skip the dirty check since the tab is moving, not closing).
 * 4. Child `WindowHost` listens for `tab-detached` on mount, receives the
 *    tab, initializes its layout store with a default layout + adds the tab.
 *
 * ## Window closing
 *
 * - Closing a child window: just `WebviewWindow.close()`. The window's
 *   `onDestroyed` callback removes it from the registry.
 * - Closing the parent: iterate the registry + close all children first.
 *   (Tauri does NOT auto-close child windows when the parent closes — we
 *   must do it explicitly. Wired in {@link App.svelte}'s `onMount` cleanup.)
 *
 * ## Why frontend-created windows (not Rust commands)
 *
 * Tauri 2's `WebviewWindow` constructor is available from `@tauri-apps/api`
 * and the capabilities already allow `core:webview:allow-create-webview-window`.
 * Using it avoids a Rust command round-trip + keeps the window logic in one
 * place (the frontend). The Rust backend's role is event relay (emit/emitTo),
 * which it already does.
 */

import { WebviewWindow, getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { emit, emitTo, listen } from '@tauri-apps/api/event';
import { layoutStore } from '$lib/layout/layoutStore.svelte';
import type { TabData } from '$lib/layout/types';

/** Registry of open child window labels. Parent-side only. */
const childWindowLabels = new Set<string>();

/** Counter for generating unique child window labels. */
let labelCounter = 0;

/**
 * Detach a tab into a new child window.
 *
 * Looks up the tab in the parent's layout store, creates a new OS window,
 * emits the tab to it, and removes the tab from the parent's layout.
 *
 * No-op if the tab is not found or the window can't be created.
 */
export async function detachTab(tabId: string): Promise<void> {
  // Find the tab + its panel.
  const sourcePanel = layoutStore.findPanelByTabId(tabId);
  if (!sourcePanel) {
    console.warn('[knot:windows] detachTab: tab not found:', tabId);
    return;
  }
  const tab = sourcePanel.tabs.find((t) => t.id === tabId);
  if (!tab) {
    console.warn('[knot:windows] detachTab: tab not found in panel:', tabId);
    return;
  }

  // Don't detach the last filebrowser tab — keep at least one in the parent.
  if (tab.kind === 'filebrowser') {
    console.warn('[knot:windows] detachTab: refusing to detach filebrowser tab (keep at least one in parent)');
    return;
  }

  // Generate a unique label.
  const label = `child-${Date.now()}-${labelCounter++}`;
  const title = `Knot — ${tab.title}`;

  // Get the workspace folder from the layout (find a filebrowser tab).
  const workspaceFolder = findWorkspaceFolder();
  if (!workspaceFolder) {
    console.warn('[knot:windows] detachTab: no workspace folder found, cannot create child window');
    return;
  }

  try {
    // Create the child window. It loads index.html?window=<label>.
    // Tauri resolves relative URLs against the dev server (dev) or the
    // frontend dist (production).
    // Thin window — the child is a single-view window (one tab, no sidebar),
    // not a full app shell. Width is compact; user can resize if needed.
    const webview = new WebviewWindow(label, {
      url: `index.html?window=${encodeURIComponent(label)}`,
      title,
      width: 750,
      height: 600,
      minWidth: 300,
      minHeight: 200,
      resizable: true,
      // Match the parent's dragDropEnabled setting (disabled for HTML5 DnD).
      dragDropEnabled: false,
    });

    // Track in the registry.
    childWindowLabels.add(label);

    // Clean up on close. Listen for the Tauri close-requested / destroyed event.
    webview.once('tauri://destroyed', () => {
      childWindowLabels.delete(label);
      console.log('[knot:windows] child window closed:', label);
    });

    // Wait for the child window's JS to signal readiness before emitting the
    // tab. `tauri://created` fires when the native window exists, but the
    // webview's JS context (WindowHost.onMount) takes longer to start — if we
    // emit `tab-detached` now, no listener exists yet and the event is lost.
    // The child emits `window-ready` after registering its `tab-detached`
    // listener. We await it with a 10s timeout fallback (so the parent
    // doesn't hang forever if the child fails to start).
    await waitForChildReady(label);

    // Emit the tab to the child window. Now the listener is registered.
    await emitTo(label, 'tab-detached', {
      tab: serializeTab(tab),
      workspaceFolder,
    });

    // Remove the tab from the parent's layout (force — skip dirty check,
    // the tab is moving, not closing).
    layoutStore.forceCloseTab(tabId);
  } catch (err) {
    console.error('[knot:windows] detachTab: failed to create child window:', err);
    childWindowLabels.delete(label);
  }
}

/** Close all child windows. Called when the parent is closing. */
export async function closeAllChildWindows(): Promise<void> {
  const labels = Array.from(childWindowLabels);
  for (const label of labels) {
    try {
      const win = await WebviewWindow.getByLabel(label);
      await win?.close();
    } catch (err) {
      console.warn('[knot:windows] error closing child window', label, err);
    }
  }
  childWindowLabels.clear();
}

/** Get the current window's label (from the URL query param). */
export function getChildWindowLabel(): string | null {
  const params = new URLSearchParams(window.location.search);
  const label = params.get('window');
  return label;
}

/** Whether this JS context is running in a child window. */
export function isChildWindow(): boolean {
  return getChildWindowLabel() !== null;
}

/** Focus a child window by label (bring to front). */
export async function focusChildWindow(label: string): Promise<void> {
  try {
    const win = await WebviewWindow.getByLabel(label);
    await win?.setFocus();
  } catch (err) {
    console.warn('[knot:windows] error focusing child window', label, err);
  }
}

/**
 * Wait for a child window to signal readiness (parent-side).
 *
 * Listens for the `window-ready` event emitted by the child's
 * {@link WindowHost.svelte} after it registers its `tab-detached` listener.
 * Resolves when the event is received, or after a 10s timeout (so the parent
 * doesn't hang forever if the child fails to start).
 *
 * @param childLabel The child window's label (to filter the event).
 */
async function waitForChildReady(childLabel: string): Promise<void> {
  return new Promise<void>((resolve) => {
    let resolved = false;
    const timeout = setTimeout(() => {
      if (!resolved) {
        resolved = true;
        console.warn('[knot:windows] timed out waiting for child ready:', childLabel);
        unlisten?.then((fn) => fn()).catch(() => {});
        resolve();
      }
    }, 10_000);

    let unlisten: Promise<() => void> | null = listen('window-ready', (event) => {
      if (event.payload === childLabel && !resolved) {
        resolved = true;
        clearTimeout(timeout);
        unlisten?.then((fn) => fn()).catch(() => {});
        console.log('[knot:windows] child ready:', childLabel);
        resolve();
      }
    });
  });
}

/**
 * Signal readiness to the parent (child-side).
 *
 * Called by {@link WindowHost.svelte} after registering its `tab-detached`
 * listener. The parent's {@link detachTab} is awaiting this signal before
 * emitting the tab.
 *
 * @param label This child window's label.
 */
export async function signalWindowReady(label: string): Promise<void> {
  await emit('window-ready', label);
}

/**
 * Listen for `tab-detached` events (child-side). Returns an unlisten function.
 *
 * The child WindowHost calls this on mount to receive the tab from the parent.
 */
export async function onTabDetached(
  callback: (payload: { tab: SerializedTab; workspaceFolder: string }) => void,
): Promise<() => void> {
  return listen<{ tab: SerializedTab; workspaceFolder: string }>('tab-detached', (event) => {
    callback(event.payload);
  });
}

/**
 * Listen for window close requests (child-side). The parent emits
 * `close-child-windows` when the parent is closing — children should exit.
 */
export async function onCloseChildWindows(
  callback: () => void,
): Promise<() => void> {
  return listen('close-child-windows', () => {
    callback();
  });
}

/**
 * Broadcast a close request to all child windows (parent-side). Called when
 * the parent is closing.
 */
export async function broadcastCloseToChildren(): Promise<void> {
  await emit('close-child-windows', {});
}

// ── Serialization ──────────────────────────────────────────────────────

/**
 * Serialized tab — a plain (non-proxy) copy of a {@link TabData}. Svelte 5's
 * `$state` proxy can't be sent across the IPC boundary, so we deep-copy the
 * tab + its payload.
 */
export interface SerializedTab {
  id: string;
  kind: TabData['kind'];
  title: string;
  payload: unknown;
}

/** Deep-copy a tab into a plain object for IPC. */
function serializeTab(tab: TabData): SerializedTab {
  // JSON round-trip strips Svelte 5 proxies + deep-copies the payload.
  return JSON.parse(JSON.stringify(tab));
}

/** Deserialize a tab back into a TabData (the type is identical). */
export function deserializeTab(tab: SerializedTab): TabData {
  return tab as TabData;
}

// ── Helpers ────────────────────────────────────────────────────────────

/** Find the workspace folder by looking for a filebrowser tab in the layout. */
function findWorkspaceFolder(): string | null {
  if (!layoutStore.root) return null;
  return findFolderInNode(layoutStore.root);
}

/** Recursively search for a filebrowser tab + extract its folder. */
function findFolderInNode(node: import('$lib/layout/types').LayoutNode): string | null {
  if (node.type === 'panel') {
    for (const tab of node.tabs) {
      if (tab.kind === 'filebrowser') {
        const payload = tab.payload as { folder: string };
        return payload.folder;
      }
    }
    return null;
  }
  for (const child of node.children) {
    const folder = findFolderInNode(child);
    if (folder) return folder;
  }
  return null;
}
