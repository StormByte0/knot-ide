<script lang="ts">
  /**
   * Monaco editor Svelte component.
   *
   * Creates a Monaco editor instance bound to a file URI + content. The editor
   * syncs changes to the LSP server via `didOpen` / `didChange` (handled
   * automatically by `monaco-languageclient` once the client is started).
   */

  import { onMount, onDestroy } from 'svelte';
  import * as monaco from 'monaco-editor';
  import { initializeMonaco, TWEE_LANGUAGE_ID } from '$lib/editor/monaco-init';
  import { statusStore } from '$lib/statusbar/statusStore.svelte';
  import { editorStore } from '$lib/editor/editorStore.svelte';

  interface Props {
    /** File URI in `file://` scheme. Required for LSP `didOpen`. */
    uri: string;
    /** Initial file content. */
    content: string;
    /** Language id (defaults to `twee`). */
    language?: string;
  }

  let { uri, content, language = TWEE_LANGUAGE_ID }: Props = $props();

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
      theme: 'vs-dark',
      fontSize: 14,
      minimap: { enabled: true },
      scrollBeyondLastLine: false,
      tabSize: 2,
      wordWrap: 'on',
      lineNumbers: 'on',
      renderWhitespace: 'selection',
      bracketPairColorization: { enabled: true },
    });

    console.log('[knot] Monaco editor created for', uri, 'content length:', content.length);

    // Notify the editor store of content changes (for dirty-state tracking).
    // The store updates the tab's `isDirty` flag + cached content; the
    // tab strip re-renders to show the dirty dot.
    editor.onDidChangeModelContent(() => {
      const value = editor!.getValue();
      // Update the local `content` prop so the parent's $derived stays in
      // sync (used for model-swap comparisons on tab switch).
      content = value;
      if (currentUri) {
        editorStore.markContentChanged(currentUri, value);
      }
    });

    // Push the active file + language to the status store. The URI is the
    // source of truth for "what's being edited"; the language prop tells
    // the status bar which language label to show.
    statusStore.setActiveFile(uri, language);

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

  onDestroy(() => {
    editor?.dispose();
    model?.dispose();
    editor = null;
    model = null;
    // Clear active-file state so the status bar doesn't show a stale file
    // after the editor is destroyed (e.g. user closed the last tab).
    statusStore.clearActiveFile();
  });

  // When the URI changes (different tab activated), swap the Monaco model.
  // The tab's content is read from the editor store so a previously-typed
  // (unsaved) edit is preserved across tab switches.
  $effect(() => {
    // Track uri so the effect re-runs when it changes.
    const targetUri = uri;
    if (!editor || targetUri === currentUri) return;
    currentUri = targetUri;

    console.log('[knot:editor] swapping model to', targetUri);

    const monacoUri = monaco.Uri.parse(targetUri);
    let existing = monaco.editor.getModel(monacoUri);
    // The store holds the latest known content (including unsaved edits).
    const storeTab = editorStore.tabs.find((t) => t.uri === targetUri);
    const expectedContent = storeTab?.content ?? content;
    if (!existing) {
      // Create a new model with the current content.
      existing = monaco.editor.createModel(expectedContent, language, monacoUri);
      console.log('[knot:editor] created new model, content length:', expectedContent.length);
    } else {
      // Update existing model's content if it differs (e.g. file reload).
      if (existing.getValue() !== expectedContent) {
        existing.setValue(expectedContent);
      }
    }
    model = existing;
    editor.setModel(model);

    // Sync the status store with the newly-active file + language.
    statusStore.setActiveFile(targetUri, language);
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
