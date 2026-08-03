/**
 * Editor settings store — reactive, global, per-user.
 *
 * Singleton class with `$state` fields mirroring {@link EditorSettings}. On
 * load, reads from `<appData>/settings.json` via the Rust
 * `load_editor_settings` command. On save, serializes to JSON + calls
 * `save_editor_settings`.
 *
 * ## Reactive Monaco wiring
 *
 * `Editor.svelte` reads `editorStore.settings` in a `$derived` and calls
 * `editor.updateOptions(...)` when any field changes. This makes settings
 * changes apply live without restarting the editor.
 *
 * ## Svelte 5 runes
 *
 * Uses `$state` — file must be `*.svelte.ts` so the Svelte compiler processes it.
 */

import { invoke } from '@tauri-apps/api/core';
import { DEFAULT_EDITOR_SETTINGS, type EditorSettings } from './types';

class EditorSettingsStore {
  /** Current settings. Starts with defaults; replaced after `load()`. */
  settings = $state<EditorSettings>({ ...DEFAULT_EDITOR_SETTINGS });

  /** Whether settings have been loaded from disk. */
  loaded = $state(false);

  /** Whether a save is in progress (for UI feedback). */
  saving = $state(false);

  /** Load settings from `<appData>/settings.json`. Call on app startup. */
  async load(): Promise<void> {
    try {
      const json = await invoke<string>('load_editor_settings');
      const parsed = JSON.parse(json) as Partial<EditorSettings>;
      // Merge with defaults so missing fields get default values (forward-compat).
      this.settings = { ...DEFAULT_EDITOR_SETTINGS, ...parsed };
      this.loaded = true;
      console.log('[knot:settings] editor settings loaded:', this.settings);
    } catch (err) {
      console.error('[knot:settings] failed to load editor settings:', err);
      // Keep defaults on error — don't crash the app.
      this.settings = { ...DEFAULT_EDITOR_SETTINGS };
      this.loaded = true;
    }
  }

  /** Save current settings to `<appData>/settings.json`. */
  async save(): Promise<void> {
    this.saving = true;
    try {
      const json = JSON.stringify(this.settings, null, 2);
      await invoke('save_editor_settings', { json });
      console.log('[knot:settings] editor settings saved');
    } catch (err) {
      console.error('[knot:settings] failed to save editor settings:', err);
      throw err;
    } finally {
      this.saving = false;
    }
  }

  /** Update a single field + auto-save. Convenience for the settings dialog. */
  async update<K extends keyof EditorSettings>(key: K, value: EditorSettings[K]): Promise<void> {
    this.settings[key] = value;
    await this.save();
  }

  /** Detect the Tweego executable path. Returns the path or `null`. */
  async detectTweego(): Promise<string | null> {
    try {
      const result = await invoke<string | null>('detect_tweego');
      if (result) {
        this.settings.tweegoPath = result;
        await this.save();
      }
      return result;
    } catch (err) {
      console.error('[knot:settings] failed to detect tweego:', err);
      return null;
    }
  }

  /**
   * Detect the Tweego version by running `<tweegoPath> --version`. Returns
   * the version string (e.g. `"2.1.1"`) or `null` if the path is unset or
   * the binary can't be executed.
   *
   * Does NOT update the store — the caller (App.svelte) pushes the result to
   * `statusStore.setTweegoVersion` for status-bar display. The version isn't
   * a setting; it's runtime state derived from the configured path.
   */
  async detectTweegoVersion(): Promise<string | null> {
    const path = this.settings.tweegoPath;
    if (!path) return null;
    try {
      return await invoke<string | null>('detect_tweego_version', { tweegoPath: path });
    } catch (err) {
      console.error('[knot:settings] failed to detect tweego version:', err);
      return null;
    }
  }
}

/** Singleton editor settings store. */
export const editorSettingsStore = new EditorSettingsStore();
