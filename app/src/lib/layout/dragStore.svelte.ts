/**
 * Drag session store for tab drag-and-drop.
 *
 * Single source of truth for the active drag. `null` when no drag is in
 * progress. TabStrip sets it on `dragstart`, DockPanel reads it on
 * `dragover`/`drop`, DropOverlay reads it to render the highlight.
 *
 * ## Why a store (not dataTransfer)
 *
 * HTML5 `dataTransfer` is string-only and awkward to use with structured
 * data. A singleton store is cleaner: the source tab id is set on dragstart,
 * any DockPanel can read it during dragover to compute its zone, and it's
 * cleared on dragend. The browser's DnD events still drive the lifecycle —
 * the store just holds the app-level payload.
 *
 * ## Svelte 5 runes
 *
 * Uses `$state` — file must be `*.svelte.ts` so the Svelte compiler
 * processes it.
 */

/** Dock zone — where a dragged tab will land relative to the target panel. */
export type DropZone = 'left' | 'right' | 'top' | 'bottom' | 'center';

/** Active drag session. `null` when no drag is in progress. */
export interface DragSession {
  /** Id of the tab being dragged. */
  sourceTabId: string;
  /** Id of the panel the tab started in. */
  sourcePanelId: string;
  /** Panel id currently under the pointer (drop target), or `null`. */
  targetPanelId: string | null;
  /** Zone currently highlighted, or `null` if not over a valid drop zone. */
  currentZone: DropZone | null;
}

class DragStore {
  /** The active drag session, or `null`. */
  session = $state<DragSession | null>(null);

  /** Start a drag. Called by TabStrip on `dragstart`. */
  startDrag(sourceTabId: string, sourcePanelId: string): void {
    this.session = {
      sourceTabId,
      sourcePanelId,
      targetPanelId: null,
      currentZone: null,
    };
  }

  /** Update the current drop target + zone. Called by DockPanel on `dragover`. */
  setTarget(targetPanelId: string, zone: DropZone): void {
    if (!this.session) return;
    this.session.targetPanelId = targetPanelId;
    this.session.currentZone = zone;
  }

  /** Clear the drop target (e.g. pointer left the panel). */
  clearTarget(): void {
    if (!this.session) return;
    this.session.targetPanelId = null;
    this.session.currentZone = null;
  }

  /** End the drag. Called by TabStrip on `dragend` (always, success or not). */
  endDrag(): void {
    this.session = null;
  }

  /** Whether a drag is in progress. */
  get isActive(): boolean {
    return this.session !== null;
  }
}

/** Singleton drag store. */
export const dragStore = new DragStore();
