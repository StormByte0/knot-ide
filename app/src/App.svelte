<script lang="ts">
  /**
   * Phase 0 spike app shell.
   *
   * Flow:
   * 1. App starts with no workspace → welcome screen with "Open Folder"
   * 2. User picks a project folder → becomes workspace root
   * 3. LSP client starts with that folder as rootUri (NOT homeDir — that
   *    caused the server to index the entire user profile)
   * 4. File browser shows .tw/.twee files from the workspace
   * 5. Clicking a file opens it in the Monaco editor
   */

  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { readTextFile } from '@tauri-apps/plugin-fs';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import Editor from '$lib/editor/Editor.svelte';
  import FileBrowser from '$lib/filebrowser/FileBrowser.svelte';
  import { initializeMonaco } from '$lib/editor/monaco-init';
  import { startLanguageClient } from '$lib/lsp/client';

  // State (Svelte 5 runes).
  let workspaceFolder = $state<string | null>(null);
  let filePath = $state<string | null>(null);
  let fileContent = $state<string>('');
  let lspStatus = $state<'idle' | 'starting' | 'ready' | 'failed' | 'exited'>('idle');
  let lspError = $state<string>('');

  onMount(async () => {
    // Initialize Monaco (registers the Twee language + grammar).
    try {
      await initializeMonaco();
      console.log('[knot] Monaco initialized');
    } catch (err) {
      console.error('[knot] Monaco init failed:', err);
      lspError = `Monaco init failed: ${err instanceof Error ? err.message : String(err)}`;
    }

    // Listen for LSP lifecycle events from the Rust backend.
    await listen('lsp-started', () => {
      console.log('[knot] lsp-started event received');
    });
    await listen<string>('lsp-start-failed', (event) => {
      console.error('[knot] lsp-start-failed:', event.payload);
      lspStatus = 'failed';
      lspError = event.payload;
    });
    await listen<number>('lsp-exited', (event) => {
      console.warn('[knot] lsp-exited:', event.payload);
      lspStatus = 'exited';
      lspError = `knot-server exited with code ${event.payload}`;
    });

    // Listen for native menu bar actions.
    await listen<string>('menu-action', (event) => {
      handleMenuAction(event.payload);
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
          // Trigger the file browser's new-file via a custom event
          window.dispatchEvent(new CustomEvent('knot-new-file'));
        }
        break;
      case 'new-folder':
        if (workspaceFolder) {
          window.dispatchEvent(new CustomEvent('knot-new-folder'));
        }
        break;
      case 'save':
        // Phase 1: wire to editor save
        console.log('[knot] save (not yet implemented)');
        break;
      case 'find':
        // Phase 1: wire to Monaco find
        console.log('[knot] find (not yet implemented)');
        break;
      case 'rename':
        if (filePath) {
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
        alert('Knot — Phase 0 Spike\nTwine/Twee IDE\nv0.1.0');
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
    lspStatus = 'starting';
    console.log('[knot] workspace folder:', selected);

    // Tell the Rust backend the workspace root (for path validation in fs_ops).
    await invoke('set_workspace_root', { path: selected });

    // Start the LSP client with the selected folder as workspace root.
    try {
      await startLanguageClient(selected);
      console.log('[knot] LanguageClient started');
      lspStatus = 'ready';
    } catch (err) {
      console.error('[knot] LanguageClient start failed:', err);
      lspStatus = 'failed';
      lspError = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleSelectFile(path: string) {
    // Guard: don't try to read directories as files. The file browser should
    // only call this for files, but double-check in case of miscategorization.
    if (path.endsWith('/.git') || path.endsWith('\\.git') || path.endsWith('/.git/') || path.endsWith('\\.git\\')) {
      console.warn('[knot] refusing to open .git as a file (it is a directory)');
      return;
    }
    try {
      const content = await readTextFile(path);
      // Set content BEFORE filePath so the Editor's model-swap effect
      // sees the new content when it creates the model.
      fileContent = content;
      filePath = path;
      lspError = '';
      console.log('[knot] opened file:', path, 'length:', content.length);
    } catch (err) {
      console.error('[knot] file read failed:', err);
      lspError = `Failed to read file: ${err instanceof Error ? err.message : String(err)}`;
    }
  }

  // Compute the file:// URI for Monaco. On Windows, paths like `D:\path\file.twee`
  // must become `file:///D:/path/file.twee` (forward slashes, triple slash).
  let monacoUri = $derived.by(() => {
    if (!filePath) return 'inmemory://knot/empty.twee';
    if (filePath.startsWith('file://') || filePath.startsWith('inmemory://')) {
      return filePath;
    }
    const normalized = filePath.replace(/\\/g, '/');
    return `file://${normalized.startsWith('/') ? '' : '/'}${normalized}`;
  });
</script>

<div class="app">
  <header class="toolbar">
    {#if workspaceFolder}
      <span class="folder-name">{workspaceFolder}</span>
    {:else}
      <span class="app-title">Knot — Phase 0 Spike</span>
    {/if}
  </header>

  {#if !workspaceFolder}
    <!-- Welcome screen -->
    <main class="welcome">
      <div class="welcome-content">
        <h1>Knot</h1>
        <p>Twine/Twee IDE — Phase 0 Spike</p>
        <button class="open-folder-btn" onclick={handleOpenFolder}>
          Open Project Folder…
        </button>
        {#if lspError}
          <p class="error">{lspError}</p>
        {/if}
      </div>
    </main>
  {:else}
    <!-- Workspace: file browser + editor -->
    <main class="workspace">
      <aside class="sidebar">
        <FileBrowser
          folder={workspaceFolder}
          currentFile={filePath}
          onSelect={handleSelectFile}
        />
      </aside>
      <section class="editor-area">
        {#if filePath}
          <Editor uri={monacoUri} content={fileContent} />
        {:else}
          <div class="no-file">Select a file from the browser</div>
        {/if}
      </section>
    </main>
  {/if}

  <footer class="status-bar">
    <span class="status-item">
      LSP:
      <span class="status-{lspStatus}">{lspStatus}</span>
    </span>
    {#if lspError}
      <span class="status-item error">{lspError}</span>
    {/if}
  </footer>
</div>

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

  .toolbar button {
    background: #0e639c;
    color: white;
    border: none;
    padding: 6px 12px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 13px;
  }

  .toolbar button:hover {
    background: #1177bb;
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
    display: flex;
    overflow: hidden;
  }

  .sidebar {
    width: 240px;
    flex-shrink: 0;
    border-right: 1px solid #3c3c3c;
    overflow: hidden;
  }

  .editor-area {
    flex: 1;
    position: relative;
    overflow: hidden;
  }

  .no-file {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: #666;
    font-size: 14px;
  }

  .status-bar {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 4px 12px;
    background: #007acc;
    color: white;
    font-size: 12px;
    flex-shrink: 0;
  }

  .status-item {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .status-idle { color: #888; }
  .status-starting { color: #ffcc00; }
  .status-ready { color: #4ec9b0; }
  .status-failed, .status-exited { color: #f48771; }

  .error {
    color: #f48771;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
