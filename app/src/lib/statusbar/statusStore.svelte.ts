/**
 * Reactive status bar store.
 *
 * Single source of truth for everything displayed in the status bar. Any
 * component or backend-event listener mutates state via the setter methods;
 * {@link StatusBar.svelte} reads reactively and re-renders.
 *
 * ## Ownership
 *
 * The store owns **presentation state only** — strings and enums that describe
 * what to show. It does not own document content, LSP client handles, build
 * processes, or file watchers. Those live in their respective modules and push
 * status updates here.
 *
 * ## Svelte 5 runes
 *
 * Fields are declared with `$state` so consumers that read them in template
 * or `$derived` blocks re-run when they change. The store is a class instance
 * exported as a singleton — equivalent to a struct owned by app-level state,
 * matching the "no hidden globals" rule (CONVENTIONS §2.7) by being an
 * explicit, importable, typed dependency.
 */

/** LSP server lifecycle state, mirrored from the backend's supervisor events. */
export type LspStatus =
  | 'idle'
  | 'starting'
  | 'ready'
  | 'restarting'
  | 'failed';

/** Build pipeline state. Phase 1 Task 1 only seeds `idle`; later phases wire real transitions. */
export type BuildStatus = 'idle' | 'building' | 'success' | 'failed';

/** 1-based cursor position reported by Monaco (`onDidChangeCursorPosition`). */
export interface CursorPosition {
  line: number;
  column: number;
}

class StatusStore {
  /** LSP server lifecycle. Drives the "LSP: <status>" item tone + label. */
  lspStatus = $state<LspStatus>('idle');

  /** Human-readable error detail (empty string when no error). */
  lspError = $state<string>('');

  /** Workspace root basename (e.g. `my-game`). Empty before a folder is opened. */
  projectName = $state<string>('');

  /** Tweego version string, or `'not configured'` when not detected. Detection lands in Task 6. */
  tweegoVersion = $state<string>('not configured');

  /** Build pipeline state. */
  buildStatus = $state<BuildStatus>('idle');

  /** Absolute path of the file currently focused in the editor. Empty when no file is open. */
  activeFile = $state<string>('');

  /** Last reported cursor position. Reset to `{1,1}` when the editor has no model. */
  cursorPosition = $state<CursorPosition>({ line: 1, column: 1 });

  /** Language id of the active model (e.g. `'twee'`), displayed as a human label by the bar. */
  languageMode = $state<string>('');

  /** Update availability indicator. Empty string = no indicator shown. Wired in a later phase. */
  updateAvailable = $state<string>('');

  /** Update the LSP status and optionally the error detail. */
  setLspStatus(status: LspStatus, error: string = ''): void {
    this.lspStatus = status;
    this.lspError = error;
  }

  /** Set the project name from the workspace root basename. */
  setProjectName(name: string): void {
    this.projectName = name;
  }

  /** Set the Tweego version string (or `'not configured'`). */
  setTweegoVersion(version: string): void {
    this.tweegoVersion = version;
  }

  /** Set the build status. */
  setBuildStatus(status: BuildStatus): void {
    this.buildStatus = status;
  }

  /** Set the active file path and language mode together (they always change together). */
  setActiveFile(path: string, languageMode: string): void {
    this.activeFile = path;
    this.languageMode = languageMode;
  }

  /** Clear the active file (e.g. when the last editor tab closes). */
  clearActiveFile(): void {
    this.activeFile = '';
    this.languageMode = '';
    this.cursorPosition = { line: 1, column: 1 };
  }

  /** Update the cursor position from a Monaco position event. */
  setCursorPosition(line: number, column: number): void {
    this.cursorPosition = { line, column };
  }

  /** Set the update-available indicator (empty string hides it). */
  setUpdateAvailable(text: string): void {
    this.updateAvailable = text;
  }
}

/** Singleton status store. Imported by {@link StatusBar.svelte} and all status producers. */
export const statusStore = new StatusStore();
