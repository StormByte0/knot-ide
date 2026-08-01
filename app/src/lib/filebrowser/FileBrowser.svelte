<script lang="ts">
  /**
   * File browser — main component.
   *
   * Features:
   * - Lazy-loading tree (flat-list rendering, no recursive components)
   * - Selection state for context-aware operations (New File creates in selected dir)
   * - Targeted refresh (only reloads the affected directory, preserves expand state)
   * - Drag and drop move (drag file/folder onto a directory to move it)
   * - Toolbar: New File, New Folder, Refresh, Toggle Expand/Collapse
   * - Context menu: Open, New, Rename, Duplicate, Copy Path, Delete
   */

  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { join } from '@tauri-apps/api/path';
  import ContextMenu, { type MenuEntry } from './ContextMenu.svelte';
  import PromptDialog from './PromptDialog.svelte';
  import ConfirmDialog from './ConfirmDialog.svelte';
  import { getFileIcon } from './icons';
  import type { FileEntry, TreeNode } from './types';

  /** Copy text to clipboard — uses the Tauri plugin if available, falls back to navigator. */
  async function copyToClipboard(text: string): Promise<void> {
    try {
      const { writeText } = await import('@tauri-apps/plugin-clipboard-manager');
      await writeText(text);
    } catch {
      await navigator.clipboard.writeText(text);
    }
  }

  interface Props {
    folder: string;
    currentFile: string | null;
    onSelect: (path: string) => void;
  }

  let { folder, currentFile, onSelect }: Props = $props();

  // Tree state
  let rootChildren = $state<TreeNode[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);

  // Selection state — the currently selected node (for context-aware New File/Folder).
  // Clicking a file selects it; clicking a directory selects + toggles it.
  let selectedNode = $state<TreeNode | null>(null);

  // Context menu state
  let contextMenu = $state<{ x: number; y: number; node: TreeNode | null } | null>(null);

  // Drag and drop state
  let draggedNode = $state<TreeNode | null>(null);
  let dropTarget = $state<TreeNode | null>(null);

  // Dialog state — discriminated union, no closures in $state.
  type PromptState =
    | { type: 'new-file'; targetDir: string }
    | { type: 'new-folder'; targetDir: string }
    | { type: 'rename'; node: TreeNode };

  type ConfirmState =
    | { type: 'delete'; node: TreeNode };

  let promptState = $state<PromptState | null>(null);
  let confirmState = $state<ConfirmState | null>(null);

  let promptDialog = $derived.by(() => {
    if (!promptState) return null;
    switch (promptState.type) {
      case 'new-file':
        return { title: 'New File', label: 'File name:', defaultValue: 'untitled.twee', confirmLabel: 'Create' };
      case 'new-folder':
        return { title: 'New Folder', label: 'Folder name:', defaultValue: 'new-folder', confirmLabel: 'Create' };
      case 'rename':
        return { title: 'Rename', label: 'New name:', defaultValue: promptState.node.name, confirmLabel: 'Rename' };
    }
  });

  let confirmDialog = $derived.by(() => {
    if (!confirmState) return null;
    switch (confirmState.type) {
      case 'delete':
        return {
          title: 'Delete',
          message: `Are you sure you want to delete "${confirmState.node.name}"?${confirmState.node.isDirectory ? ' This will delete all contents.' : ''}\n\nThe item will be moved to the trash.`,
        };
    }
  });

  // --- Tree building (lazy) ---

  function makeNode(entry: FileEntry, depth: number): TreeNode {
    return { ...entry, children: [], expanded: false, loaded: false, loading: false, depth };
  }

  async function fetchChildren(dirPath: string, depth: number): Promise<TreeNode[]> {
    const entries = await invoke<FileEntry[]>('list_dir', { path: dirPath });
    return entries.map((e) => makeNode(e, depth));
  }

  /** Full refresh — reloads top-level. Only used on initial mount and manual refresh. */
  async function refresh() {
    loading = true;
    error = null;
    try {
      rootChildren = await fetchChildren(folder, 0);
      console.log('[knot:filebrowser] loaded top level:', rootChildren.length, 'entries');
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  /**
   * Targeted refresh — reloads only the children of `dirPath`, preserving
   * expand state of all other directories. Used after create/delete/rename
   * so the tree doesn't collapse.
   *
   * If `dirPath` is the workspace root, reloads rootChildren.
   * If the directory isn't currently expanded/loaded, does nothing (no need).
   */
  async function refreshDir(dirPath: string) {
    if (dirPath === folder) {
      rootChildren = await fetchChildren(folder, 0);
      rootChildren = [...rootChildren];
      return;
    }
    // Find the node in the tree and reload its children.
    const node = findNode(rootChildren, dirPath);
    if (node && node.isDirectory && node.loaded) {
      node.children = await fetchChildren(node.path, node.depth + 1);
      rootChildren = [...rootChildren];
    }
  }

  /** Recursively find a node by path. */
  function findNode(nodes: TreeNode[], path: string): TreeNode | null {
    for (const node of nodes) {
      if (node.path === path) return node;
      if (node.isDirectory && node.children.length > 0) {
        const found = findNode(node.children, path);
        if (found) return found;
      }
    }
    return null;
  }

  onMount(() => {
    refresh();
    const onNewFile = () => handleNewFile(null);
    const onNewFolder = () => handleNewFolder(null);
    const onRefresh = () => refresh();
    window.addEventListener('knot-new-file', onNewFile);
    window.addEventListener('knot-new-folder', onNewFolder);
    window.addEventListener('knot-refresh', onRefresh);
    return () => {
      window.removeEventListener('knot-new-file', onNewFile);
      window.removeEventListener('knot-new-folder', onNewFolder);
      window.removeEventListener('knot-refresh', onRefresh);
    };
  });

  // --- Flat list rendering ---

  function flatten(nodes: TreeNode[], result: TreeNode[] = []): TreeNode[] {
    for (const node of nodes) {
      result.push(node);
      if (node.isDirectory && node.expanded && node.children.length > 0) {
        flatten(node.children, result);
      }
    }
    return result;
  }

  let visibleNodes = $derived.by(() => flatten(rootChildren));

  // --- Tree navigation ---

  async function toggleDir(node: TreeNode) {
    if (!node.isDirectory) return;
    if (!node.loaded && !node.loading) {
      node.loading = true;
      try {
        node.children = await fetchChildren(node.path, node.depth + 1);
        node.loaded = true;
      } catch (err) {
        console.error('[knot:filebrowser] failed to load children for', node.path, err);
        node.children = [];
        node.loaded = true;
      } finally {
        node.loading = false;
      }
    }
    node.expanded = !node.expanded;
    selectedNode = node;
    rootChildren = [...rootChildren];
  }

  function toggleExpandCollapse() {
    function hasExpanded(nodes: TreeNode[]): boolean {
      for (const node of nodes) {
        if (node.isDirectory && node.expanded) return true;
        if (node.isDirectory && node.children.length > 0 && hasExpanded(node.children)) return true;
      }
      return false;
    }
    function setAll(nodes: TreeNode[], expanded: boolean) {
      for (const node of nodes) {
        if (node.isDirectory) {
          node.expanded = expanded;
          setAll(node.children, expanded);
        }
      }
    }
    setAll(rootChildren, !hasExpanded(rootChildren));
    rootChildren = [...rootChildren];
  }

  // --- File operations ---

  function handleSelectFile(node: TreeNode) {
    selectedNode = node;
    onSelect(node.path);
  }

  function showContextMenu(node: TreeNode | null, x: number, y: number) {
    contextMenu = { x, y, node };
    selectedNode = node;
  }

  function getContextMenuItems(node: TreeNode | null): MenuEntry[] {
    const items: MenuEntry[] = [];
    if (node && !node.isDirectory) {
      items.push({ id: 'open', label: 'Open', icon: '📂' });
      items.push({ separator: true });
      items.push({ id: 'new-file', label: 'New File…', icon: '📄' });
      items.push({ id: 'new-folder', label: 'New Folder…', icon: '📁' });
      items.push({ separator: true });
      items.push({ id: 'rename', label: 'Rename…', icon: '✏' });
      items.push({ id: 'duplicate', label: 'Duplicate', icon: '⧉' });
      items.push({ id: 'copy-path', label: 'Copy Path', icon: '📋' });
      items.push({ separator: true });
      items.push({ id: 'delete', label: 'Delete…', icon: '🗑', danger: true });
    } else if (node && node.isDirectory) {
      items.push({ id: 'new-file', label: 'New File…', icon: '📄' });
      items.push({ id: 'new-folder', label: 'New Folder…', icon: '📁' });
      items.push({ separator: true });
      items.push({ id: 'rename', label: 'Rename…', icon: '✏' });
      items.push({ id: 'copy-path', label: 'Copy Path', icon: '📋' });
      items.push({ separator: true });
      items.push({ id: 'delete', label: 'Delete…', icon: '🗑', danger: true });
    } else {
      items.push({ id: 'new-file', label: 'New File…', icon: '📄' });
      items.push({ id: 'new-folder', label: 'New Folder…', icon: '📁' });
      items.push({ separator: true });
      items.push({ id: 'refresh', label: 'Refresh', icon: '↻' });
    }
    return items;
  }

  async function handleContextAction(id: string) {
    const node = contextMenu?.node ?? null;
    contextMenu = null;
    switch (id) {
      case 'open': if (node) handleSelectFile(node); break;
      case 'new-file': await handleNewFile(node); break;
      case 'new-folder': await handleNewFolder(node); break;
      case 'rename': if (node) handleRename(node); break;
      case 'duplicate': if (node) await handleDuplicate(node); break;
      case 'copy-path': if (node) await handleCopyPath(node); break;
      case 'delete': if (node) handleDelete(node); break;
      case 'refresh': await refresh(); break;
    }
  }

  /** Determine the target directory for a new file/folder based on selection. */
  function getTargetDir(node: TreeNode | null): string {
    // If no node passed, use the selected node.
    const target = node ?? selectedNode;
    if (!target) return folder;
    if (target.isDirectory) return target.path;
    // File node — use its parent directory.
    const parent = target.path.substring(0, target.path.length - target.name.length);
    return parent.replace(/[/\\]+$/, '');
  }

  function handleNewFile(node: TreeNode | null) {
    const targetDir = getTargetDir(node);
    console.log('[knot:filebrowser] handleNewFile, targetDir:', targetDir);
    promptState = { type: 'new-file', targetDir };
  }

  function handleNewFolder(node: TreeNode | null) {
    const targetDir = getTargetDir(node);
    console.log('[knot:filebrowser] handleNewFolder, targetDir:', targetDir);
    promptState = { type: 'new-folder', targetDir };
  }

  function handleRename(node: TreeNode) {
    promptState = { type: 'rename', node };
  }

  function handleDelete(node: TreeNode) {
    confirmState = { type: 'delete', node };
  }

  async function handlePromptConfirm(name: string) {
    console.log('[knot:filebrowser] handlePromptConfirm:', name, promptState);
    if (!promptState) return;
    const state = promptState;
    promptState = null;
    try {
      switch (state.type) {
        case 'new-file': {
          const fullPath = await join(state.targetDir, name);
          const created = await invoke<string>('create_file', { path: fullPath });
          // Targeted refresh — only reload the parent dir, don't collapse everything.
          await refreshDir(state.targetDir);
          onSelect(created);
          break;
        }
        case 'new-folder': {
          const fullPath = await join(state.targetDir, name);
          await invoke<string>('create_dir', { path: fullPath });
          await refreshDir(state.targetDir);
          // Auto-expand the parent so the new folder is visible.
          const parent = findNode(rootChildren, state.targetDir);
          if (parent && parent.isDirectory && !parent.expanded) {
            parent.expanded = true;
            rootChildren = [...rootChildren];
          }
          break;
        }
        case 'rename': {
          const parent = state.node.path.substring(0, state.node.path.length - state.node.name.length);
          const newPath = await join(parent.replace(/[/\\]+$/, ''), name);
          await invoke<string>('rename_path', { oldPath: state.node.path, newPath });
          await refreshDir(parent.replace(/[/\\]+$/, ''));
          if (!state.node.isDirectory) onSelect(newPath);
          break;
        }
      }
    } catch (err) {
      console.error('[knot:filebrowser] operation failed:', err);
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleConfirmConfirm() {
    if (!confirmState) return;
    const state = confirmState;
    confirmState = null;
    try {
      switch (state.type) {
        case 'delete': {
          await invoke('delete_path', { path: state.node.path });
          // Refresh the parent directory.
          const parentPath = state.node.path.substring(0, state.node.path.length - state.node.name.length).replace(/[/\\]+$/, '');
          await refreshDir(parentPath);
          break;
        }
      }
    } catch (err) {
      console.error('[knot:filebrowser] delete failed:', err);
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleDuplicate(node: TreeNode) {
    try {
      const parent = node.path.substring(0, node.path.length - node.name.length);
      const parentDir = parent.replace(/[/\\]+$/, '');
      const duplicated = await invoke<string>('copy_file', { src: node.path, dest: await join(parentDir, node.name) });
      await refreshDir(parentDir);
      onSelect(duplicated);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleCopyPath(node: TreeNode) {
    try {
      await copyToClipboard(node.path);
    } catch (err) {
      console.error('[knot:filebrowser] copy path failed:', err);
    }
  }

  // --- Drag and drop ---

  function handleDragStart(e: DragEvent, node: TreeNode) {
    draggedNode = node;
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = 'move';
      e.dataTransfer.setData('text/plain', node.path);
    }
  }

  function handleDragOver(e: DragEvent, node: TreeNode) {
    // Only allow drop onto directories.
    if (!draggedNode || !node.isDirectory) return;
    // Don't allow dropping a node onto itself or its own descendant.
    if (node.path === draggedNode.path || draggedNode.path.startsWith(node.path)) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
    dropTarget = node;
  }

  function handleDragLeave(_e: DragEvent, node: TreeNode) {
    if (dropTarget === node) dropTarget = null;
  }

  async function handleDrop(e: DragEvent, target: TreeNode) {
    e.preventDefault();
    e.stopPropagation();
    const source = draggedNode;
    draggedNode = null;
    dropTarget = null;
    if (!source || !target.isDirectory) return;
    if (source.path === target.path) return;
    // Don't allow moving a directory into itself.
    if (source.isDirectory && target.path.startsWith(source.path)) return;

    console.log('[knot:filebrowser] move', source.path, '→', target.path);
    try {
      const newPath = await join(target.path, source.name);
      await invoke<string>('rename_path', { oldPath: source.path, newPath });
      // Refresh both the source parent and the target.
      const sourceParent = source.path.substring(0, source.path.length - source.name.length).replace(/[/\\]+$/, '');
      await refreshDir(sourceParent);
      await refreshDir(target.path);
    } catch (err) {
      console.error('[knot:filebrowser] move failed:', err);
      error = err instanceof Error ? err.message : String(err);
    }
  }

  function handleDragEnd() {
    draggedNode = null;
    dropTarget = null;
  }
</script>

<div class="file-browser">
  <div class="toolbar">
    <span class="title">Explorer</span>
    <div class="toolbar-actions">
      <button class="tool-btn" onclick={() => handleNewFile(null)} title="New File">📄</button>
      <button class="tool-btn" onclick={() => handleNewFolder(null)} title="New Folder">📁</button>
      <button class="tool-btn" onclick={refresh} title="Refresh">↻</button>
      <button class="tool-btn" onclick={toggleExpandCollapse} title="Toggle Expand/Collapse">⤢</button>
    </div>
  </div>

  <div
    class="tree-container"
    role="tree"
    tabindex="-1"
    oncontextmenu={(e) => { e.preventDefault(); showContextMenu(null, e.clientX, e.clientY); }}
  >
    {#if loading}
      <div class="empty">Scanning…</div>
    {:else if error}
      <div class="error">{error}</div>
    {:else if visibleNodes.length > 0}
      {#each visibleNodes as node (node.path)}
        <button
          class="tree-row"
          class:selected={currentFile === node.path || selectedNode?.path === node.path}
          class:drop-target={dropTarget === node}
          style="padding-left: {8 + node.depth * 14}px"
          onclick={() => (node.isDirectory ? toggleDir(node) : handleSelectFile(node))}
          oncontextmenu={(e) => { e.preventDefault(); showContextMenu(node, e.clientX, e.clientY); }}
          draggable="true"
          ondragstart={(e) => handleDragStart(e, node)}
          ondragover={(e) => handleDragOver(e, node)}
          ondragleave={(e) => handleDragLeave(e, node)}
          ondrop={(e) => handleDrop(e, node)}
          ondragend={handleDragEnd}
        >
          <span class="chevron">
            {#if node.isDirectory}
              {#if node.loading}⋯{:else if node.expanded}▾{:else}▸{/if}
            {/if}
          </span>
          <span class="icon">{getFileIcon(node.name, node.isDirectory)}</span>
          <span class="name">{node.name}</span>
        </button>
      {/each}
    {:else}
      <div class="empty">No files found</div>
    {/if}
  </div>
</div>

{#if contextMenu}
  <ContextMenu
    x={contextMenu.x}
    y={contextMenu.y}
    items={getContextMenuItems(contextMenu.node)}
    onAction={handleContextAction}
    onClose={() => (contextMenu = null)}
  />
{/if}

{#if promptDialog}
  <PromptDialog
    title={promptDialog.title}
    label={promptDialog.label}
    defaultValue={promptDialog.defaultValue}
    confirmLabel={promptDialog.confirmLabel}
    onConfirm={handlePromptConfirm}
    onCancel={() => (promptState = null)}
  />
{/if}

{#if confirmDialog}
  <ConfirmDialog
    title={confirmDialog.title}
    message={confirmDialog.message}
    onConfirm={handleConfirmConfirm}
    onCancel={() => (confirmState = null)}
  />
{/if}

<style>
  .file-browser {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: #252526;
    color: #cccccc;
    font-size: 13px;
    overflow: hidden;
  }

  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 8px 6px 12px;
    border-bottom: 1px solid #3c3c3c;
    flex-shrink: 0;
  }

  .title {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: #bbbbbb;
  }

  .toolbar-actions {
    display: flex;
    gap: 2px;
  }

  .tool-btn {
    background: none;
    border: none;
    color: #cccccc;
    cursor: pointer;
    font-size: 14px;
    padding: 4px 6px;
    border-radius: 3px;
    line-height: 1;
  }

  .tool-btn:hover {
    background: #3c3c3c;
  }

  .tree-container {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0;
  }

  .tree-row {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 100%;
    text-align: left;
    background: none;
    border: none;
    color: #cccccc;
    padding: 3px 8px;
    cursor: pointer;
    font-size: 13px;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    line-height: 1.4;
    user-select: none;
  }

  .tree-row:hover {
    background: #2a2d2e;
  }

  .tree-row.selected {
    background: #094771;
    color: #ffffff;
  }

  .tree-row.drop-target {
    background: #0e639c;
    color: #ffffff;
    outline: 1px dashed #4fc1ff;
    outline-offset: -1px;
  }

  .chevron {
    width: 12px;
    text-align: center;
    font-size: 10px;
    color: #888;
    flex-shrink: 0;
  }

  .icon {
    width: 16px;
    text-align: center;
    flex-shrink: 0;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .empty {
    padding: 16px 12px;
    color: #888;
    font-size: 12px;
    text-align: center;
  }

  .error {
    padding: 12px;
    color: #f48771;
    font-size: 12px;
  }
</style>
