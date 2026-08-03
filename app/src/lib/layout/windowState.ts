/**
 * Window-state serialization — pure transform layer between the reactive
 * layout tree and the JSON written to `.knot/window-state.json`.
 *
 * ## Ownership (CONVENTIONS §2.3)
 *
 * This module is pure: no Tauri `invoke`, no `$state`, no side effects. It
 * converts between {@link LayoutNode} (a Svelte 5 `$state` proxy tree owned by
 * `layoutStore.svelte.ts`) and {@link WindowStateV1} (a plain JSON object
 * suitable for `JSON.stringify`). The store calls `serialize` before invoking
 * the backend; the store calls `deserialize` after the backend returns JSON.
 *
 * ## Versioning
 *
 * The on-disk JSON has a `version` field. The current schema is `1`. If the
 * schema changes in a backwards-incompatible way, bump `version` and write a
 * migrator. Old schemas must remain loadable so users don't lose their
 * layout on upgrade.
 *
 * ## Content persistence
 *
 * Editor-tab content is included in the serialized form. This matches the
 * store's existing behavior of caching content on tab payloads so the Editor
 * can restore Monaco models without re-reading disk. The trade-off: stale
 * content if the file changed on disk while the app was closed. Acceptable
 * for Phase 1 — a future phase can add an async disk re-read on restore.
 */

import type { LayoutNode, TabData } from './types';

/** Current window-state schema version. */
export const WINDOW_STATE_VERSION = 1;

/**
 * On-disk shape of `.knot/window-state.json`.
 *
 * `workspaceFolder` is stored alongside the layout so a stale state file from
 * a different workspace can be detected + rejected on load (defensive — the
 * backend also validates the workspace root).
 */
export interface WindowStateV1 {
  /** Schema version. Must be `1` for this type. */
  version: typeof WINDOW_STATE_VERSION;
  /** Workspace root the layout belongs to. */
  workspaceFolder: string;
  /** The layout tree root. */
  layout: LayoutNode;
}

/**
 * Serialize a layout tree into the on-disk window-state shape.
 *
 * The input is a Svelte 5 `$state` proxy; `JSON.stringify` reads through the
 * proxy and produces a plain object. The output is a fresh object — mutating
 * it does not affect the store.
 *
 * Returns `null` if `root` is `null` (no workspace open — nothing to persist).
 */
export function serializeLayout(
  root: LayoutNode | null,
  workspaceFolder: string,
): WindowStateV1 | null {
  if (!root) return null;
  // JSON round-trip strips Svelte 5 $state proxies → plain object tree.
  // This is the same trick `windowManager.ts:serializeTab` uses for IPC.
  const layoutPlain = JSON.parse(JSON.stringify(root)) as LayoutNode;
  return {
    version: WINDOW_STATE_VERSION,
    workspaceFolder,
    layout: layoutPlain,
  };
}

/**
 * Deserialize an on-disk window-state JSON string into a layout tree.
 *
 * Validates:
 * - The JSON parses.
 * - `version` is `1` (the only supported version).
 * - `workspaceFolder` matches the expected value (defensive — the backend
 *   already validates this, but a stale file from a different workspace
 *   could in theory be copied in).
 * - `layout` is a valid {@link LayoutNode} (basic shape check).
 *
 * Returns `null` if validation fails (caller falls back to default layout).
 * Returns the layout tree on success. The returned tree is a plain object —
 * the store wraps it in `$state` by assigning to `this.root`.
 */
