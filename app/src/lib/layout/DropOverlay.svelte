<script lang="ts">
  /**
   * Drop-zone overlay for tab drag-and-drop.
   *
   * Renders 5 zone indicators (left / right / top / bottom / center) over a
   * panel's content area when a tab is being dragged. The active zone
   * (computed by {@link DockPanel.svelte} from the pointer position) is
   * highlighted.
   *
   * Pure presentation — reads the active zone from props, knows nothing
   * about the drag store or layout store. DockPanel decides which zone is
   * active and passes it down.
   */

  import type { DropZone } from './dragStore.svelte';

  interface Props {
    /** Currently-highlighted zone, or `null` if none. */
    activeZone: DropZone | null;
  }

  let { activeZone }: Props = $props();

  // The 5 zones + their CSS class + label.
  const zones: { id: DropZone; cls: string; label: string }[] = [
    { id: 'left', cls: 'zone-left', label: 'Left' },
    { id: 'right', cls: 'zone-right', label: 'Right' },
    { id: 'top', cls: 'zone-top', label: 'Top' },
    { id: 'bottom', cls: 'zone-bottom', label: 'Bottom' },
    { id: 'center', cls: 'zone-center', label: 'Join' },
  ];
</script>

<div class="drop-overlay">
  {#each zones as zone (zone.id)}
    <div class="zone {zone.cls}" class:active={activeZone === zone.id}>
      <span class="zone-label">{zone.label}</span>
    </div>
  {/each}
</div>

<style>
  .drop-overlay {
    position: absolute;
    inset: 0;
    z-index: 50;
    pointer-events: none;
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    grid-template-rows: 1fr 1fr 1fr;
  }

  .zone {
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 0;
    transition: opacity 0.1s, background 0.1s;
  }

  /* Zone positioning in the 3×3 grid. */
  .zone-left {
    grid-column: 1;
    grid-row: 1 / span 3;
  }
  .zone-right {
    grid-column: 3;
    grid-row: 1 / span 3;
  }
  .zone-top {
    grid-column: 2;
    grid-row: 1;
  }
  .zone-bottom {
    grid-column: 2;
    grid-row: 3;
  }
  .zone-center {
    grid-column: 2;
    grid-row: 2;
  }

  /* Base hover state — faint highlight so the user sees the zones. */
  .zone {
    background: color-mix(in srgb, var(--accent) 8%, transparent);
    opacity: 1;
  }

  /* Active zone — bright highlight. */
  .zone.active {
    background: color-mix(in srgb, var(--accent) 40%, transparent);
    box-shadow: inset 0 0 0 2px var(--accent);
  }

  .zone-label {
    font-size: 11px;
    color: var(--fg-status-bar);
    opacity: 0.7;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    pointer-events: none;
    user-select: none;
  }

  .zone.active .zone-label {
    opacity: 1;
  }
</style>
