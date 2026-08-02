<script lang="ts">
  /**
   * Split container — renders children side-by-side or stacked with resize
   * handles between them.
   *
   * Recursion: each child is rendered via {@link LayoutRoot.svelte}, which
   * may render another `SplitView` (nested split) or a `DockPanel` (leaf).
   * This self-referential rendering is the Svelte 5 idiomatic pattern for
   * recursive trees (same as `FileTree.svelte`).
   *
   * ## Sizing
   *
   * Children use `flex-grow: <size>; flex-basis: 0;` so space is distributed
   * proportionally to the `sizes` array. {@link ResizeHandle} calls
   * `layoutStore.resizeSplit` which mutates `sizes[i]` / `sizes[i+1]` —
   * Svelte 5's `$state` proxy makes the mutation reactive, and the
   * `flex-grow` style updates automatically.
   */

  import LayoutRoot from './LayoutRoot.svelte';
  import ResizeHandle from './ResizeHandle.svelte';
  import { layoutStore } from './layoutStore.svelte';
  import type { LayoutNode, SplitNode } from './types';

  interface Props {
    /** The split node to render. Must be `type: 'split'`. */
    node: LayoutNode;
  }

  let { node }: Props = $props();

  /** Typed view of the split node. `null` if node is not a split (shouldn't happen). */
  let split = $derived(node.type === 'split' ? (node as SplitNode) : null);

  /** Forward resize delta to the store. */
  function handleResize(childIndex: number, deltaPercent: number): void {
    if (!split) return;
    layoutStore.resizeSplit(split, childIndex, deltaPercent);
  }
</script>

{#if split}
  <div class="split-container {split.direction}">
    {#each split.children as child, i (i)}
      <div class="split-child" style="flex-grow: {split.sizes[i]}; flex-basis: 0;">
        <LayoutRoot node={child} />
      </div>
      {#if i < split.children.length - 1}
        <ResizeHandle
          direction={split.direction}
          onDrag={(delta) => handleResize(i, delta)}
        />
      {/if}
    {/each}
  </div>
{/if}

<style>
  .split-container {
    display: flex;
    width: 100%;
    height: 100%;
    overflow: hidden;
  }

  /* Horizontal split → children in a row. */
  .split-container.horizontal {
    flex-direction: row;
  }

  /* Vertical split → children in a column. */
  .split-container.vertical {
    flex-direction: column;
  }

  .split-child {
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    position: relative;
  }
</style>
