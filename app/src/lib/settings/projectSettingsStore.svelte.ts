/**
 * Reactive project settings store.
 *
 * Holds the current workspace's project settings (loaded from
 * `.knot/config.json`) in a reactive `$state` so components can react to
 * changes. The most important field is `storyFormat` — the Editor component
 * reads it to set Monaco's model language (SugarCube/Harlowe/Chapbook/Snowman
 * grammar).
 *
 * ## Ownership
 *
 * The store owns **presentation state only** — the settings object + a
 * `loaded` flag. It does NOT own file I/O (that's `projectSettings.ts` —
 * pure async functions). App.svelte calls `load()` on workspace open;
 * SettingsDialog calls `update()` when the user changes a field.
 *
 * ## Svelte 5 runes
 *
 * Uses `$state` — file must be `*.svelte.ts` so the Svelte compiler processes it.
 */

import { DEFAULT_PROJECT_SETTINGS, type ProjectSettings } from './types';
import { loadProjectSettings, saveProjectSettings } from './projectSettings';

class ProjectSettingsStore {
  /** Current settings. Starts with defaults; replaced after `load()`. */
  settings = $state<ProjectSettings>({ ...DEFAULT_PROJECT_SETTINGS });

  /** Whether settings have been loaded for the current workspace. */
  loaded = $state(false);

  /** The workspace root these settings belong to. `null` when no workspace open. */
  workspaceRoot = $state<string | null>(null);

  /**
   * Load project settings from `.knot/config.json` for the given workspace.
   * Call on workspace open (after `set_workspace_root` + migration).
   */
  async load(workspaceRoot: string): Promise<void> {
    this.workspaceRoot = workspaceRoot;
    try {
      this.settings = await loadProjectSettings(workspaceRoot);
      this.loaded = true;
      console.log('[knot:project-settings] loaded:', this.settings.storyFormat);
    } catch (err) {
      console.error('[knot:project-settings] failed to load:', err);
      this.settings = { ...DEFAULT_PROJECT_SETTINGS };
      this.loaded = true;
    }
  }

  /**
   * Save current settings to `.knot/config.json`. Call from the Settings
   * dialog's Save button.
   */
  async save(): Promise<void> {
    if (!this.workspaceRoot) {
      console.warn('[knot:project-settings] cannot save — no workspace root');
      return;
    }
    try {
      await saveProjectSettings(this.workspaceRoot, this.settings);
      console.log('[knot:project-settings] saved:', this.settings.storyFormat);
    } catch (err) {
      console.error('[knot:project-settings] failed to save:', err);
      throw err;
    }
  }

  /**
   * Update the story format. Call when the user changes the format in the
   * Settings dialog. The Editor component reacts to this change and
   * switches the Monaco model language.
   */
  setStoryFormat(format: ProjectSettings['storyFormat']): void {
    this.settings.storyFormat = format;
  }

  /** Reset to defaults (e.g. when the workspace is closed). */
  reset(): void {
    this.settings = { ...DEFAULT_PROJECT_SETTINGS };
    this.loaded = false;
    this.workspaceRoot = null;
  }
}

/** Singleton project settings store. */
export const projectSettingsStore = new ProjectSettingsStore();
