<script lang="ts">
  /**
   * Draggable resize handle between split children.
   *
   * Pure interaction component: captures pointer events, calculates the drag
   * delta as a percentage of the parent container, and calls `onDrag`. Knows
   * nothing about the layout tree — the parent ({@link SplitView.svelte})
   * decides what to do with the delta.
   *
   * ## Pointer capture
   *
   * Uses `setPointerCapture` so the handle keeps receiving `pointermove`
   * events even when the cursor leaves the handle element (e.g. over the
   * editor). Released on `pointerup` / `pointercancel`.
   */

  interface Props {
    /** Split direction. Determines cursor + which axis to track. */
    direction: 'horizontal' | 'vertical';
    /** Called on every pointermove during drag. `deltaPercent` is signed. */
    onDrag: (deltaPercent: number) => void;
  }

  let { direction, onDrag }: Props = $props();

  let dragging = $state(false);
  let lastPos = 0;
  let containerSize = 0;

  /** Handle pointer down — start drag, capture pointer, record container size. */
  function handlePointerDown(e: PointerEvent): void {
    e.preventDefault();
    e.stopPropagation();
    dragging = true;
    lastPos = direction === 'horizontal' ? e.clientX : e.clientY;
    // The parent SplitView container is the handle's offsetParent.
    const parent = (e.currentTarget as HTMLElement).parentElement;
    containerSize = parent
      ? direction === 'horizontal'
        ? parent.clientWidth
        : parent.clientHeight
      : 0;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  /** Handle pointer move — fire onDrag with the delta since last move. */
  function handlePointerMove(e: PointerEvent): void {
    if (!dragging) return;
    const currentPos = direction === 'horizontal' ? e.clientX : e.clientY;
    const delta = currentPos - lastPos;
    lastPos = currentPos;
    if (containerSize > 0) {
      onDrag((delta / containerSize) * 100);
    }
  }

  /** Handle pointer up — end drag, release capture. */
  function handlePointerUp(e: PointerEvent): void {
    if (!dragging) return;
    dragging = false;
    (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
  }
</script>

<div
  class="resize-handle {direction}"
  class:dragging
  role="separator"
  aria-orientation={direction === 'horizontal' ? 'vertical' : 'horizontal'}
  onpointerdown={handlePointerDown}
  onpointermove={handlePointerMove}
  onpointerup={handlePointerUp}
  onpointercancel={handlePointerUp}
></div>

<style>
  .resize-handle {
    flex-shrink: 0;
    background: #1e1e1e;
    transition: background 0.1s;
  }

  .resize-handle:hover,
  .resize-handle.dragging {
    background: #007acc;
  }

  /* Horizontal split → handle is a vertical bar (cursor: col-resize). */
  .resize-handle.horizontal {
    width: 4px;
    cursor: col-resize;
  }

  /* Vertical split → handle is a horizontal bar (cursor: row-resize). */
  .resize-handle.vertical {
    height: 4px;
    cursor: row-resize;
  }
</style>
