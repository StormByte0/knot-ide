<script lang="ts">
  /**
   * Monaco editor Svelte component.
   *
   * Creates a Monaco editor instance bound to a file URI + content. The editor
   * syncs changes to the LSP server via `didOpen` / `didChange` (handled
   * automatically by `monaco-languageclient` once the client is started).
   *
   * ## Task 3 integration
   *
   * The Editor is now rendered by {@link DockPanel.svelte} when the active tab
   * is `kind: 'editor'`. Props come from the tab's payload. Content changes
   * are pushed to {@link layoutStore.markTabContentChanged} so the tab's
   * dirty flag + cached content stay in sync.
   */

  import { onMount, onDestroy } from 'svelte';
  import * as monaco from 'monaco-editor';
  import { initializeMonaco, TWEE_LANGUAGE_ID } from '$lib/editor/monaco-init';
  import { statusStore } from '$lib/statusbar/statusStore.svelte';
  import { layoutStore } from '$lib/layout/layoutStore.svelte';
  import { editorSettingsStore } from '$lib/settings/editorSettings.svelte';

  interface Props {
    /** Tab id (=== file path). Used for layout-store content tracking. */
    tabId: string;
    /** File URI in `file://` scheme. Required for LSP `didOpen`. */
    uri: string;
    /** Initial file content. */
    content: string;
    /** Language id (defaults to `twee`). */
    language?: string;
  }

  let { tabId, uri, content, language = TWEE_LANGUAGE_ID }: Props = $props();

  let container: HTMLDivElement;
  let editor: monaco.editor.IStandaloneCodeEditor | null = null;
  let model: monaco.editor.ITextModel | null = null;
  let currentUri = $state<string | null>(null);

  onMount(async () => {
    // Ensure monaco-vscode-api is initialized before creating any editor.
    await initializeMonaco();

    const monacoUri = monaco.Uri.parse(uri);
    model =
      monaco.editor.getModel(monacoUri) ??
      monaco.editor.createModel(content, language, monacoUri);

    editor = monaco.editor.create(container, {
      model,
      automaticLayout: true,
      theme: editorSettingsStore.settings.theme,
      fontFamily: editorSettingsStore.settings.fontFamily,
      fontSize: editorSettingsStore.settings.fontSize,
      minimap: { enabled: editorSettingsStore.settings.minimap },
      scrollBeyondLastLine: false,
      tabSize: editorSettingsStore.settings.tabSize,
      wordWrap: editorSettingsStore.settings.wordWrap,
      lineNumbers: 'on',
      renderWhitespace: 'selection',
      bracketPairColorization: { enabled: editorSettingsStore.settings.bracketPairColorization },
      // CRITICAL: enable semantic highlighting. Without this, Monaco computes
      // semantic tokens from the LSP but doesn't apply the theme's
      // `semanticTokenColors` rules — the tokens are silently ignored.
      // `configuredByTheme` respects the theme's `semanticHighlighting: true`
      // flag (set in our theme JSON via `toThemeJson`).
      'semanticHighlighting.enabled': 'configuredByTheme',
    });

    console.log('[knot] Monaco editor created for', uri, 'content length:', content.length);

    // Notify the layout store of content changes (for dirty-state tracking).
    // The store updates the tab's `isDirty` flag + cached content; the tab
    // strip re-renders to show the dirty dot.
    editor.onDidChangeModelContent(() => {
      const value = editor!.getValue();
      content = value;
      layoutStore.markTabContentChanged(tabId, value);
    });

    // Push the active file path + language to the status store. The path
    // (tabId) is what the status bar and file browser use for display /
    // highlighting.
    statusStore.setActiveFile(tabId, language);

    // Push the initial cursor position (1:1 on a fresh model) and subscribe
    // to future moves so the status bar tracks line:col live.
    const pos = editor.getPosition();
    if (pos) {
      statusStore.setCursorPosition(pos.lineNumber, pos.column);
    }
    editor.onDidChangeCursorPosition((e) => {
      statusStore.setCursorPosition(e.position.lineNumber, e.position.column);
    });

    currentUri = uri;
  });

  // Reactively apply editor settings when they change (e.g. user changed
  // font size in the Settings dialog). $effect re-runs whenever any
  // settingsStore.settings field read inside it changes.
  //
  // NOTE: `theme` is intentionally NOT applied here. Theme switching is owned
  // exclusively by `applyTheme.ts` (which calls `monaco.editor.setTheme`).
  // Having two paths race (this $effect + applyTheme) caused flicker on
  // theme switch. The `theme` setting field still triggers reactivity (so
  // this effect re-runs when the theme changes), but we don't pass it to
  // `updateOptions` — the applyTheme call handles the Monaco theme swap.
  // See PLAN.md §13.7.
  $effect(() => {
    if (!editor) return;
    const s = editorSettingsStore.settings;
    editor.updateOptions({
      fontFamily: s.fontFamily,
      fontSize: s.fontSize,
      minimap: { enabled: s.minimap },
      tabSize: s.tabSize,
      wordWrap: s.wordWrap,
      bracketPairColorization: { enabled: s.bracketPairColorization },
      'semanticHighlighting.enabled': 'configuredByTheme',
    });
  });

  onDestroy(() => {
    editor?.dispose();
    model?.dispose();
    editor = null;
    model = null;
    // Clear active-file state so the status bar doesn't show a stale file
    // after the editor is destroyed (e.g. user closed the last tab).
    // TODO: when multiple editor panels exist (Task 5), only clear if this
    // was the active editor. For now there's at most one editor panel.
    statusStore.clearActiveFile();
  });

  // When the URI changes (different tab activated), swap the Monaco model.
  // The `content` prop is passed by DockPanel from the tab's payload, so
  // unsaved edits (tracked in the layout store) are preserved across tab
  // switches.
  $effect(() => {
    const targetUri = uri;
    if (!editor || targetUri === currentUri) return;
    currentUri = targetUri;

    console.log('[knot:editor] swapping model to', targetUri);

    const monacoUri = monaco.Uri.parse(targetUri);
    let existing = monaco.editor.getModel(monacoUri);
    if (!existing) {
      existing = monaco.editor.createModel(content, language, monacoUri);
      console.log('[knot:editor] created new model, content length:', content.length);
    } else {
      if (existing.getValue() !== content) {
        existing.setValue(content);
      }
    }
    model = existing;
    editor.setModel(model);

    // Sync the status store with the newly-active file + language.
    statusStore.setActiveFile(tabId, language);
    const pos = editor.getPosition();
    if (pos) {
      statusStore.setCursorPosition(pos.lineNumber, pos.column);
    }
  });
</script>

<div bind:this={container} class="editor-container"></div>

<style>
  .editor-container {
    width: 100%;
    height: 100%;
    position: absolute;
    top: 0;
    left: 0;
  }
</style>
