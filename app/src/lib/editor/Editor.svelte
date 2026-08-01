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

    // Notify the Svelte parent of content changes (for dirty-state tracking).
    editor.onDidChangeModelContent(() => {
      content = editor!.getValue();
    });

    currentUri = uri;
  });

  onDestroy(() => {
    editor?.dispose();
    model?.dispose();
    editor = null;
    model = null;
  });

  // When the URI changes (different file opened), swap the model.
  $effect(() => {
    // Track uri so the effect re-runs when it changes.
    const targetUri = uri;
    if (!editor || targetUri === currentUri) return;
    currentUri = targetUri;

    console.log('[knot:editor] swapping model to', targetUri);

    const monacoUri = monaco.Uri.parse(targetUri);
    let existing = monaco.editor.getModel(monacoUri);
    if (!existing) {
      // Create a new model with the current content.
      existing = monaco.editor.createModel(content, language, monacoUri);
      console.log('[knot:editor] created new model, content length:', content.length);
    } else {
      // Update existing model's content if it differs (e.g. file reload).
      if (existing.getValue() !== content) {
        existing.setValue(content);
      }
    }
    model = existing;
    editor.setModel(model);
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
