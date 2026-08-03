/**
 * Monaco + monaco-vscode-api initialization.
 *
 * Registers a single `twee` language. Syntax highlighting is provided
 * exclusively by the LSP server's semantic token provider — no TextMate
 * grammars are used. Semantic token colors are applied via the VS Code
 * configuration setting `editor.semanticTokenColorCustomizations`.
 *
 * ## Theme architecture
 *
 * Two layers:
 * 1. **Basic editor colors** (background, foreground, selection, etc.) —
 *    applied via `monaco.editor.defineTheme` + `setTheme` (standalone API).
 *    This works reliably for editor chrome colors.
 * 2. **Semantic token colors** (passageHeader, macro, variable, etc.) —
 *    applied via `editor.semanticTokenColorCustomizations` in the VS Code
 *    configuration service. This setting injects semantic token color rules
 *    into whatever theme is active, regardless of how the theme was
 *    registered. The theme service reads this setting + applies the rules
 *    to the semantic token stream from the LSP.
 *
 * ## Why not extension contribution themes
 *
 * Registering themes as VS Code extension contributions + applying via
 * `workbench.colorTheme` didn't work — the theme service couldn't find the
 * themes by id (the editor fell back to the default light theme, producing
 * a white background). The `semanticTokenColorCustomizations` approach is
 * more robust: it works with any theme (including standalone-defined ones)
 * and doesn't require the theme to be in the extension registry.
 */

import { initialize } from '@codingame/monaco-vscode-api';
import getThemeServiceOverride from '@codingame/monaco-vscode-theme-service-override';
import getLanguagesServiceOverride from '@codingame/monaco-vscode-languages-service-override';
import getTextMateServiceOverride from '@codingame/monaco-vscode-textmate-service-override';
import getExtensionsServiceOverride from '@codingame/monaco-vscode-extensions-service-override';
import getConfigurationServiceOverride, { updateUserConfiguration } from '@codingame/monaco-vscode-configuration-service-override';
import * as monaco from 'monaco-editor';

import { setupMonacoWorkers } from './workers';

/** Twee language id — the single language for all Twine format files. */
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

  // 3. Initialize monaco-vscode-api with service overrides. The configuration
  //    service is needed for `editor.semanticHighlighting.enabled` +
  //    `editor.semanticTokenColorCustomizations` (set in applyTheme.ts).
  await initialize({
    ...getConfigurationServiceOverride(),
    ...getThemeServiceOverride(),
    ...getLanguagesServiceOverride(),
    ...getTextMateServiceOverride(),
    ...getExtensionsServiceOverride(),
  });

  // 4. Enable semantic highlighting globally. The `editor.semanticHighlighting.enabled`
  //    setting must be `true` for Monaco to request + apply semantic tokens
  //    from the LSP. Setting it via `updateUserConfiguration` puts it in the
  //    VS Code configuration registry where the theme service reads it.
  updateUserConfiguration(JSON.stringify({
    'editor.semanticHighlighting.enabled': true,
  }));
}
