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
  import { startLanguageClient } from '$lib/lsp/client';
  import { statusStore } from '$lib/statusbar/statusStore.svelte';
  import { layoutStore } from '$lib/layout/layoutStore.svelte';
  import { editorSettingsStore } from '$lib/settings/editorSettings.svelte';
  import { migrateVscodeConfig } from '$lib/settings/projectSettings';
  import WindowHost from '$lib/windows/WindowHost.svelte';
  import { isChildWindow, closeAllChildWindows, broadcastCloseToChildren } from '$lib/windows/windowManager';

  // Detect child-window mode. If true, this component renders WindowHost
  // and the parent shell below is never mounted.
  const childMode = isChildWindow();

  // UI-local state. LSP status lives in statusStore; layout/tabs live in
  // layoutStore. This component only tracks the workspace folder + init error.
  let workspaceFolder = $state<string | null>(null);
  let monacoInitError = $state<string>('');
  let showSettings = $state(false);

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

    // Load editor settings (global, per-user) on startup.
    await editorSettingsStore.load();

    // When the parent window is closing, broadcast to all child windows
    // so they can close too (Tauri does NOT auto-close children).
    window.addEventListener('beforeunload', () => {
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
        console.log('[knot] save (not yet implemented)');
        break;
      case 'settings':
        showSettings = true;
        break;
      case 'find':
        console.log('[knot] find (not yet implemented)');
        break;
      case 'rename':
        if (layoutStore.getActiveEditorTab()) {
          window.dispatchEvent(new CustomEvent('knot-rename'));
        }
        break;
      case 'refresh':
        window.dispatchEvent(new CustomEvent('knot-refresh'));
        break;
      case 'build':
      case 'play':
      case 'watch-toggle':
        console.log('[knot] build action (not yet implemented):', action);
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

    // Initialize the layout with the default filebrowser | editor split.
    layoutStore.initDefaultLayout(selected);

    // Tell the Rust backend the workspace root (for path validation in fs_ops).
    await invoke('set_workspace_root', { path: selected });

    // Migrate .vscode/knot.json → .knot/config.json if needed.
    const migrated = await migrateVscodeConfig(selected);
    if (migrated) {
      console.log('[knot] migrated .vscode/knot.json → .knot/config.json');
    }

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
    background: #1e1e1e;
    color: #ccc;
    border-bottom: 1px solid #333;
    flex-shrink: 0;
  }

  .app-title {
    font-size: 13px;
    font-weight: 600;
  }

  .folder-name {
    font-size: 12px;
    color: #888;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .welcome {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    background: #1e1e1e;
  }

  .welcome-content {
    text-align: center;
    color: #cccccc;
  }

  .welcome-content h1 {
    font-size: 48px;
    margin-bottom: 8px;
    color: #4fc1ff;
  }

  .welcome-content p {
    font-size: 14px;
    color: #888;
    margin-bottom: 24px;
  }

  .open-folder-btn {
    background: #0e639c;
    color: white;
    border: none;
    padding: 10px 20px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 14px;
  }

  .open-folder-btn:hover {
    background: #1177bb;
  }

  .workspace {
    flex: 1;
    overflow: hidden;
  }

  .error {
    color: #f48771;
    margin-top: 16px;
  }
</style>
