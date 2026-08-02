<script lang="ts">
  /**
   * Settings dialog — modal with two tabs (Editor / Project).
   *
   * Editor tab edits the global {@link editorSettingsStore} (reactive —
   * changes apply live to Monaco). Project tab edits a local copy of
   * {@link ProjectSettings} and saves on "Save".
   *
   * ## Tabs
   *
   * - **Editor**: font family, font size, tab size, word wrap, minimap,
   *   bracket pair colorization, Tweego path (with "Detect" button).
   * - **Project**: story format, build output dir, output format, Tweego
   *   flags, Story Map layout.
   */

  import { editorSettingsStore } from './editorSettings.svelte';
  import {
    loadProjectSettings,
    saveProjectSettings,
    migrateVscodeConfig,
  } from './projectSettings';
  import { DEFAULT_PROJECT_SETTINGS, type ProjectSettings } from './types';

  interface Props {
    /** Workspace root path (for project settings). `null` if no workspace open. */
    workspaceFolder: string | null;
    /** Called when the dialog is closed (via Cancel, Save, or Escape). */
    onClose: () => void;
  }

  let { workspaceFolder, onClose }: Props = $props();

  /** Active tab: `'editor'` or `'project'`. */
  let activeTab = $state<'editor' | 'project'>('editor');

  /** Local copy of project settings (loaded on mount, saved on "Save"). */
  let projectSettings = $state<ProjectSettings>({ ...DEFAULT_PROJECT_SETTINGS });
  let projectLoaded = $state(false);
  let projectError = $state<string | null>(null);

  // Load project settings on mount (if a workspace is open).
  $effect(() => {
    if (workspaceFolder && !projectLoaded) {
      loadProject();
    }
  });

  async function loadProject(): Promise<void> {
    if (!workspaceFolder) return;
    try {
      // Migrate first (if needed), then load.
      const migrated = await migrateVscodeConfig(workspaceFolder);
      if (migrated) {
        console.log('[knot:settings] migrated .vscode/knot.json → .knot/config.json');
      }
      projectSettings = await loadProjectSettings(workspaceFolder);
      projectLoaded = true;
    } catch (err) {
      projectError = err instanceof Error ? err.message : String(err);
    }
  }

  /** Save editor settings (reactive store — just calls save). */
  async function saveEditor(): Promise<void> {
    await editorSettingsStore.save();
  }

  /** Save project settings + close. */
  async function saveProject(): Promise<void> {
    if (!workspaceFolder) return;
    try {
      await saveProjectSettings(workspaceFolder, projectSettings);
    } catch (err) {
      projectError = err instanceof Error ? err.message : String(err);
      return;
    }
    onClose();
  }

  /** Detect Tweego executable path. */
  async function handleDetectTweego(): Promise<void> {
    const path = await editorSettingsStore.detectTweego();
    if (!path) {
      alert('Tweego was not found on PATH or common install locations. You can set the path manually.');
    }
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="overlay" role="presentation" onclick={onClose}>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.stopPropagation()}
  >
    <div class="dialog-header">
      <h2>Settings</h2>
      <button type="button" class="close-btn" onclick={onClose} aria-label="Close">×</button>
    </div>

    <div class="tab-bar">
      <button
        type="button"
        class="tab-btn"
        class:active={activeTab === 'editor'}
        onclick={() => (activeTab = 'editor')}
      >
        Editor
      </button>
      <button
        type="button"
        class="tab-btn"
        class:active={activeTab === 'project'}
        onclick={() => (activeTab = 'project')}
        disabled={!workspaceFolder}
        title={workspaceFolder ? undefined : 'Open a project folder first'}
      >
        Project
      </button>
    </div>

    <div class="dialog-body">
      {#if activeTab === 'editor'}
        {@render editorTab()}
      {:else}
        {@render projectTab()}
      {/if}
    </div>

    <div class="dialog-footer">
      {#if activeTab === 'editor'}
        <button type="button" class="btn btn-primary" onclick={saveEditor} disabled={editorSettingsStore.saving}>
          {editorSettingsStore.saving ? 'Saving…' : 'Save'}
        </button>
      {:else}
        <button type="button" class="btn btn-primary" onclick={saveProject} disabled={!projectLoaded}>
          Save
        </button>
      {/if}
      <button type="button" class="btn btn-cancel" onclick={onClose}>Close</button>
    </div>
  </div>
</div>

{#snippet editorTab()}
  <div class="settings-grid">
    <label class="setting">
      <span class="setting-label">Font Family</span>
      <input
        type="text"
        class="setting-input"
        value={editorSettingsStore.settings.fontFamily}
        onchange={(e) => editorSettingsStore.update('fontFamily', e.currentTarget.value)}
      />
    </label>

    <label class="setting">
      <span class="setting-label">Font Size</span>
      <input
        type="number"
        class="setting-input"
        min="8"
        max="32"
        value={editorSettingsStore.settings.fontSize}
        onchange={(e) => editorSettingsStore.update('fontSize', Number(e.currentTarget.value))}
      />
    </label>

    <label class="setting">
      <span class="setting-label">Tab Size</span>
      <input
        type="number"
        class="setting-input"
        min="1"
        max="8"
        value={editorSettingsStore.settings.tabSize}
        onchange={(e) => editorSettingsStore.update('tabSize', Number(e.currentTarget.value))}
      />
    </label>

    <label class="setting setting-checkbox">
      <input
        type="checkbox"
        checked={editorSettingsStore.settings.wordWrap === 'on'}
        onchange={(e) => editorSettingsStore.update('wordWrap', e.currentTarget.checked ? 'on' : 'off')}
      />
      <span>Word Wrap</span>
    </label>

    <label class="setting setting-checkbox">
      <input
        type="checkbox"
        checked={editorSettingsStore.settings.minimap}
        onchange={(e) => editorSettingsStore.update('minimap', e.currentTarget.checked)}
      />
      <span>Show Minimap</span>
    </label>

    <label class="setting setting-checkbox">
      <input
        type="checkbox"
        checked={editorSettingsStore.settings.bracketPairColorization}
        onchange={(e) => editorSettingsStore.update('bracketPairColorization', e.currentTarget.checked)}
      />
      <span>Bracket Pair Colorization</span>
    </label>

    <div class="setting">
      <span class="setting-label">Tweego Path</span>
      <div class="setting-row">
        <input
          type="text"
          class="setting-input"
          placeholder="Auto-detect or browse…"
          value={editorSettingsStore.settings.tweegoPath ?? ''}
          onchange={(e) => editorSettingsStore.update('tweegoPath', e.currentTarget.value || null)}
        />
        <button type="button" class="btn btn-small" onclick={handleDetectTweego}>Detect</button>
      </div>
    </div>
  </div>
{/snippet}

{#snippet projectTab()}
  {#if projectError}
    <p class="error">{projectError}</p>
  {:else if !projectLoaded}
    <p class="loading">Loading…</p>
  {:else}
    <div class="settings-grid">
      <label class="setting">
        <span class="setting-label">Story Format</span>
        <select
          class="setting-input"
          value={projectSettings.storyFormat}
          onchange={(e) => (projectSettings.storyFormat = e.currentTarget.value as ProjectSettings['storyFormat'])}
        >
          <option value="sugarcube">SugarCube</option>
          <option value="harlowe">Harlowe</option>
          <option value="chapbook">Chapbook</option>
          <option value="snowman">Snowman</option>
        </select>
      </label>

      <label class="setting">
        <span class="setting-label">Output Directory</span>
        <input
          type="text"
          class="setting-input"
          value={projectSettings.buildConfig.outputDir}
          onchange={(e) => (projectSettings.buildConfig.outputDir = e.currentTarget.value)}
        />
      </label>

      <label class="setting">
        <span class="setting-label">Output Format</span>
        <select
          class="setting-input"
          value={projectSettings.buildConfig.outputFormat}
          onchange={(e) => (projectSettings.buildConfig.outputFormat = e.currentTarget.value as ProjectSettings['buildConfig']['outputFormat'])}
        >
          <option value="html">HTML (single file)</option>
          <option value="zip">ZIP (bundle)</option>
        </select>
      </label>

      <label class="setting">
        <span class="setting-label">Story Map Layout</span>
        <select
          class="setting-input"
          value={projectSettings.storymapLayout}
          onchange={(e) => (projectSettings.storymapLayout = e.currentTarget.value as ProjectSettings['storymapLayout'])}
        >
          <option value="manual">Manual (saved positions)</option>
          <option value="hierarchical">Hierarchical</option>
          <option value="force-directed">Force-Directed</option>
        </select>
      </label>
    </div>
  {/if}
{/snippet}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .dialog {
    background: #252526;
    border: 1px solid #3c3c3c;
    border-radius: 6px;
    min-width: 520px;
    max-width: 600px;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
  }

  .dialog-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-bottom: 1px solid #3c3c3c;
  }

  .dialog-header h2 {
    font-size: 15px;
    font-weight: 600;
    color: #fff;
    margin: 0;
  }

  .close-btn {
    background: none;
    border: none;
    color: #888;
    font-size: 20px;
    cursor: pointer;
    padding: 0 4px;
    line-height: 1;
  }

  .close-btn:hover {
    color: #fff;
  }

  .tab-bar {
    display: flex;
    border-bottom: 1px solid #3c3c3c;
    padding: 0 16px;
  }

  .tab-btn {
    background: none;
    border: none;
    color: #969696;
    padding: 8px 16px;
    cursor: pointer;
    font-size: 13px;
    border-bottom: 2px solid transparent;
    font-family: inherit;
  }

  .tab-btn:hover {
    color: #ccc;
  }

  .tab-btn.active {
    color: #fff;
    border-bottom-color: #007acc;
  }

  .tab-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .dialog-body {
    flex: 1;
    overflow-y: auto;
    padding: 16px;
  }

  .settings-grid {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .setting {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .setting-label {
    font-size: 12px;
    color: #969696;
  }

  .setting-input {
    background: #1e1e1e;
    border: 1px solid #3c3c3c;
    color: #ccc;
    padding: 6px 8px;
    border-radius: 3px;
    font-size: 13px;
    font-family: inherit;
    outline: none;
  }

  .setting-input:focus {
    border-color: #007acc;
  }

  .setting-checkbox {
    flex-direction: row;
    align-items: center;
    gap: 8px;
  }

  .setting-checkbox input {
    accent-color: #007acc;
  }

  .setting-checkbox span {
    font-size: 13px;
    color: #ccc;
  }

  .setting-row {
    display: flex;
    gap: 8px;
  }

  .setting-row .setting-input {
    flex: 1;
  }

  .dialog-footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 16px;
    border-top: 1px solid #3c3c3c;
  }

  .btn {
    padding: 6px 14px;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-size: 13px;
    font-family: inherit;
  }

  .btn-primary {
    background: #0e639c;
    color: white;
  }

  .btn-primary:hover {
    background: #1177bb;
  }

  .btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-cancel {
    background: #3c3c3c;
    color: #ccc;
  }

  .btn-cancel:hover {
    background: #4c4c4c;
  }

  .btn-small {
    background: #3c3c3c;
    color: #ccc;
    padding: 6px 10px;
    font-size: 12px;
  }

  .btn-small:hover {
    background: #4c4c4c;
  }

  .error {
    color: #f48771;
    font-size: 13px;
  }

  .loading {
    color: #888;
    font-size: 13px;
  }
</style>
