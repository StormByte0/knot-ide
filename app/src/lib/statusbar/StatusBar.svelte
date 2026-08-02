<script lang="ts">
  /**
   * Status bar — assembles all status items.
   *
   * Reads presentation state from {@link statusStore} and renders a row of
   * {@link StatusItem} components. Owns no state itself; it is a pure
   * projection of the store onto the DOM.
   *
   * ## Layout
   *
   * Two groups separated by `justify-content: space-between`:
   * - **Left:** project name, LSP status, LSP error, build status
   * - **Right:** active file, cursor position, language mode, Tweego version, update indicator
   *
   * This matches VS Code's status bar convention (project/navigation on the
   * left, file context on the right).
   *
   * ## Tones
   *
   * LSP and build statuses map their enum value to a tone via {@link lspTone}
   * and {@link buildTone}. The tone drives the value's color through
   * {@link StatusItem.svelte}'s CSS classes.
   */

  import StatusItem from './StatusItem.svelte';
  import { statusStore, type LspStatus, type BuildStatus } from './statusStore.svelte';

  /** Map LSP status to a tone for {@link StatusItem}. */
  function lspTone(status: LspStatus): 'idle' | 'warning' | 'success' | 'error' {
    switch (status) {
      case 'idle': return 'idle';
      case 'starting':
      case 'restarting': return 'warning';
      case 'ready': return 'success';
      case 'failed': return 'error';
    }
  }

  /** Map build status to a tone for {@link StatusItem}. */
  function buildTone(status: BuildStatus): 'idle' | 'warning' | 'success' | 'error' {
    switch (status) {
      case 'idle': return 'idle';
      case 'building': return 'warning';
      case 'success': return 'success';
      case 'failed': return 'error';
    }
  }

  /** Capitalize a language id for display: `twee` → `Twee`. */
  function displayLanguage(id: string): string {
    if (!id) return '';
    return id.charAt(0).toUpperCase() + id.slice(1);
  }

  /** Extract the basename from an absolute path (cross-platform). */
  function basename(path: string): string {
    if (!path) return '';
    const parts = path.split(/[/\\]/);
    return parts[parts.length - 1] || path;
  }

  /** Format cursor position as `Ln <line>, Col <col>`. */
  function formatCursor(line: number, column: number): string {
    return `Ln ${line}, Col ${column}`;
  }

  // Reactive reads from the store. Svelte 5 runes re-run these getters when
  // the underlying `$state` fields change.
  let projectName = $derived(statusStore.projectName);
  let lspStatus = $derived(statusStore.lspStatus);
  let lspError = $derived(statusStore.lspError);
  let buildStatus = $derived(statusStore.buildStatus);
  let activeFile = $derived(statusStore.activeFile);
  let cursor = $derived(statusStore.cursorPosition);
  let languageMode = $derived(statusStore.languageMode);
  let tweegoVersion = $derived(statusStore.tweegoVersion);
  let updateAvailable = $derived(statusStore.updateAvailable);

  let lspToneValue = $derived(lspTone(lspStatus));
  let buildToneValue = $derived(buildTone(buildStatus));
  let languageLabel = $derived(displayLanguage(languageMode));
  let fileLabel = $derived(basename(activeFile));
  let cursorLabel = $derived(formatCursor(cursor.line, cursor.column));
</script>

<footer class="status-bar">
  <div class="group group-left">
    <StatusItem
      label="Knot"
      value={projectName || 'No folder'}
      tone={projectName ? 'default' : 'idle'}
      title={projectName || 'No workspace folder open'}
    />
    <StatusItem
      label="LSP:"
      value={lspStatus}
      tone={lspToneValue}
      title={lspError || `knot-server: ${lspStatus}`}
    />
    {#if lspError}
      <StatusItem value={lspError} tone="error" />
    {/if}
    <StatusItem
      label="Build:"
      value={buildStatus}
      tone={buildToneValue}
      title={`Build status: ${buildStatus}`}
    />
  </div>

  <div class="group group-right">
    {#if fileLabel}
      <StatusItem value={fileLabel} title={activeFile} />
      <StatusItem value={cursorLabel} tone="idle" />
      <StatusItem value={languageLabel} tone="idle" />
    {/if}
    <StatusItem
      label="Tweego:"
      value={tweegoVersion}
      tone={tweegoVersion === 'not configured' ? 'idle' : 'default'}
      title={tweegoVersion === 'not configured'
        ? 'Tweego not detected. Configure in Settings (Task 6).'
        : `Tweego ${tweegoVersion}`}
    />
    {#if updateAvailable}
      <StatusItem value={updateAvailable} tone="warning" />
    {/if}
  </div>
</footer>

<style>
  .status-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 0 8px;
    height: 26px;
    background: var(--bg-status-bar);
    color: var(--fg-status-bar);
    flex-shrink: 0;
    overflow: hidden;
  }

  .group {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    overflow: hidden;
  }

  .group-right {
    justify-content: flex-end;
  }
</style>
