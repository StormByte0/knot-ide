<script lang="ts">
  /**
   * File browser — VS Code-style file explorer.
   *
   * Features:
   * - Lazy-loading tree (flat-list rendering, stable node IDs)
   * - Inline New File / New Folder / Rename (no modal — input box in the row)
   * - Extension-aware selection (renaming `file.twee` selects `file`, not `.twee`)
   * - Targeted refresh (only the affected parent dir reloads; expand state preserved)
   * - Selection state for context-aware operations
   * - Cut / Copy / Paste clipboard
   * - Keyboard navigation (arrows, Enter, F2, Delete, Ctrl+X/C/V, Escape)
   * - Drag and drop (move + Ctrl-copy, reject self/cycle, auto-expand on hover)
   * - Auto-reveal (when currentFile changes, expand ancestors + scroll to node)
   * - Indent guides
   * - Auto-refresh via FS watcher (backend emits `fs-changed` events)
   */

  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { join } from '@tauri-apps/api/path';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import ContextMenu, { type MenuEntry } from './ContextMenu.svelte';
  import { getFileIcon } from './icons';
  import type { FileEntry, TreeNode } from './types';

  /** Copy text to clipboard. */
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

  // --- Tree state ---
  let rootChildren = $state<TreeNode[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let selectedNode = $state<TreeNode | null>(null);
  let focusedId = $state<string | null>(null);

  // --- Context menu ---
  let contextMenu = $state<{ x: number; y: number; node: TreeNode | null } | null>(null);

  // --- Drag and drop ---
  let draggedNode = $state<TreeNode | null>(null);
  let dropTarget = $state<TreeNode | null>(null);
  let dropEffect = $state<'move' | 'copy'>('move');
  let hoverTimer: ReturnType<typeof setTimeout> | null = null;

  // --- Editing state (inline input) ---
  type EditState =
    | { type: 'new-file'; parentPath: string; parentId: string; tempId: string }
    | { type: 'new-folder'; parentPath: string; parentId: string; tempId: string }
    | { type: 'rename'; node: TreeNode };
  let editState = $state<EditState | null>(null);
  let editValue = $state('');
  let editError = $state<string | null>(null);

  // --- Clipboard ---
  type Clipboard = { operation: 'copy' | 'cut'; paths: string[] } | null;
  let clipboard = $state<Clipboard>(null);

  // --- FS watcher ---
  let fsUnlisten: UnlistenFn | null = null;
  let treeContainer: HTMLDivElement;

  // --- Tree building ---

  function makeNode(entry: FileEntry, depth: number): TreeNode {
    return {
      id: entry.path,
      path: entry.path,
      name: entry.name,
      isDirectory: entry.isDirectory,
      children: [],
      expanded: false,
      loaded: false,
      loading: false,
      depth,
    };
  }

  async function fetchChildren(dirPath: string, depth: number): Promise<TreeNode[]> {
    const entries = await invoke<FileEntry[]>('list_dir', { path: dirPath });
    return entries.map((e) => makeNode(e, depth));
  }

  async function refresh() {
    loading = true;
    error = null;
    try {
      rootChildren = await fetchChildren(folder, 0);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  async function refreshDir(dirPath: string) {
    if (dirPath === folder) {
      rootChildren = await fetchChildren(folder, 0);
      rootChildren = [...rootChildren];
      return;
    }
    const node = findNode(rootChildren, dirPath);
    if (node && node.isDirectory && node.loaded) {
      node.children = await fetchChildren(node.path, node.depth + 1);
      rootChildren = [...rootChildren];
    }
  }

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
    startWatcher();

    const onNewFile = () => startNewFile(null);
    const onNewFolder = () => startNewFolder(null);
    const onRefresh = () => refresh();
    window.addEventListener('knot-new-file', onNewFile);
    window.addEventListener('knot-new-folder', onNewFolder);
    window.addEventListener('knot-refresh', onRefresh);

    return () => {
      window.removeEventListener('knot-new-file', onNewFile);
      window.removeEventListener('knot-new-folder', onNewFolder);
      window.removeEventListener('knot-refresh', onRefresh);
      fsUnlisten?.();
    };
  });

  async function startWatcher() {
    try {
      await invoke('watch_workspace', { rootPath: folder });
      fsUnlisten = await listen<{ kind: string; path: string }>('fs-changed', (event) => {
        const changedPath = event.payload.path;
        // Refresh the parent of the changed path.
        const parent = changedPath.substring(0, changedPath.lastIndexOf('/') + 1) ||
                       changedPath.substring(0, changedPath.lastIndexOf('\\') + 1);
        const parentDir = parent.replace(/[/\\]+$/, '') || folder;
        refreshDir(parentDir);
      });
    } catch (err) {
      console.error('[knot:filebrowser] watcher failed:', err);
    }
  }

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

  let visibleNodes = $derived.by(() => {
    const nodes = flatten(rootChildren);
    // Insert temporary edit nodes for new-file/new-folder.
    if (editState && (editState.type === 'new-file' || editState.type === 'new-folder')) {
      const parent = editState.type === 'rename' ? null : findNode(rootChildren, editState.parentId);
      if (parent && parent.expanded) {
        // Insert temp node at the right depth.
        const tempNode: TreeNode = {
          id: editState.tempId,
          path: '',
          name: editValue || '',
          isDirectory: editState.type === 'new-folder',
          children: [],
          expanded: false,
          loaded: false,
          loading: false,
          depth: parent.depth + 1,
        };
        // Insert right after parent's last child (or after parent if no children).
        const parentIdx = nodes.indexOf(parent);
        if (parentIdx >= 0) {
          nodes.splice(parentIdx + 1, 0, tempNode);
        }
      }
    }
    return nodes;
  });

  // --- Tree navigation ---

  async function toggleDir(node: TreeNode) {
    if (!node.isDirectory) return;
    if (!node.loaded && !node.loading) {
      node.loading = true;
      try {
        node.children = await fetchChildren(node.path, node.depth + 1);
        node.loaded = true;
      } catch (err) {
        console.error('[knot:filebrowser] load children failed:', err);
        node.children = [];
        node.loaded = true;
      } finally {
        node.loading = false;
      }
    }
    node.expanded = !node.expanded;
    selectedNode = node;
    focusedId = node.id;
    rootChildren = [...rootChildren];
  }

  function toggleExpandCollapse() {
    function hasExpanded(nodes: TreeNode[]): boolean {
      for (const node of nodes) {
        if (node.isDirectory && node.expanded) return true;
        if (node.isDirectory && hasExpanded(node.children)) return true;
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

  // --- Selection ---

  function handleSelectFile(node: TreeNode) {
    selectedNode = node;
    focusedId = node.id;
    onSelect(node.path);
  }

  function showContextMenu(node: TreeNode | null, x: number, y: number) {
    contextMenu = { x, y, node };
    selectedNode = node;
  }

  function getTargetDir(node: TreeNode | null): string {
    const target = node ?? selectedNode;
    if (!target) return folder;
    if (target.isDirectory) return target.path;
    const parent = target.path.substring(0, target.path.length - target.name.length);
    return parent.replace(/[/\\]+$/, '');
  }

  // --- Inline editing ---

  function startNewFile(node: TreeNode | null) {
    const targetDir = getTargetDir(node);
    const target = node ?? selectedNode;
    const parentId = target ? (target.isDirectory ? target.id : folder) : folder;
    // Ensure parent is expanded.
    const parent = target ? (target.isDirectory ? target : findNode(rootChildren, getTargetDir(target))) : null;
    if (parent && !parent.expanded) {
      parent.expanded = true;
    }
    editState = { type: 'new-file', parentPath: targetDir, parentId, tempId: `__temp_${Date.now()}` };
    editValue = 'untitled.twee';
    editError = null;
    rootChildren = [...rootChildren];
  }

  function startNewFolder(node: TreeNode | null) {
    const targetDir = getTargetDir(node);
    const target = node ?? selectedNode;
    const parentId = target ? (target.isDirectory ? target.id : folder) : folder;
    const parent = target ? (target.isDirectory ? target : findNode(rootChildren, getTargetDir(target))) : null;
    if (parent && !parent.expanded) {
      parent.expanded = true;
    }
    editState = { type: 'new-folder', parentPath: targetDir, parentId, tempId: `__temp_${Date.now()}` };
    editValue = 'new-folder';
    editError = null;
    rootChildren = [...rootChildren];
  }

  function startRename(node: TreeNode) {
    editState = { type: 'rename', node };
    editValue = node.name;
    editError = null;
    rootChildren = [...rootChildren];
  }

  function cancelEdit() {
    editState = null;
    editValue = '';
    editError = null;
    rootChildren = [...rootChildren];
  }

  async function confirmEdit() {
    if (!editState) return;
    const name = editValue.trim();
    if (!name) {
      editError = 'Name cannot be empty';
      return;
    }

    const state = editState;
    editState = null;
    editError = null;

    try {
      switch (state.type) {
        case 'new-file': {
          const fullPath = await join(state.parentPath, name);
          await invoke<string>('create_file', { path: fullPath });
          await refreshDir(state.parentPath);
          onSelect(fullPath);
          break;
        }
        case 'new-folder': {
          const fullPath = await join(state.parentPath, name);
          await invoke<string>('create_dir', { path: fullPath });
          await refreshDir(state.parentPath);
          break;
        }
        case 'rename': {
          const parent = state.node.path.substring(0, state.node.path.length - state.node.name.length);
          const newPath = await join(parent.replace(/[/\\]+$/, ''), name);
          if (newPath === state.node.path) break;
          await invoke<string>('rename_path', { oldPath: state.node.path, newPath });
          await refreshDir(parent.replace(/[/\\]+$/, ''));
          if (!state.node.isDirectory) onSelect(newPath);
          break;
        }
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      rootChildren = [...rootChildren];
    }
  }

  function handleEditKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      e.stopPropagation();
      confirmEdit();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      cancelEdit();
    }
  }

  function isEditing(node: TreeNode): boolean {
    if (!editState) return false;
    if (editState.type === 'rename') return editState.node.id === node.id;
    return editState.tempId === node.id;
  }

  /** Compute selection range for rename: filename without extension. */
  function getEditSelection(node: TreeNode | null): { start: number; end: number } {
    if (!node || node.isDirectory) {
      return { start: 0, end: editValue.length };
    }
    const lastDot = node.name.lastIndexOf('.');
    if (lastDot > 0) {
      return { start: 0, end: lastDot };
    }
    return { start: 0, end: editValue.length };
  }

  // --- Context menu actions ---

  function getContextMenuItems(node: TreeNode | null): MenuEntry[] {
    const items: MenuEntry[] = [];
    if (node && !node.isDirectory) {
      items.push({ id: 'open', label: 'Open', icon: '📂' });
      items.push({ separator: true });
      items.push({ id: 'cut', label: 'Cut', icon: '✂' });
      items.push({ id: 'copy', label: 'Copy', icon: '📋' });
      if (clipboard) items.push({ id: 'paste', label: 'Paste', icon: '📥' });
      items.push({ separator: true });
      items.push({ id: 'copy-path', label: 'Copy Path', icon: '🔗' });
      items.push({ id: 'copy-relative-path', label: 'Copy Relative Path', icon: '📎' });
      items.push({ separator: true });
      items.push({ id: 'rename', label: 'Rename…', icon: '✏' });
      items.push({ id: 'delete', label: 'Delete…', icon: '🗑', danger: true });
    } else if (node && node.isDirectory) {
      items.push({ id: 'new-file', label: 'New File…', icon: '📄' });
      items.push({ id: 'new-folder', label: 'New Folder…', icon: '📁' });
      items.push({ separator: true });
      items.push({ id: 'cut', label: 'Cut', icon: '✂' });
      items.push({ id: 'copy', label: 'Copy', icon: '📋' });
      if (clipboard) items.push({ id: 'paste', label: 'Paste', icon: '📥' });
      items.push({ separator: true });
      items.push({ id: 'copy-path', label: 'Copy Path', icon: '🔗' });
      items.push({ separator: true });
      items.push({ id: 'rename', label: 'Rename…', icon: '✏' });
      items.push({ id: 'delete', label: 'Delete…', icon: '🗑', danger: true });
    } else {
      items.push({ id: 'new-file', label: 'New File…', icon: '📄' });
      items.push({ id: 'new-folder', label: 'New Folder…', icon: '📁' });
      if (clipboard) {
        items.push({ separator: true });
        items.push({ id: 'paste', label: 'Paste', icon: '📥' });
      }
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
      case 'new-file': startNewFile(node); break;
      case 'new-folder': startNewFolder(node); break;
      case 'rename': if (node) startRename(node); break;
      case 'delete': if (node) await handleDelete(node); break;
      case 'cut': if (node) handleCut(node); break;
      case 'copy': if (node) handleCopy(node); break;
      case 'paste': await handlePaste(node); break;
      case 'copy-path': if (node) await handleCopyPath(node); break;
      case 'copy-relative-path': if (node) await handleCopyRelativePath(node); break;
      case 'refresh': await refresh(); break;
    }
  }

  // --- Cut / Copy / Paste ---

  function handleCut(node: TreeNode) {
    clipboard = { operation: 'cut', paths: [node.path] };
  }

  function handleCopy(node: TreeNode) {
    clipboard = { operation: 'copy', paths: [node.path] };
  }

  async function handlePaste(target: TreeNode | null) {
    if (!clipboard || clipboard.paths.length === 0) return;
    const targetDir = getTargetDir(target);
    for (const srcPath of clipboard.paths) {
      const name = srcPath.split(/[/\\]/).pop() || 'pasted';
      const dest = await join(targetDir, name);
      try {
        if (clipboard.operation === 'copy') {
          await invoke<string>('copy_file', { src: srcPath, dest });
        } else {
          // cut = move
          await invoke<string>('rename_path', { oldPath: srcPath, newPath: dest });
        }
      } catch (err) {
        error = err instanceof Error ? err.message : String(err);
      }
    }
    if (clipboard.operation === 'cut') clipboard = null;
    await refreshDir(targetDir);
  }

  async function handleCopyPath(node: TreeNode) {
    await copyToClipboard(node.path);
  }

  async function handleCopyRelativePath(node: TreeNode) {
    const rel = node.path.startsWith(folder)
      ? node.path.substring(folder.length).replace(/^[/\\]+/, '')
      : node.path;
    await copyToClipboard(rel);
  }

  async function handleDelete(node: TreeNode) {
    if (!confirm(`Delete "${node.name}"?${node.isDirectory ? ' This will delete all contents.' : ''}\n\nThe item will be moved to the trash.`)) {
      return;
    }
    try {
      await invoke('delete_path', { path: node.path });
      const parent = node.path.substring(0, node.path.length - node.name.length).replace(/[/\\]+$/, '');
      await refreshDir(parent);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  // --- Keyboard navigation ---

  function handleTreeKeydown(e: KeyboardEvent) {
    if (editState) return; // let the input handle keys
    if (!focusedId) return;

    const nodes = visibleNodes;
    const idx = nodes.findIndex((n) => n.id === focusedId);
    if (idx < 0) return;
    const node = nodes[idx];

    switch (e.key) {
      case 'ArrowDown': {
        e.preventDefault();
        const next = nodes[idx + 1];
        if (next) { focusedId = next.id; selectedNode = next; }
        break;
      }
      case 'ArrowUp': {
        e.preventDefault();
        const prev = nodes[idx - 1];
        if (prev) { focusedId = prev.id; selectedNode = prev; }
        break;
      }
      case 'ArrowRight': {
        e.preventDefault();
        if (node.isDirectory && !node.expanded) {
          toggleDir(node);
        } else if (node.isDirectory && node.expanded) {
          const firstChild = nodes[idx + 1];
          if (firstChild) { focusedId = firstChild.id; selectedNode = firstChild; }
        }
        break;
      }
      case 'ArrowLeft': {
        e.preventDefault();
        if (node.isDirectory && node.expanded) {
          node.expanded = false;
          rootChildren = [...rootChildren];
        } else {
          // Move to parent
          for (let i = idx - 1; i >= 0; i--) {
            if (nodes[i].isDirectory && nodes[i].depth < node.depth) {
              focusedId = nodes[i].id;
              selectedNode = nodes[i];
              break;
            }
          }
        }
        break;
      }
      case 'Enter': {
        e.preventDefault();
        if (node.isDirectory) {
          toggleDir(node);
        } else {
          handleSelectFile(node);
        }
        break;
      }
      case 'F2': {
        e.preventDefault();
        if (!node.isDirectory || true) startRename(node);
        break;
      }
      case 'Delete': {
        e.preventDefault();
        handleDelete(node);
        break;
      }
      case 'Escape': {
        e.preventDefault();
        if (clipboard) clipboard = null;
        break;
      }
    }

    // Ctrl+X / Ctrl+C / Ctrl+V
    if ((e.ctrlKey || e.metaKey) && node) {
      if (e.key === 'x') { e.preventDefault(); handleCut(node); }
      else if (e.key === 'c') { e.preventDefault(); handleCopy(node); }
      else if (e.key === 'v') { e.preventDefault(); handlePaste(selectedNode); }
    }
  }

  // --- Drag and drop ---

  function handleDragStart(e: DragEvent, node: TreeNode) {
    draggedNode = node;
    dropEffect = (e.ctrlKey || e.metaKey) ? 'copy' : 'move';
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = 'copyMove';
      e.dataTransfer.setData('text/plain', node.path);
    }
  }

  function handleDragOver(e: DragEvent, node: TreeNode) {
    if (!draggedNode) return;
    // Determine target: folder, or file's parent.
    const target = node.isDirectory ? node : null;
    if (!target) return;
    if (target.path === draggedNode.path) return;
    if (draggedNode.isDirectory && target.path.startsWith(draggedNode.path)) return;
    e.preventDefault();
    if (e.dataTransfer) {
      dropEffect = (e.ctrlKey || e.metaKey) ? 'copy' : 'move';
      e.dataTransfer.dropEffect = dropEffect;
    }
    dropTarget = target;

    // Auto-expand on hover (after 500ms)
    if (target.isDirectory && !target.expanded && !hoverTimer) {
      hoverTimer = setTimeout(() => {
        if (dropTarget === target) {
          toggleDir(target);
        }
        hoverTimer = null;
      }, 500);
    }
  }

  function handleDragLeave(_e: DragEvent, node: TreeNode) {
    if (dropTarget === node) dropTarget = null;
    if (hoverTimer) {
      clearTimeout(hoverTimer);
      hoverTimer = null;
    }
  }

  async function handleDrop(e: DragEvent, target: TreeNode) {
    e.preventDefault();
    e.stopPropagation();
    const source = draggedNode;
    draggedNode = null;
    dropTarget = null;
    if (hoverTimer) { clearTimeout(hoverTimer); hoverTimer = null; }
    if (!source || !target.isDirectory) return;
    if (source.path === target.path) return;
    if (source.isDirectory && target.path.startsWith(source.path)) return;

    const isCopy = (e.ctrlKey || e.metaKey);
    try {
      const newPath = await join(target.path, source.name);
      if (isCopy) {
        await invoke<string>('copy_file', { src: source.path, dest: newPath });
      } else {
        await invoke<string>('rename_path', { oldPath: source.path, newPath });
      }
      const sourceParent = source.path.substring(0, source.path.length - source.name.length).replace(/[/\\]+$/, '');
      if (!isCopy) await refreshDir(sourceParent);
      await refreshDir(target.path);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  function handleDragEnd() {
    draggedNode = null;
    dropTarget = null;
    if (hoverTimer) { clearTimeout(hoverTimer); hoverTimer = null; }
  }

  // --- Auto-reveal ---

  $effect(() => {
    // When currentFile changes, expand ancestors and scroll to the node.
    const file = currentFile;
    if (!file || loading) return;
    // Use setTimeout to ensure the tree is rendered.
    setTimeout(() => revealFile(file), 50);
  });

  async function revealFile(filePath: string) {
    // Walk up the path, expanding each ancestor.
    const parts = filePath.replace(/\\/g, '/').split('/').filter(Boolean);
    let current = folder.replace(/\\/g, '/').replace(/\/$/, '');
    for (let i = 0; i < parts.length - 1; i++) {
      current = current + '/' + parts[i];
      const node = findNode(rootChildren, current);
      if (node && node.isDirectory && !node.expanded) {
        await toggleDir(node);
      }
    }
    // Scroll to the file node.
    const fileNode = findNode(rootChildren, filePath);
    if (fileNode && treeContainer) {
      const el = treeContainer.querySelector(`[data-id="${CSS.escape(filePath)}"]`);
      el?.scrollIntoView({ block: 'center', behavior: 'smooth' });
      selectedNode = fileNode;
      focusedId = fileNode.id;
    }
  }

  onDestroy(() => {
    fsUnlisten?.();
  });
</script>

<div class="file-browser">
  <div class="toolbar">
    <span class="title">Explorer</span>
    <div class="toolbar-actions">
      <button class="tool-btn" onclick={() => startNewFile(null)} title="New File (Ctrl+N)">📄</button>
      <button class="tool-btn" onclick={() => startNewFolder(null)} title="New Folder">📁</button>
      <button class="tool-btn" onclick={refresh} title="Refresh">↻</button>
      <button class="tool-btn" onclick={toggleExpandCollapse} title="Toggle Expand/Collapse">⤢</button>
    </div>
  </div>

  <div
    bind:this={treeContainer}
    class="tree-container"
    role="tree"
    tabindex="-1"
    onkeydown={handleTreeKeydown}
    oncontextmenu={(e) => { e.preventDefault(); showContextMenu(null, e.clientX, e.clientY); }}
  >
    {#if loading}
      <div class="empty">Scanning…</div>
    {:else if error}
      <div class="error">{error}</div>
    {:else if visibleNodes.length > 0}
      {#each visibleNodes as node (node.id)}
        <div
          class="tree-row"
          class:selected={currentFile === node.path || selectedNode?.path === node.path}
          class:drop-target={dropTarget === node}
          class:cut={clipboard?.operation === 'cut' && clipboard.paths.includes(node.path)}
          data-id={node.path}
          style="padding-left: {8 + node.depth * 14}px"
          onclick={() => (node.isDirectory ? toggleDir(node) : handleSelectFile(node))}
          oncontextmenu={(e) => { e.preventDefault(); showContextMenu(node, e.clientX, e.clientY); }}
          draggable={node.path !== ''}
          ondragstart={(e) => handleDragStart(e, node)}
          ondragover={(e) => handleDragOver(e, node)}
          ondragleave={(e) => handleDragLeave(e, node)}
          ondrop={(e) => handleDrop(e, node)}
          ondragend={handleDragEnd}
          role="treeitem"
          aria-selected={selectedNode?.path === node.path}
        >
          <span class="chevron">
            {#if node.isDirectory}
              {#if node.loading}⋯{:else if node.expanded}▾{:else}▸{/if}
            {/if}
          </span>
          <span class="icon">{getFileIcon(node.name, node.isDirectory)}</span>
          {#if isEditing(node)}
            <input
              class="inline-edit"
              bind:value={editValue}
              onkeydown={handleEditKeydown}
              onblur={confirmEdit}
              {autofocus}
            />
            {#if editError}<span class="edit-error">{editError}</span>{/if}
          {:else}
            <span class="name">{node.name}</span>
          {/if}
        </div>
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
    outline: none;
  }

  .tree-row {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 100%;
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

  .tree-row.cut {
    opacity: 0.5;
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

  .inline-edit {
    flex: 1;
    background: #1e1e1e;
    border: 1px solid #007acc;
    color: #fff;
    padding: 1px 4px;
    border-radius: 2px;
    font-size: 13px;
    font-family: inherit;
    outline: none;
    min-width: 0;
  }

  .edit-error {
    color: #f48771;
    font-size: 11px;
    margin-left: 4px;
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
