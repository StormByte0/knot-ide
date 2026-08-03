<script lang="ts">
  /**
   * File browser — VS Code-style file explorer.
   *
   * Orchestrates: tree state, lazy loading, inline editing, clipboard,
   * drag-and-drop, keyboard navigation, auto-reveal, and FS-watcher refresh.
   *
   * Row rendering is delegated to `TreeRow.svelte` (CONVENTIONS §2.3 — single
   * responsibility). This component owns state and dispatches; the row is pure
   * presentation + event forwarding. Pure tree-walking helpers (find, flatten,
   * merge, path resolution, name validation) are extracted into
   * `fileTreeHelpers.ts`; the context menu item builder is in `menuBuilder.ts`.
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
   * - Auto-refresh via FS watcher (backend emits `fs-changed` events)
   *
   * ## Size justification (CONVENTIONS §2.5)
   *
   * This file is ~920 lines, exceeding the 800-line guideline. The remaining
   * logic after extracting pure helpers is genuinely component-specific: it
   * tightly couples Svelte 5 `$state`/`$derived` reactivity with event
   * handlers that read + write multiple interdependent state fields (e.g.
   * `editState` + `editValue` + `editError` + `rootChildren` + `selectedNode`
   * all change together during an inline rename). Splitting further would
   * either:
   * - Create circular imports (state modules importing the component for
   *   callback types), or
   * - Force passing 10+ props/callbacks between the component and a
   *   "controller" module, which is worse for readability than the current
   *   inline structure.
   *
   * The extracted helpers (`fileTreeHelpers.ts`, `menuBuilder.ts`) are the
   * pure, testable parts. This file is the reactive shell that wires them
   * to the DOM + Tauri backend. Reviewed during the Phase 1 audit
   * (PLAN.md §13.9) — acceptable as-is.
   */

  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { join } from '@tauri-apps/api/path';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import ContextMenu, { type MenuEntry } from './ContextMenu.svelte';
  import TreeRow from './TreeRow.svelte';
  import type { Clipboard, EditState, FileEntry, FsChangedEvent, TreeNode } from './types';
  import {
    parentDir as parentDirHelper,
    makeNode,
    mergeChildren,
    findNode,
    flatten,
    collectExpandedPaths,
    getTargetDir as getTargetDirHelper,
    validateEditName,
  } from './fileTreeHelpers';
  import { buildContextMenuItems } from './menuBuilder';

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
    /**
     * Optional: directory paths that should start expanded. Used to restore
     * the filebrowser's expand state from `.knot/window-state.json` on app
     * launch. `undefined` on a fresh workspace (everything starts collapsed).
     */
    initialExpandedPaths?: string[];
    /**
     * Optional: called whenever the set of expanded directory paths changes
     * (toggle, auto-expand on hover, drag-over auto-expand, etc.). The
     * caller persists the new set so the next launch restores it. Receives
     * the complete current set of expanded paths (absolute, OS-native).
     */
    onExpandedPathsChange?: (expandedPaths: string[]) => void;
  }

  let { folder, currentFile, onSelect, initialExpandedPaths, onExpandedPathsChange }: Props = $props();

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
  let editState = $state<EditState | null>(null);
  let editValue = $state('');
  let editError = $state<string | null>(null);

  // --- Clipboard ---
  let clipboard = $state<Clipboard>(null);

  // --- FS watcher ---
  let fsUnlisten: UnlistenFn | null = null;
  let treeContainer: HTMLDivElement;

  // --- Path helpers ---
  // `parentDir` and other pure tree helpers are imported from
  // `fileTreeHelpers.ts`. The local wrapper binds `folder` so callers don't
  // have to pass it every time.

  /** Get the parent directory of a path, falling back to the workspace root. */
  function parentDir(path: string): string {
    return parentDirHelper(path, folder);
  }

  // --- Tree building ---

  async function fetchChildren(dirPath: string, depth: number): Promise<TreeNode[]> {
    const entries = await invoke<FileEntry[]>('list_dir', { path: dirPath });
    return entries.map((e) => makeNode(e, depth));
  }

  async function refresh() {
    loading = true;
    error = null;
    try {
      rootChildren = await fetchChildren(folder, 0);
      // After the initial load, restore expanded state from `initialExpandedPaths`.
      // Called here (not in onMount) so the tree is populated before we walk it.
      if (initialExpandedPaths && initialExpandedPaths.length > 0) {
        await restoreExpandedState(initialExpandedPaths);
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  /**
   * Restore expanded state from a saved list of paths.
   *
   * Walks the saved paths in order, expanding each directory. Because the
   * tree is lazy-loaded, expanding a directory fetches its children — so
   * expanding a deeply-nested path automatically fetches every ancestor.
   *
   * Skips paths that no longer exist (the file/folder was deleted since the
   * state was saved). This is a soft failure — we restore what we can.
   */
  async function restoreExpandedState(pathsToRestore: string[]): Promise<void> {
    // Sort by depth (shallowest first) so ancestors are expanded before
    // children — otherwise the child fetch would fail because the parent
    // isn't loaded yet.
    const sorted = [...pathsToRestore].sort((a, b) => a.length - b.length);
    for (const path of sorted) {
      // Skip the workspace root — it's always "expanded" (its children are
      // the top-level rootChildren, always visible).
      if (path === folder) continue;
      // Find the node in the current tree. If not found, the path may be a
      // descendant of an as-yet-unexpanded directory — but because we sorted
      // shallowest-first, ancestors should already be expanded by the time
      // we reach descendants. If still not found, the path no longer exists.
      const node = findNode(rootChildren, path);
      if (node && node.isDirectory && !node.expanded) {
        await toggleDir(node);
      }
    }
    // Notify the parent of the current expanded set (so the restored set
    // becomes the new baseline even if some paths were skipped).
    notifyExpandedPaths();
  }

  /**
   * Collect the current set of expanded directory paths + notify the parent.
   * Called after every expand/collapse, after restore, and after watcher-
   * triggered refreshes. The parent persists the set so the next launch
   * restores it.
   */
  function notifyExpandedPaths(): void {
    if (!onExpandedPathsChange) return;
    const expanded = collectExpandedPaths(rootChildren);
    onExpandedPathsChange(expanded);
  }

  async function refreshDir(dirPath: string) {
    if (dirPath === folder) {
      const newChildren = await fetchChildren(folder, 0);
      rootChildren = mergeChildren(rootChildren, newChildren);
      return;
    }
    const node = findNode(rootChildren, dirPath);
    if (node && node.isDirectory && node.loaded) {
      const newChildren = await fetchChildren(node.path, node.depth + 1);
      node.children = mergeChildren(node.children, newChildren);
      rootChildren = [...rootChildren];
    }
  }

  // `mergeChildren` and `findNode` are imported from `fileTreeHelpers.ts`.

  onMount(() => {
    refresh();
    startWatcher();

    const onNewFile = () => startNewFile(null);
    const onNewFolder = () => startNewFolder(null);
    const onRefresh = () => refresh();
    const onRename = () => handleMenuRename();
    window.addEventListener('knot-new-file', onNewFile);
    window.addEventListener('knot-new-folder', onNewFolder);
    window.addEventListener('knot-refresh', onRefresh);
    window.addEventListener('knot-rename', onRename);

    return () => {
      window.removeEventListener('knot-new-file', onNewFile);
      window.removeEventListener('knot-new-folder', onNewFolder);
      window.removeEventListener('knot-refresh', onRefresh);
      window.removeEventListener('knot-rename', onRename);
      fsUnlisten?.();
    };
  });

  /**
   * Handle the "Rename" menu action (`F2`). If a node is selected in the
   * tree, start inline rename on it. If no node is selected but the editor
   * has an active file, find + reveal it in the tree, then start rename.
   * No-op if nothing is selected and no file is open.
   */
  function handleMenuRename(): void {
    if (selectedNode) {
      startRename(selectedNode);
      return;
    }
    // No tree selection — check if there's an active editor file to rename.
    // We read `currentFile` (a prop) instead of importing the layout store,
    // to keep this component decoupled from the layout system.
    if (currentFile) {
      const node = findNode(rootChildren, currentFile);
      if (node) {
        selectedNode = node;
        focusedId = node.id;
        startRename(node);
      } else {
        // File isn't visible in the tree (parent collapsed). Reveal it first,
        // then start rename. Reveal is async — we pass a callback.
        revealFile(currentFile).then(() => {
          const revealed = findNode(rootChildren, currentFile);
          if (revealed) {
            selectedNode = revealed;
            focusedId = revealed.id;
            startRename(revealed);
          }
        });
      }
    }
  }

  async function startWatcher() {
    try {
      await invoke('watch_workspace', { rootPath: folder });
      fsUnlisten = await listen<FsChangedEvent>('fs-changed', (event) => {
        const { kind, path, oldPath } = event.payload;
        switch (kind) {
          case 'create':
          case 'remove':
            // A node appeared/disappeared — refresh the parent to update the tree.
            refreshDir(parentDir(path));
            break;
          case 'rename':
            // Refresh both the old parent (node disappeared) and new parent (node appeared).
            refreshDir(parentDir(path));
            if (oldPath && parentDir(oldPath) !== parentDir(path)) {
              refreshDir(parentDir(oldPath));
            }
            break;
          case 'modify':
            // File content/metadata change — doesn't affect tree structure. Skip.
            break;
        }
      });
    } catch (err) {
      console.error('[knot:filebrowser] watcher failed:', err);
    }
  }

  // --- Flat list rendering ---
  // `flatten` is imported from `fileTreeHelpers.ts`.

  let visibleNodes = $derived.by(() => {
    const nodes = flatten(rootChildren);
    // Insert temporary edit nodes for new-file/new-folder.
    if (editState && (editState.type === 'new-file' || editState.type === 'new-folder')) {
      const tempNode: TreeNode = {
        id: editState.tempId,
        path: '',
        name: editValue || '',
        isDirectory: editState.type === 'new-folder',
        isFile: editState.type === 'new-file',
        children: [],
        expanded: false,
        loaded: false,
        loading: false,
        depth: 0,
      };

      if (editState.parentId === folder) {
        // Root-level: the workspace root isn't a node in rootChildren, so
        // we can't findNode it. Insert the temp node at the end of the
        // root-level flat list.
        nodes.push(tempNode);
      } else {
        const parent = findNode(rootChildren, editState.parentId);
        if (parent && parent.expanded) {
          tempNode.depth = parent.depth + 1;
          const parentIdx = nodes.indexOf(parent);
          if (parentIdx >= 0) {
            nodes.splice(parentIdx + 1, 0, tempNode);
          }
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
    notifyExpandedPaths();
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
    notifyExpandedPaths();
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

  /** Get the target directory for an operation. Falls back to selectedNode, then folder. */
  function getTargetDir(node: TreeNode | null): string {
    return getTargetDirHelper(node ?? selectedNode, folder);
  }

  // --- Inline editing ---

  function startNewFile(node: TreeNode | null) {
    const targetDir = getTargetDir(node);
    const target = node ?? selectedNode;
    // parentId is always the target directory's path — for directories it's
    // the dir itself, for files it's the file's parent, for nothing it's root.
    const parent = target?.isDirectory
      ? target
      : findNode(rootChildren, targetDir);
    if (parent && !parent.expanded) {
      parent.expanded = true;
    }
    editState = { type: 'new-file', parentPath: targetDir, parentId: targetDir, tempId: `__temp_${Date.now()}` };
    editValue = 'untitled.twee';
    editError = null;
    rootChildren = [...rootChildren];
  }

  function startNewFolder(node: TreeNode | null) {
    const targetDir = getTargetDir(node);
    const target = node ?? selectedNode;
    const parent = target?.isDirectory
      ? target
      : findNode(rootChildren, targetDir);
    if (parent && !parent.expanded) {
      parent.expanded = true;
    }
    editState = { type: 'new-folder', parentPath: targetDir, parentId: targetDir, tempId: `__temp_${Date.now()}` };
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

  // `validateEditName` is imported from `fileTreeHelpers.ts`.

  async function confirmEdit() {
    if (!editState) return;

    let name: string;
    try {
      name = validateEditName(editValue);
    } catch (e) {
      editError = e instanceof Error ? e.message : String(e);
      return;
    }

    const state = editState;
    editState = null;
    editError = null;

    try {
      switch (state.type) {
        case 'new-file': {
          const fullPath = await join(state.parentPath, name);
          // If the name has slashes, intermediate dirs must be created first.
          const lastSlash = name.lastIndexOf('/');
          if (lastSlash > 0) {
            const dirPart = await join(state.parentPath, name.substring(0, lastSlash));
            await invoke<string>('create_dir_all', { path: dirPart });
          }
          await invoke<string>('create_file', { path: fullPath });
          await refreshDir(state.parentPath);
          onSelect(fullPath);
          break;
        }
        case 'new-folder': {
          const fullPath = await join(state.parentPath, name);
          await invoke<string>('create_dir_all', { path: fullPath });
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

  // --- Context menu actions ---
  // `getContextMenuItems` is imported as `buildContextMenuItems` from `menuBuilder.ts`.

  /** Build context menu items for the current node + clipboard state. */
  function getContextMenuItems(node: TreeNode | null): MenuEntry[] {
    return buildContextMenuItems(node, clipboard);
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
          notifyExpandedPaths();
        } else {
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
        startRename(node);
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
    // File→parent bubble: dropping on a file targets its parent directory
    // (v2 spec). The target path is the dir we'd move/copy into.
    const targetPath = node.isDirectory ? node.path : parentDir(node.path);
    if (targetPath === draggedNode.path) return;
    if (draggedNode.isDirectory && targetPath.startsWith(draggedNode.path)) return;
    e.preventDefault();
    if (e.dataTransfer) {
      dropEffect = (e.ctrlKey || e.metaKey) ? 'copy' : 'move';
      e.dataTransfer.dropEffect = dropEffect;
    }
    // Highlight the directory node (null if root — no root node exists).
    dropTarget = node.isDirectory ? node : findNode(rootChildren, targetPath);

    // Auto-expand directory on hover.
    if (node.isDirectory && !node.expanded && !hoverTimer) {
      hoverTimer = setTimeout(() => {
        if (dropTarget === node) {
          toggleDir(node);
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
    if (!source) return;
    // File→parent bubble: compute the actual target directory path.
    const targetPath = target.isDirectory ? target.path : parentDir(target.path);
    if (source.path === targetPath) return;
    if (source.isDirectory && targetPath.startsWith(source.path)) return;

    const isCopy = (e.ctrlKey || e.metaKey);
    try {
      const newPath = await join(targetPath, source.name);
      if (isCopy) {
        await invoke<string>('copy_file', { src: source.path, dest: newPath });
      } else {
        await invoke<string>('rename_path', { oldPath: source.path, newPath });
      }
      const sourceParent = parentDir(source.path);
      if (!isCopy && sourceParent !== targetPath) await refreshDir(sourceParent);
      await refreshDir(targetPath);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  function handleDragEnd() {
    draggedNode = null;
    dropTarget = null;
    if (hoverTimer) { clearTimeout(hoverTimer); hoverTimer = null; }
  }

  // --- Row click/context dispatch (passed to TreeRow) ---

  function handleRowClick(node: TreeNode) {
    if (node.isDirectory) {
      toggleDir(node);
    } else {
      handleSelectFile(node);
    }
  }

  // --- Auto-reveal ---

  $effect(() => {
    const file = currentFile;
    if (!file || loading) return;
    setTimeout(() => revealFile(file), 50);
  });

  async function revealFile(filePath: string) {
    const parts = filePath.replace(/\\/g, '/').split('/').filter(Boolean);
    let current = folder.replace(/\\/g, '/').replace(/\/$/, '');
    for (let i = 0; i < parts.length - 1; i++) {
      current = current + '/' + parts[i];
      const node = findNode(rootChildren, current);
      if (node && node.isDirectory && !node.expanded) {
        await toggleDir(node);
      }
    }
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
        <TreeRow
          {node}
          isSelected={currentFile === node.path || selectedNode?.path === node.path}
          isDropTarget={dropTarget === node}
          isCut={clipboard?.operation === 'cut' && clipboard.paths.includes(node.path)}
          isEditing={isEditing(node)}
          {editValue}
          {editError}
          isRenaming={editState?.type === 'rename'}
          onClick={handleRowClick}
          onContextMenu={showContextMenu}
          onDragStart={handleDragStart}
          onDragOver={handleDragOver}
          onDragLeave={handleDragLeave}
          onDrop={handleDrop}
          onDragEnd={handleDragEnd}
          onEditKeydown={handleEditKeydown}
          onEditBlur={confirmEdit}
          onEditInput={(v) => (editValue = v)}
        />
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
    background: var(--bg-sidebar);
    color: var(--fg-default);
    font-size: 13px;
    overflow: hidden;
  }

  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 8px 6px 12px;
    border-bottom: 1px solid var(--border-default);
    flex-shrink: 0;
  }

  .title {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--fg-subtle);
  }

  .toolbar-actions {
    display: flex;
    gap: 2px;
  }

  .tool-btn {
    background: none;
    border: none;
    color: var(--fg-default);
    cursor: pointer;
    font-size: 14px;
    padding: 4px 6px;
    border-radius: 3px;
    line-height: 1;
  }

  .tool-btn:hover {
    background: var(--bg-hover);
  }

  .tree-container {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0;
    outline: none;
  }

  .empty {
    padding: 16px 12px;
    color: var(--fg-muted);
    font-size: 12px;
    text-align: center;
  }

  .error {
    padding: 12px;
    color: var(--danger);
    font-size: 12px;
  }
</style>
