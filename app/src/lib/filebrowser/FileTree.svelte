<script lang="ts">
  /**
   * Recursive file tree renderer.
   *
   * Renders a list of TreeNodes. Directories that are expanded recurse into
   * their children. Directories that are loading show a spinner.
   */

  import type { TreeNode } from './types';
  import { getFileIcon } from './icons';

  interface Props {
    nodes: TreeNode[];
    selectedPath: string | null;
    onSelectFile: (node: TreeNode) => void;
    onToggleDir: (node: TreeNode) => void;
    onContext: (node: TreeNode | null, x: number, y: number) => void;
  }

  let { nodes, selectedPath, onSelectFile, onToggleDir, onContext }: Props = $props();
</script>

{#each nodes as node (node.path)}
  <button
    class="tree-row"
    class:selected={selectedPath === node.path}
    style="padding-left: {8 + node.depth * 14}px"
    onclick={() => (node.isDirectory ? onToggleDir(node) : onSelectFile(node))}
    oncontextmenu={(e) => { e.preventDefault(); onContext(node, e.clientX, e.clientY); }}
  >
    <span class="chevron">
      {#if node.isDirectory}
        {#if node.loading}⋯{:else if node.expanded}▾{:else}▸{/if}
      {/if}
    </span>
    <span class="icon">{getFileIcon(node.name, node.isDirectory)}</span>
    <span class="name">{node.name}</span>
  </button>
  {#if node.isDirectory && node.expanded && node.children.length > 0}
    <FileTree
      nodes={node.children}
      {selectedPath}
      {onSelectFile}
      {onToggleDir}
      {onContext}
    />
  {/if}
{/each}

<style>
  .tree-row {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 100%;
    text-align: left;
    background: none;
    border: none;
    color: #cccccc;
    padding: 3px 8px 3px 8px;
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
</style>
