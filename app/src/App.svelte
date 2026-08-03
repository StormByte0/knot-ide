<script lang="ts">
  /**
   * Knot app shell.
   *
   * **Mode detection:** if the URL has `?window=<label>`, this JS context is
   * a detached child window — render {@link WindowHost} and bail. Otherwise
   * this is the parent window: full app shell (toolbar, welcome screen,
   * layout tree, status bar).
   *
   * ## Parent flow
   *
   * 1. App starts with no workspace → welcome screen with "Open Folder"
   * 2. User picks a project folder → becomes workspace root
   * 3. LSP client starts with that folder as rootUri
   * 4. Layout store inits with default layout (filebrowser | editor split)
   * 5. LayoutRoot renders the layout tree recursively
   *
   * ## Context
   *
   * The `openFile` callback is provided via Svelte context so DockPanel →
   * FileBrowser can trigger editor-tab opens without prop-drilling through
   * the recursive layout tree.
   */

  import { onMount, setContext } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { readTextFile } from '@tauri-apps/plugin-fs';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import LayoutRoot from '$lib/layout/LayoutRoot.svelte';
  import StatusBar from '$lib/statusbar/StatusBar.svelte';
  import SettingsDialog from '$lib/settings/SettingsDialog.svelte';
  import { initializeMonaco } from '$lib/editor/monaco-init';
  import { startLanguageClient, stopLanguageClient } from '$lib/lsp/client';
  import { statusStore } from '$lib/statusbar/statusStore.svelte';
  import { layoutStore } from '$lib/layout/layoutStore.svelte';
  import { editorSettingsStore } from '$lib/settings/editorSettings.svelte';
  import { migrateVscodeConfig } from '$lib/settings/projectSettings';
  import { projectSettingsStore } from '$lib/settings/projectSettingsStore.svelte';
  import { themeStore } from '$lib/themes/themeStore.svelte';
  import WindowHost from '$lib/windows/WindowHost.svelte';
  import { isChildWindow, closeAllChildWindows, broadcastCloseToChildren } from '$lib/windows/windowManager';
  import type { LayoutNode } from '$lib/layout/types';

  // Detect child-window mode. If true, this component renders WindowHost
  // and the parent shell below is never mounted.
  const childMode = isChildWindow();

  // UI-local state. LSP status lives in statusStore; layout/tabs live in
  // layoutStore. This component only tracks the workspace folder + init error.
  let workspaceFolder = $state<string | null>(null);
  let monacoInitError = $state<string>('');
  let showSettings = $state(false);

  // Debounce timer for layout-state saves. The $effect below fires on every
  // structural layout change (tab open/close, panel drag, resize, expand/
  // collapse); we coalesce rapid changes into a single disk write. 500ms is
  // the same debounce VS Code uses for its own layout-state writes.
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  const SAVE_DEBOUNCE_MS = 500;

  // Provide the openFile callback via context. DockPanel picks this up and
  // passes it to FileBrowser.onSelect. The function is a closure over this
  // component's scope, so it captures `handleSelectFile` (hoisted) and reads
  // the latest `workspaceFolder` at call time.
  setContext('openFile', (path: string) => { handleSelectFile(path); });

  // Child-window mode: render WindowHost + bail. The rest of this script
  // (onMount, handlers, template) is parent-only and never runs in a child.
  // Svelte conditionally renders below.
  if (childMode) {
    // No-op — the template below renders <WindowHost /> when childMode is true.
  }

  /**
   * Track structural layout changes + schedule a debounced save.
   *
   * Reads only structural fields (panel ids, tab lists, sizes, active tab
   * ids, expandedPaths on filebrowser tabs). Does NOT read editor-tab
   * content/dirty fields — those change on every keystroke, and we don't
   * want to trigger a save (with content serialization) per keystroke.
   * Content is persisted on window close via `flushSave()`.
   *
   * The deep walk through `root` registers Svelte 5 reactivity on every
   * structural field. When any of them changes, this effect re-runs +
   * reschedules the debounced save.
   */
  $effect(() => {
    if (!workspaceFolder) return;
    const root = layoutStore.root;
    if (!root) return;
    // Touch structural fields to register reactivity. Excludes payload
    // content/dirty — those are editor-write hot paths we don't want to
    // trigger saves on. expandedPaths is included so file-browser expand/
    // collapse persists.
    touchStructure(root);
    scheduleSave(workspaceFolder);
  });

  /** Walk the layout tree, reading only structural fields. See $effect docs. */
  function touchStructure(node: LayoutNode): void {
    if (node.type === 'panel') {
      void node.id;
      void node.activeTabId;
      void node.tabs.length;
      for (const tab of node.tabs) {
        void tab.id;
        void tab.kind;
        void tab.title;
        // Read expandedPaths on filebrowser tabs so expand/collapse
        // triggers a save. Other payload fields (content, isDirty) are
        // intentionally NOT read here.
        if (tab.kind === 'filebrowser') {
          const payload = tab.payload as { expandedPaths?: string[] };
          void payload.expandedPaths?.length;
        }
      }
    } else {
      void node.direction;
      void node.sizes.length;
      for (let i = 0; i < node.sizes.length; i++) void node.sizes[i];
      void node.children.length;
      for (const child of node.children) touchStructure(child);
    }
  }

  /** Schedule a debounced save. Cancels any pending save + starts a new timer. */
  function scheduleSave(folder: string): void {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      saveTimer = null;
      void layoutStore.saveState(folder);
    }, SAVE_DEBOUNCE_MS);
  }

  /**
   * Flush any pending save immediately. Called on `beforeunload` so the
   * latest layout is written to disk before the window closes. Cancels the
   * debounce timer + writes synchronously (the save itself is async via
   * Tauri invoke; the browser keeps the page alive briefly for in-flight
   * fetches, and `sendBeacon`-style patterns aren't available for Tauri
   * invoke — fire-and-forget is the best we can do).
   */
  function flushSave(): void {
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = null;
    }
    if (workspaceFolder) {
      void layoutStore.saveState(workspaceFolder);
    }
  }

  onMount(async () => {
    // In child-window mode, App.svelte does nothing — WindowHost handles
    // its own onMount. This early return prevents the parent's LSP listeners
    // + menu handlers from running in a child context.
    if (childMode) return;

    // Initialize Monaco (registers the Twee language + grammar).
    try {
      await initializeMonaco();
      console.log('[knot] Monaco initialized');
    } catch (err) {
      console.error('[knot] Monaco init failed:', err);
      monacoInitError = `Monaco init failed: ${err instanceof Error ? err.message : String(err)}`;
      statusStore.setLspStatus('failed', monacoInitError);
    }

    // Listen for LSP lifecycle events from the Rust backend.
    await listen('lsp-started', () => {
      console.log('[knot] lsp-started event received');
      statusStore.setLspStatus('ready');
    });
    await listen<string>('lsp-start-failed', (event) => {
      console.error('[knot] lsp-start-failed:', event.payload);
      statusStore.setLspStatus('failed', event.payload);
    });
    await listen<number>('lsp-exited', (event) => {
      console.warn('[knot] lsp-exited: code', event.payload, '— supervisor will restart');
      statusStore.setLspStatus('restarting', `knot-server exited (code ${event.payload}). Restarting…`);
    });
    await listen<string>('lsp-failed', (event) => {
      console.error('[knot] lsp-failed:', event.payload);
      statusStore.setLspStatus('failed', event.payload);
    });

    // Listen for native menu bar actions.
    await listen<string>('menu-action', (event) => {
      handleMenuAction(event.payload);
    });

    // Register the global keydown handler for browser-intercepted shortcuts
    // (Ctrl+S, Ctrl+O, Ctrl+N, Ctrl+=/-/0, F5, etc.). See `handleKeydown`
    // docs for why the native menu accelerator alone isn't enough on Windows.
    // Uses capture phase so it fires BEFORE Monaco (which might stopPropagation
    // on some keys, preventing bubble-phase listeners from seeing them).
    window.addEventListener('keydown', handleKeydown, true);

    // Load editor settings (global, per-user) on startup.
    await editorSettingsStore.load();

    // Initialize the theme system (reads preference from editor settings,
    // registers Monaco themes, applies the active theme).
    themeStore.init();

    // Detect the Tweego version (if a path is configured) + push it to the
    // status bar. Fire-and-forget — a missing/failed version shouldn't block
    // app startup. The status bar shows "not configured" until this resolves.
    void refreshTweegoVersion();

    // When the parent window is closing, save the active editor tab (if dirty)
    // + flush any pending layout save + broadcast to all child windows so
    // they can close too (Tauri does NOT auto-close children). The saves are
    // fire-and-forget — the browser keeps the page alive briefly for the
    // in-flight Tauri invokes.
    window.addEventListener('beforeunload', () => {
      void layoutStore.saveActiveEditorTab();
      flushSave();
      broadcastCloseToChildren();
      closeAllChildWindows();
    });
  });

  /** Dispatch menu bar actions to the appropriate handler. */
  function handleMenuAction(action: string) {
    console.log('[knot] menu action:', action);
    switch (action) {
      case 'open-folder':
        handleOpenFolder();
        break;
      case 'new-file':
        if (workspaceFolder) {
          window.dispatchEvent(new CustomEvent('knot-new-file'));
        }
        break;
      case 'new-folder':
        if (workspaceFolder) {
          window.dispatchEvent(new CustomEvent('knot-new-folder'));
        }
        break;
      case 'save':
        handleSave();
        break;
      case 'settings':
        showSettings = true;
        break;
      case 'theme-dark':
        themeStore.setTheme('knot-dark');
        break;
      case 'theme-light':
        themeStore.setTheme('knot-light');
        break;
      case 'find':
        console.log('[knot] find (not yet implemented)');
        break;
      case 'rename':
        if (layoutStore.getActiveEditorTab() || statusStore.activeFile) {
          window.dispatchEvent(new CustomEvent('knot-rename'));
        }
        break;
      case 'refresh':
        window.dispatchEvent(new CustomEvent('knot-refresh'));
        break;
      case 'zoom-in':
        handleZoom(1);
        break;
      case 'zoom-out':
        handleZoom(-1);
        break;
      case 'reset-zoom':
        handleZoom(0);
        break;
      case 'documentation':
        // Open the docs in the user's default browser. Tauri's shell plugin
        // would be cleaner, but we don't have it configured — window.open
        // works in the webview + is forwarded to the OS browser by Tauri.
        window.open('https://github.com/StormByte0/knot-ide', '_blank');
        break;
      case 'toggle-file-browser':
        // TODO(Phase 8): toggle sidebar panel visibility. Needs a layout-model
        // change (hidden flag on panels) — deferred per PLAN.md §13.9.
        console.log('[knot] toggle-file-browser (deferred to Phase 8)');
        break;
      case 'build':
      case 'play':
      case 'watch-toggle':
        console.log('[knot] build action (not yet implemented):', action);
        break;
      case 'restart-lsp':
        handleRestartLsp();
        break;
      case 'check-updates':
        console.log('[knot] check for updates (not yet implemented)');
        break;
      case 'about':
        alert('Knot — Phase 1\nTwine/Twee IDE\nv0.1.0');
        break;
      default:
        console.log('[knot] unhandled menu action:', action);
    }
  }

  /**
   * Global keydown handler for keyboard shortcuts.
   *
   * ## Why this exists (PLAN.md §13.9 — unhandled menu actions follow-up)
   *
   * On Windows, WebView2 (Chromium) intercepts certain `Ctrl+key` combos —
   * `Ctrl+S` (Save Page), `Ctrl+O` (Open File), `Ctrl+N` (New Window),
   * `Ctrl+=/-/0` (browser zoom), `F5` (Reload) — BEFORE Tauri's native menu
   * accelerator can fire. The `on_menu_event` never reaches the frontend for
   * these combos, so the menu action is never dispatched.
   *
   * This handler catches those browser-intercepted combos, calls
   * `preventDefault()` to suppress the browser default (Save Page dialog,
   * Open File dialog, etc.), and dispatches to the same `handleMenuAction`
   * the menu uses.
   *
   * ## Double-fire safety
   *
   * For shortcuts the browser does NOT intercept (`Ctrl+Shift+E`, `F2`,
   * `Ctrl+,`, `Ctrl+Shift+B`, `Ctrl+Q`), Tauri's native menu accelerator
   * fires + consumes the key before the webview sees it — this handler never
   * runs for those. No double-fire.
   *
   * For browser-intercepted shortcuts, the native menu never fires — this
   * handler is the sole trigger. No double-fire.
   *
   * ## Excluded shortcuts
   *
   * - `Ctrl+F` (find): Monaco has its own find widget that's more useful than
   *   our stub. Letting Monaco handle it is the right call. Don't preventDefault.
   * - `Ctrl+Q` (quit): handled backend-side via `app.exit(0)`. The native
   *   accelerator works (not Chromium-intercepted). Don't handle in frontend.
   *
   * ## Modal guard
   *
   * When `showSettings` is true (Settings dialog open), skip all shortcuts.
   * The dialog handles its own Escape/Enter; other shortcuts should be inert
   * while a modal is up.
   */
  function handleKeydown(e: KeyboardEvent): void {
    // Skip when a modal dialog is open.
    if (showSettings) return;

    // Skip when focus is in an inline-edit input (file browser rename/new file).
    // The user is typing a filename — let them finish without shortcut interference.
    if (document.activeElement?.classList.contains('inline-edit')) return;

    const ctrl = e.ctrlKey || e.metaKey;

    // Non-Ctrl shortcuts (plain function keys).
    if (!ctrl) {
      if (e.key === 'F2') { e.preventDefault(); handleMenuAction('rename'); return; }
      if (e.key === 'F5') { e.preventDefault(); handleMenuAction('play'); return; }
      return;
    }

    // Ctrl shortcuts.
    const key = e.key.toLowerCase();
    if (e.shiftKey) {
      switch (key) {
        case 'n': e.preventDefault(); handleMenuAction('new-folder'); return;
        case 'e': e.preventDefault(); handleMenuAction('toggle-file-browser'); return;
        case 'b': e.preventDefault(); handleMenuAction('build'); return;
        case 'w': e.preventDefault(); handleMenuAction('watch-toggle'); return;
      }
      return;
    }
    switch (key) {
      case 's': e.preventDefault(); handleMenuAction('save'); return;
      case 'o': e.preventDefault(); handleMenuAction('open-folder'); return;
      case 'n': e.preventDefault(); handleMenuAction('new-file'); return;
      case ',': e.preventDefault(); handleMenuAction('settings'); return;
      case '=': e.preventDefault(); handleMenuAction('zoom-in'); return;
      case '-': e.preventDefault(); handleMenuAction('zoom-out'); return;
      case '0': e.preventDefault(); handleMenuAction('reset-zoom'); return;
      // Ctrl+F (find) intentionally NOT handled — Monaco's find widget is better.
      // Ctrl+Q (quit) intentionally NOT handled — backend owns it via native menu.
    }
  }

  /**
   * Adjust the editor font size. `direction > 0` zooms in, `< 0` zooms out,
   * `0` resets to default. Clamped to 8..32. Persists via editorSettingsStore
   * so the change survives app restart.
   */
  async function handleZoom(direction: number): Promise<void> {
    const DEFAULT_FONT_SIZE = 14;
    const MIN = 8;
    const MAX = 32;
    const newSize = direction === 0
      ? DEFAULT_FONT_SIZE
      : Math.max(MIN, Math.min(MAX, editorSettingsStore.settings.fontSize + direction));
    if (newSize !== editorSettingsStore.settings.fontSize) {
      await editorSettingsStore.update('fontSize', newSize);
    }
  }

  async function handleOpenFolder() {
    const selected = await open({
      multiple: false,
      directory: true,
    });
    if (typeof selected !== 'string') return;

    workspaceFolder = selected;
    statusStore.setProjectName(basename(selected));
    statusStore.setLspStatus('starting');
    console.log('[knot] workspace folder:', selected);

    // Tell the Rust backend the workspace root (for path validation in fs_ops).
    // Must happen BEFORE loading window state — the `load_window_state` command
    // validates the workspace root against the tracked root.
    await invoke('set_workspace_root', { path: selected });

    // Restore the saved window state (layout tree, open tabs, expanded
    // folders). Falls back to the default layout (filebrowser | editor
    // split) if no saved state exists or the saved state fails validation.
    await layoutStore.loadSavedState(selected);

    // Migrate .vscode/knot.json → .knot/config.json if needed.
    const migrated = await migrateVscodeConfig(selected);
    if (migrated) {
      console.log('[knot] migrated .vscode/knot.json → .knot/config.json');
    }

    // Load project settings (story format, build config) into the reactive
    // store. The Editor reads `storyFormat` from this store to set the
    // Monaco model language (SugarCube/Harlowe/Chapbook/Snowman grammar).
    await projectSettingsStore.load(selected);

    // Start the LSP client with the selected folder as workspace root.
    try {
      await startLanguageClient(selected);
      console.log('[knot] LanguageClient started');
      statusStore.setLspStatus('ready');
    } catch (err) {
      console.error('[knot] LanguageClient start failed:', err);
      statusStore.setLspStatus('failed', err instanceof Error ? err.message : String(err));
    }
  }

  /** Cross-platform basename: `D:\projects\my-game` → `my-game`. */
  function basename(path: string): string {
    const parts = path.split(/[/\\]/);
    return parts[parts.length - 1] || path;
  }

  async function handleSelectFile(path: string) {
    // Guard: don't try to read directories as files.
    if (path.endsWith('/.git') || path.endsWith('\\.git') || path.endsWith('/.git/') || path.endsWith('\\.git\\')) {
      console.warn('[knot] refusing to open .git as a file (it is a directory)');
      return;
    }
    try {
      const content = await readTextFile(path);
      layoutStore.openEditorTab(path, content);
      console.log('[knot] opened file:', path, 'length:', content.length);
    } catch (err) {
      console.error('[knot] file read failed:', err);
      statusStore.setLspStatus(statusStore.lspStatus, `Failed to read file: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  /**
   * Save the active editor tab. Triggered by `Ctrl+S` (menu action) or by
   * `beforeunload` (so pending edits aren't lost on window close).
   *
   * On failure, shows an alert so the user can retry or copy their work
   * elsewhere. The tab stays dirty — the close-dirty confirmation will still
   * fire if they try to close it.
   */
  async function handleSave(): Promise<void> {
    const ok = await layoutStore.saveActiveEditorTab();
    if (!ok) {
      alert('Failed to save file. Check the console for details. Your edits are still in the editor — do not close the tab.');
    }
  }

  /**
   * Refresh the Tweego version in the status bar. Runs `tweego --version` via
   * the Rust backend + pushes the result to `statusStore.setTweegoVersion`.
   * Called on startup (after settings load) + after the user clicks Detect
   * in the Settings dialog (which updates `tweegoPath`).
   *
   * Fire-and-forget — callers don't await this. A missing/failed version
   * leaves the status bar showing "not configured" (the store default).
   */
  async function refreshTweegoVersion(): Promise<void> {
    try {
      const version = await editorSettingsStore.detectTweegoVersion();
      statusStore.setTweegoVersion(version ?? 'not configured');
    } catch (err) {
      console.warn('[knot] tweego version detection failed:', err);
      statusStore.setTweegoVersion('not configured');
    }
  }

  /**
   * Restart the language server. Stops the frontend LanguageClient, asks the
   * Rust backend to re-spawn `knot-server`, then starts a fresh LanguageClient.
   * Useful when the server is stuck (hung, not crashing — crashes auto-restart
   * via the supervisor) or when the user wants to force a re-index.
   *
   * The status bar shows "restarting…" during the swap.
   */
  async function handleRestartLsp(): Promise<void> {
    if (!workspaceFolder) return;
    statusStore.setLspStatus('restarting', 'Manual restart in progress…');
    try {
      await stopLanguageClient();
      // Ask the backend to re-spawn the server subprocess.
      await invoke('lsp_start');
      await startLanguageClient(workspaceFolder);
      statusStore.setLspStatus('ready');
      console.log('[knot] LSP manually restarted');
    } catch (err) {
      console.error('[knot] LSP restart failed:', err);
      statusStore.setLspStatus('failed', err instanceof Error ? err.message : String(err));
    }
  }
</script>

{#if childMode}
  <!-- Child window: render WindowHost instead of the parent shell. -->
  <WindowHost />
{:else}
  <!-- Parent window: full app shell. -->
  <div class="app">
  <header class="toolbar">
    {#if workspaceFolder}
      <span class="folder-name">{workspaceFolder}</span>
    {:else}
      <span class="app-title">Knot</span>
    {/if}
  </header>

  {#if !workspaceFolder}
    <!-- Welcome screen -->
    <main class="welcome">
      <div class="welcome-content">
        <h1>Knot</h1>
        <p>Twine/Twee IDE</p>
        <button class="open-folder-btn" onclick={handleOpenFolder}>
          Open Project Folder…
        </button>
        {#if monacoInitError}
          <p class="error">{monacoInitError}</p>
        {/if}
      </div>
    </main>
  {:else}
    <!-- Workspace: layout tree -->
    <main class="workspace">
      {#if layoutStore.root}
        <LayoutRoot node={layoutStore.root} />
      {/if}
    </main>
  {/if}

  <StatusBar />
</div>
{/if}

{#if showSettings}
  <SettingsDialog
    workspaceFolder={workspaceFolder}
    onClose={() => (showSettings = false)}
  />
{/if}

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    width: 100vw;
    overflow: hidden;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 12px;
    background: var(--bg-toolbar);
    color: var(--fg-subtle);
    border-bottom: 1px solid var(--border-default);
    flex-shrink: 0;
  }

  .app-title {
    font-size: 13px;
    font-weight: 600;
  }

  .folder-name {
    font-size: 12px;
    color: var(--fg-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .welcome {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-app);
  }

  .welcome-content {
    text-align: center;
    color: var(--fg-default);
  }

  .welcome-content h1 {
    font-size: 48px;
    margin-bottom: 8px;
    color: var(--accent);
  }

  .welcome-content p {
    font-size: 14px;
    color: var(--fg-muted);
    margin-bottom: 24px;
  }

  .open-folder-btn {
    background: var(--accent);
    color: var(--fg-status-bar);
    border: none;
    padding: 10px 20px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 14px;
  }

  .open-folder-btn:hover {
    background: var(--accent-hover);
  }

  .workspace {
    flex: 1;
    overflow: hidden;
  }

  .error {
    color: var(--danger);
    margin-top: 16px;
  }
</style>