export function deserializeLayout(
  json: string,
  expectedWorkspaceFolder: string,
): LayoutNode | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(json);
  } catch (err) {
    console.warn('[knot:window-state] failed to parse JSON:', err);
    return null;
  }

  if (!isWindowStateV1(parsed)) {
    console.warn('[knot:window-state] unrecognized shape — ignoring saved state');
    return null;
  }

  if (parsed.version !== WINDOW_STATE_VERSION) {
    console.warn(
      '[knot:window-state] unsupported version %s (expected %s) — ignoring',
      parsed.version,
      WINDOW_STATE_VERSION,
    );
    return null;
  }

  if (parsed.workspaceFolder !== expectedWorkspaceFolder) {
    // Defensive: the backend validates workspace root, but if a stale file
    // from a different workspace somehow ends up at this path, don't load it.
    console.warn(
      '[knot:window-state] workspace mismatch (saved=%s, current=%s) — ignoring',
      parsed.workspaceFolder,
      expectedWorkspaceFolder,
    );
    return null;
  }

  if (!isValidLayoutNode(parsed.layout)) {
    console.warn('[knot:window-state] invalid layout node shape — ignoring');
    return null;
  }

  // Deep clone so the caller can wrap in $state without aliasing the parsed
  // object (defensive — JSON.parse already returns fresh objects, but being
  // explicit avoids future surprises if the input source changes).
  return JSON.parse(JSON.stringify(parsed.layout)) as LayoutNode;
}

// ── Type guards (pure) ──────────────────────────────────────────────────

/** Runtime type guard for {@link WindowStateV1}. */
function isWindowStateV1(value: unknown): value is WindowStateV1 {
  if (typeof value !== 'object' || value === null) return false;
  const v = value as Record<string, unknown>;
  return (
    typeof v['version'] === 'number' &&
    typeof v['workspaceFolder'] === 'string' &&
    isValidLayoutNode(v['layout'])
  );
}

/**
 * Validate the shape of a layout node.
 *
 * Checks the discriminated union (`type: 'split' | 'panel'`) and the
 * invariant that split `children.length === sizes.length`. Does NOT deeply
 * validate every tab payload — that would duplicate the type system. The
 * goal is to reject obviously-corrupt files, not to enforce full schema
 * compliance (a corrupted payload will surface as a runtime error in the
 * Editor/FileBrowser, which is acceptable for a local-only state file).
 */
function isValidLayoutNode(value: unknown): boolean {
  if (typeof value !== 'object' || value === null) return false;
  const node = value as Record<string, unknown>;
  if (node['type'] === 'split') {
    const children = node['children'];
    const sizes = node['sizes'];
    if (!Array.isArray(children) || !Array.isArray(sizes)) return false;
    if (children.length !== sizes.length) return false;
    if (node['direction'] !== 'horizontal' && node['direction'] !== 'vertical') return false;
    return children.every((c) => isValidLayoutNode(c));
  }
  if (node['type'] === 'panel') {
    if (typeof node['id'] !== 'string') return false;
    if (!Array.isArray(node['tabs'])) return false;
    // `activeTabId` is `string | null`.
    const active = node['activeTabId'];
    if (active !== null && typeof active !== 'string') return false;
    return node['tabs'].every((t) => isValidTabData(t));
  }
  return false;
}

/** Validate the shape of a single tab. */
function isValidTabData(value: unknown): boolean {
  if (typeof value !== 'object' || value === null) return false;
  const tab = value as Record<string, unknown>;
  return (
    typeof tab['id'] === 'string' &&
    typeof tab['kind'] === 'string' &&
    typeof tab['title'] === 'string'
    // `payload` is `unknown` — accept anything. The rendering component
    // casts it to the expected type; a corrupt payload surfaces there.
  );
}

/**
 * Walk a layout tree and return the set of paths that should remain expanded
 * in the file browser. Used by the store when restoring a filebrowser tab's
 * `expandedPaths` from a freshly-deserialized tree.
 *
 * Pure helper — exported for unit testing.
 */
export function collectExpandedPaths(_root: LayoutNode): string[] {
  // The expandedPaths are stored on the filebrowser tab's payload, so this
  // helper is a no-op placeholder for now. It exists so future
  // restructurings (e.g. moving expand state out of the payload) have a
  // single call site to update.
  return [];
}

/**
 * Sentinel re-export so consumers can reference the version without importing
 * the constant directly. Used by the store's save method.
 */
export type { TabData };
