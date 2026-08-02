<script lang="ts">
  /**
   * Window host — root component for detached child windows.
   *
   * Rendered by {@link App.svelte} when the JS context detects it's running
   * in a child window (`?window=<label>` query param). Owns its own
   * `LayoutRoot` + `StatusBar`, and receives the detached tab via the
   * `tab-detached` event.
   *
   * ## Lifecycle
   *
   * 1. On mount, listen for `tab-detached` (the parent emits it after
   *    creating the window).
   * 2. On receipt, initialize the layout store with a default single-panel
   *    layout + add the detached tab.
   * 3. Also listen for `close-child-windows` (parent broadcasts this when
   *    closing) — close this window on receipt.
   * 4. Monaco `initialize()` runs once per window (acceptable per PLAN §6.2,
   *    ~50-100ms + ~30MB per detached editor).
   *
   * ## State isolation
   *
   * Each child window has its own `layoutStore` instance (Svelte 5 module
   * singletons are per-JS-context). The parent's layout is unaffected. The
   * LSP client + supervisor are shared (the Rust backend owns the single
   * `knot-server` subprocess — child windows just send LSP messages through
   * the same `lsp_send` command).
   */

  import { onMount, onDestroy, setContext } from 'svelte';
  import LayoutRoot from '$lib/layout/LayoutRoot.svelte';
  import StatusBar from '$lib/statusbar/StatusBar.svelte';
  import { initializeMonaco } from '$lib/editor/monaco-init';
  import { statusStore } from '$lib/statusbar/statusStore.svelte';
  import { layoutStore } from '$lib/layout/layoutStore.svelte';
  import type { TabData } from '$lib/layout/types';
  import {
    onTabDetached,
    onCloseChildWindows,
    deserializeTab,
    getChildWindowLabel,
    signalWindowReady,
    type SerializedTab,
  } from './windowManager';
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';

  let ready = $state(false);
  let error = $state<string | null>(null);
  let unlistenTabDetached: (() => void) | null = null;
  let unlistenCloseChildren: (() => void) | null = null;

  // No file browser in child windows — the child is a thin view, not a full
  // app shell. The openFile context is a no-op (DockPanel won't render a
  // filebrowser tab anyway since the layout is a single editor panel).
  setContext('openFile', (_path: string) => {
    console.warn('[knot:child] openFile called in child window — no file browser available');
  });

  onMount(async () => {
    const label = getChildWindowLabel();
    if (!label) {
      error = 'Child window started without a ?window=<label> query param.';
      return;
    }

    // Initialize Monaco (runs once per window context).
    try {
      await initializeMonaco();
      console.log('[knot:child] Monaco initialized for window', label);
    } catch (err) {
      console.error('[knot:child] Monaco init failed:', err);
      error = `Monaco init failed: ${err instanceof Error ? err.message : String(err)}`;
      return;
    }

    // Listen for the detached tab from the parent.
    // IMPORTANT: register the listener BEFORE signaling readiness. The parent
    // is awaiting `window-ready` before emitting `tab-detached` — if we signal
    // first, there's no race; if we signal after, the parent might emit before
    // this listener is attached.
    unlistenTabDetached = await onTabDetached((payload) => {
      const { tab: serializedTab, workspaceFolder } = payload;
      const tab = deserializeTab(serializedTab);
      // Initialize the layout with a single editor panel containing the tab.
      initChildLayout(workspaceFolder, tab);
      ready = true;
      console.log('[knot:child] received tab:', tab.id, 'for window', label);
    });

    // Signal readiness to the parent. The parent's `detachTab` is awaiting
    // this before emitting `tab-detached` (avoids the race where the event
    // is emitted before this listener is registered).
    await signalWindowReady(label);

    // Listen for parent-initiated close.
    unlistenCloseChildren = await onCloseChildWindows(async () => {
      console.log('[knot:child] received close request from parent');
      try {
        const win = await getCurrentWebviewWindow();
        await win.close();
      } catch (err) {
        console.warn('[knot:child] error closing window:', err);
      }
    });
  });

  onDestroy(() => {
    unlistenTabDetached?.();
    unlistenCloseChildren?.();
  });

  // Auto-close: if the child window's last tab closes, the window is empty
  // and useless (no file browser to open new tabs). Close it automatically.
  $effect(() => {
    if (!ready) return;
    const root = layoutStore.root;
    if (!root) return;
    // Check if the root panel has no tabs left.
    if (root.type === 'panel' && root.tabs.length === 0) {
      console.log('[knot:child] last tab closed — auto-closing child window');
      try {
        const win = getCurrentWebviewWindow();
        void win.close();
      } catch (err) {
        console.warn('[knot:child] error auto-closing window:', err);
      }
    }
  });

  /**
   * Initialize the child window's layout with a SINGLE panel containing just
   * the detached tab. No file browser, no split — the child is a thin view
   * window, not a full app shell.
   */
  function initChildLayout(workspaceFolder: string, tab: TabData): void {
    // Set the project name in the status store (for the status bar).
    statusStore.setProjectName(basename(workspaceFolder));
    // Single-panel layout — just the detached tab. No file browser sidebar.
    layoutStore.root = {
      type: 'panel',
      id: 'editor',
      tabs: [tab],
      activeTabId: tab.id,
    };
  }

  /** Cross-platform basename. */
  function basename(path: string): string {
    const parts = path.split(/[/\\]/);
    return parts[parts.length - 1] || path;
  }
</script>

<div class="child-window">
  {#if error}
    <div class="child-error">
      <p>{error}</p>
    </div>
  {:else if !ready}
    <div class="child-loading">
      <p>Loading…</p>
    </div>
  {:else if layoutStore.root}
    <main class="child-workspace">
      <LayoutRoot node={layoutStore.root} />
    </main>
    <StatusBar />
  {/if}
</div>

<style>
  .child-window {
    display: flex;
    flex-direction: column;
    height: 100vh;
    width: 100vw;
    overflow: hidden;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
    background: #1e1e1e;
  }

  .child-workspace {
    flex: 1;
    overflow: hidden;
  }

  .child-loading,
  .child-error {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: #888;
    font-size: 14px;
  }

  .child-error {
    color: #f48771;
  }
</style>
