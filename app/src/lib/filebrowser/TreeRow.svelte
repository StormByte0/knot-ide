<script lang="ts">
  /**
   * Single row in the file browser tree.
   *
   * Owns: row markup, chevron/icon/name, inline-edit `<input>`, drag-and-drop
   * event forwarding, and the focus-and-select action for inline editing.
   *
   * Does NOT own: tree state, selection state, edit state, clipboard. Those
   * live in `FileBrowser.svelte` and are passed in as props + callbacks.
   * This keeps the row a pure presentation component (CONVENTIONS §2.3).
   *
   * All interaction handlers live on the root `.tree-row` div — NOT on a
   * nested button. This is intentional: the indent padding (which can be
   * 50+px for deep nodes) is part of the row's clickable/droppable area.
   * If handlers were on a child element, dragging over the indent would
   * show the browser's "no-drop" cursor because `dragover.preventDefault()`
   * would never fire for the padding region.
   */

  import type { TreeNode } from './types';
  import { getFileIcon } from './icons';

  interface Props {
    /** The tree node to render. */
    node: TreeNode;
    /** True if this row is the currently selected/active file. */
    isSelected: boolean;
    /** True if this row is the current drag-drop target. */
    isDropTarget: boolean;
    /** True if this row is on the clipboard with a 'cut' operation. */
    isCut: boolean;
    /** True if this row is currently in inline-edit mode. */
    isEditing: boolean;
    /** Current value of the inline-edit input (owned by parent). */
    editValue: string;
    /** Validation error text for the inline-edit, or null. */
    editError: string | null;
    /** True when editing in 'rename' mode (drives filename-only selection). */
    isRenaming: boolean;

    /** Click on the row — toggles dir or opens file. */
    onClick: (node: TreeNode) => void;
    /** Right-click — opens context menu. */
    onContextMenu: (node: TreeNode, x: number, y: number) => void;
    /** Drag start — begins a drag operation. */
    onDragStart: (e: DragEvent, node: TreeNode) => void;
    /** Drag over — validates drop target + auto-expand. */
    onDragOver: (e: DragEvent, node: TreeNode) => void;
    /** Drag leave — clears drop target. */
    onDragLeave: (e: DragEvent, node: TreeNode) => void;
    /** Drop — performs move/copy. */
    onDrop: (e: DragEvent, node: TreeNode) => void;
    /** Drag end — cleanup. */
    onDragEnd: () => void;
    /** Inline-edit input keydown (Enter confirms, Escape cancels). */
    onEditKeydown: (e: KeyboardEvent) => void;
    /** Inline-edit input blur — confirms edit. */
    onEditBlur: () => void;
    /** Inline-edit input value changed. */
    onEditInput: (value: string) => void;
  }

  let {
    node,
    isSelected,
    isDropTarget,
    isCut,
    isEditing,
    editValue,
    editError,
    isRenaming,
    onClick,
    onContextMenu,
    onDragStart,
    onDragOver,
    onDragLeave,
    onDrop,
    onDragEnd,
    onEditKeydown,
    onEditBlur,
    onEditInput,
  }: Props = $props();

  /**
   * Svelte action: focus the input and select the filename (not extension)
   * when renaming a file. For new file/folder, select the full text.
   */
  function focusAndSelect(input: HTMLInputElement) {
    input.focus();
    if (isRenaming && !node.isDirectory) {
      const lastDot = input.value.lastIndexOf('.');
      if (lastDot > 0) {
        input.setSelectionRange(0, lastDot);
        return;
      }
    }
    input.select();
  }
</script>

<div
  class="tree-row"
  class:selected={isSelected}
  class:drop-target={isDropTarget}
  class:cut={isCut}
  data-id={node.path}
  style="padding-left: {8 + node.depth * 14}px; --depth: {node.depth};"
  role="treeitem"
  tabindex="-1"
  aria-selected={isSelected}
  draggable={node.path !== ''}
  onclick={() => onClick(node)}
  onkeydown={(e) => {
    // Enter/Space activate the row; arrow keys bubble to the container's
    // handleTreeKeydown for tree-wide navigation.
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      e.stopPropagation();
      onClick(node);
    }
  }}
  oncontextmenu={(e) => { e.preventDefault(); e.stopPropagation(); onContextMenu(node, e.clientX, e.clientY); }}
  ondragstart={(e) => onDragStart(e, node)}
  ondragover={(e) => onDragOver(e, node)}
  ondragleave={(e) => onDragLeave(e, node)}
  ondrop={(e) => onDrop(e, node)}
  ondragend={onDragEnd}
>
  <span class="indent-guides" aria-hidden="true"></span>
  <span class="chevron">
    {#if node.isDirectory}
      {#if node.loading}⋯{:else if node.expanded}▾{:else}▸{/if}
    {/if}
  </span>
  <span class="icon">{getFileIcon(node.name, node.isDirectory)}</span>
  {#if isEditing}
    <input
      class="inline-edit"
      value={editValue}
      oninput={(e) => onEditInput(e.currentTarget.value)}
      onkeydown={onEditKeydown}
      onblur={onEditBlur}
      use:focusAndSelect
    />
    {#if editError}<span class="edit-error">{editError}</span>{/if}
  {:else}
    <span class="name">{node.name}</span>
  {/if}
</div>

<style>
  .tree-row {
    display: flex;
    align-items: center;
    gap: 4px;
    box-sizing: border-box;
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
    line-height: 1.4;
    user-select: none;
    position: relative;
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

  /**
   * Indent guide lines — one vertical line per nesting depth level.
   *
   * Uses `repeating-linear-gradient` to draw a 1px line every 14px (matching
   * the indent step in `padding-left`). The gradient is drawn from the left
   * edge of the row up to `calc(var(--depth) * 14px)`, so a depth-3 node
   * gets 3 guide lines. Lines are centered in the 14px indent column
   * (7px offset).
   *
   * The element is `position: absolute` so it sits behind the row content
   * and doesn't affect the flex layout.
   */
  .indent-guides {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    width: calc(var(--depth) * 14px);
    pointer-events: none;
    background-image: repeating-linear-gradient(
      to right,
      transparent 0,
      transparent 7px,
      #3c3c3c 7px,
      #3c3c3c 8px,
      transparent 8px,
      transparent 14px
    );
  }

  /* Don't show guide lines when the row has no depth (root-level items). */
  .tree-row[style*="--depth: 0"] .indent-guides {
    display: none;
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
    flex: 1;
    min-width: 0;
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
</style>
