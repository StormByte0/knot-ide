/**
 * Monaco + monaco-vscode-api initialization.
 *
 * `initialize()` from `@codingame/monaco-vscode-api` MUST be called exactly
 * once per JS context, before any Monaco editor is created. This sets up VS
 * Code's services (textmate, theme, languages, extensions) inside Monaco.
 *
 * For the spike, we register a minimal Twee language and a SugarCube TextMate
 * grammar. The full 5-grammar set (Twee, SugarCube, Harlowe, Chapbook, Snowman)
 * lands in Phase 2.
 */

import { initialize } from '@codingame/monaco-vscode-api';
import getThemeServiceOverride from '@codingame/monaco-vscode-theme-service-override';
import getLanguagesServiceOverride from '@codingame/monaco-vscode-languages-service-override';
import getTextMateServiceOverride from '@codingame/monaco-vscode-textmate-service-override';
import getExtensionsServiceOverride from '@codingame/monaco-vscode-extensions-service-override';
import { registerExtension, ExtensionHostKind } from '@codingame/monaco-vscode-api/extensions';
import * as monaco from 'monaco-editor';

import { setupMonacoWorkers } from './workers';

/** Twee language id used across Monaco and the LSP client. */
export const TWEE_LANGUAGE_ID = 'twee';

/** Whether `initializeMonaco()` has already run. */
let initialized = false;

/**
 * Initialize Monaco + VS Code services. Safe to call multiple times — only
 * the first call has effect.
 */
export async function initializeMonaco(): Promise<void> {
  if (initialized) return;
  initialized = true;

  // 1. Set up worker resolution before any editor or service starts.
  setupMonacoWorkers();

  // 2. Register the Twee language with Monaco.
  monaco.languages.register({ id: TWEE_LANGUAGE_ID, extensions: ['.tw', '.twee'] });
  monaco.languages.setLanguageConfiguration(TWEE_LANGUAGE_ID, {
    comments: { lineComment: '::%' },
    brackets: [
      ['[[', ']]'],
      ['<<', '>>'],
      ['{', '}'],
      ['(', ')'],
    ],
    autoClosingPairs: [
      { open: '[[', close: ']]' },
      { open: '<<', close: '>>' },
      { open: '{', close: '}' },
      { open: '(', close: ')' },
      { open: '"', close: '"' },
      { open: "'", close: "'" },
    ],
  });

  // 3. Initialize monaco-vscode-api with service overrides.
  await initialize({
    ...getThemeServiceOverride(),
    ...getLanguagesServiceOverride(),
    ...getTextMateServiceOverride(),
    ...getExtensionsServiceOverride(),
  });

  // 4. Register the SugarCube TextMate grammar as a VS Code extension
  //    contribution. This is how monaco-vscode-api loads custom grammars.
  registerTweeGrammar();
}

/**
 * Register a minimal Twee/SugarCube TextMate grammar via the extensions
 * service. The grammar is registered as a local-process extension with an
 * inline virtual file.
 */
function registerTweeGrammar(): void {
  const result = registerExtension(
    {
      name: 'knot-twee',
      publisher: 'knot',
      version: '0.1.0',
      engines: { vscode: '*' },
      contributes: {
        languages: [
          {
            id: TWEE_LANGUAGE_ID,
            aliases: ['Twee', 'Twine'],
            extensions: ['.tw', '.twee'],
          },
        ],
        grammars: [
          {
            language: TWEE_LANGUAGE_ID,
            scopeName: 'source.twee',
            path: './syntaxes/twee.tmLanguage.json',
          },
        ],
      },
    },
    ExtensionHostKind.LocalProcess,
  );

  // Register the grammar file content with the extension's virtual FS.
  // `registerFileUrl` expects a URL; we create a data URL from the JSON.
  const grammarUrl = `data:application/json;base64,${btoa(JSON.stringify(TWEE_GRAMMAR))}`;
  result.registerFileUrl('./syntaxes/twee.tmLanguage.json', grammarUrl);
}

/**
 * Minimal Twee/SugarCube TextMate grammar for the spike.
 *
 * Phase 2 will replace this with the full grammar derived from
 * `crates/formats/src/sugarcube/lsp/token_builder.rs`.
 */
const TWEE_GRAMMAR = {
  scopeName: 'source.twee',
  patterns: [
    {
      name: 'meta.header.twee',
      match: '^(::)\\s*(\\S[^\\[\\{]*?)\\s*(\\[[^\\]]*\\])?\\s*(\\{[^\\}]*\\})?\\s*$',
      captures: {
        '1': { name: 'punctuation.definition.header.twee' },
        '2': { name: 'entity.name.section.twee' },
        '3': { name: 'meta.tag.twee' },
        '4': { name: 'meta.metadata.twee' },
      },
    },
    {
      name: 'meta.link.twee',
      match: '(\\[\\[)([^\\]\\[]+?)(->|\\|)([^\\]\\[]+?)(\\]\\])',
      captures: {
        '1': { name: 'punctuation.definition.link.begin.twee' },
        '2': { name: 'string.other.link.twee' },
        '3': { name: 'punctuation.separator.link.twee' },
        '4': { name: 'entity.name.tag.link.twee' },
        '5': { name: 'punctuation.definition.link.end.twee' },
      },
    },
    {
      name: 'meta.link.simple.twee',
      match: '(\\[\\[)([^\\]\\[]+?)(\\]\\])',
      captures: {
        '1': { name: 'punctuation.definition.link.begin.twee' },
        '2': { name: 'entity.name.tag.link.twee' },
        '3': { name: 'punctuation.definition.link.end.twee' },
      },
    },
    {
      name: 'meta.image.twee',
      match: '(\\[img\\[)([^\\]]+?)(\\]\\])',
      captures: {
        '1': { name: 'punctuation.definition.image.begin.twee' },
        '2': { name: 'string.other.image.twee' },
        '3': { name: 'punctuation.definition.image.end.twee' },
      },
    },
    {
      name: 'meta.macro.inline.twee',
      match: '(<<)([a-zA-Z][\\w]*)([\\s\\S]*?)(>>)',
      captures: {
        '1': { name: 'punctuation.definition.macro.begin.twee' },
        '2': { name: 'entity.name.function.macro.twee' },
        '4': { name: 'punctuation.definition.macro.end.twee' },
      },
    },
    {
      name: 'comment.block.twee',
      begin: '/%',
      end: '%/',
      captures: {
        '0': { name: 'punctuation.definition.comment.twee' },
      },
    },
  ],
  repository: {},
};
