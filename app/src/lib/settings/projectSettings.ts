/**
 * Project settings — per-workspace, stored in `.knot/config.json`.
 *
 * Unlike editor settings (which use a reactive store because they drive
 * Monaco options live), project settings are loaded once on workspace open
 * and saved on explicit user action (the Settings dialog). No reactive
 * store needed — just async load/save functions.
 */

import { invoke } from '@tauri-apps/api/core';
import { DEFAULT_PROJECT_SETTINGS, type ProjectSettings } from './types';

/**
 * Load project settings from `.knot/config.json` at the workspace root.
 * Returns defaults if the file doesn't exist.
 */
export async function loadProjectSettings(workspaceRoot: string): Promise<ProjectSettings> {
  try {
    const json = await invoke<string>('load_project_settings', { workspaceRoot });
    const parsed = JSON.parse(json) as Partial<ProjectSettings>;
    // Merge with defaults so missing fields get default values.
    return { ...DEFAULT_PROJECT_SETTINGS, ...parsed };
  } catch (err) {
    console.error('[knot:settings] failed to load project settings:', err);
    return { ...DEFAULT_PROJECT_SETTINGS };
  }
}

/**
 * Save project settings to `.knot/config.json` at the workspace root.
 * Creates the `.knot/` directory if it doesn't exist.
 */
export async function saveProjectSettings(
  workspaceRoot: string,
  settings: ProjectSettings,
): Promise<void> {
  const json = JSON.stringify(settings, null, 2);
  await invoke('save_project_settings', { workspaceRoot, json });
  console.log('[knot:settings] project settings saved');
}

/**
 * Migrate `.vscode/knot.json` → `.knot/config.json` if the old file exists
 * and the new one doesn't. Call on workspace open (before loading project
 * settings).
 *
 * Returns `true` if migration was performed, `false` otherwise.
 */
export async function migrateVscodeConfig(workspaceRoot: string): Promise<boolean> {
  try {
    return await invoke<boolean>('migrate_vscode_config', { workspaceRoot });
  } catch (err) {
    console.error('[knot:settings] migration failed:', err);
    return false;
  }
}
