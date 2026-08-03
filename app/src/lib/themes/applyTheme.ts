/**
 * Theme application — CSS variables + Monaco theme + semantic token colors.
 *
 * `applyTheme(theme)` does three things:
 * 1. Sets CSS custom properties on `:root` for app chrome (toolbar, sidebar,
 *    status bar, etc. — anything using `var(--...)` in components).
 * 2. Defines + applies the Monaco editor theme via `monaco.editor.defineTheme`
 *    + `setTheme` — this handles editor chrome (background, foreground,
 *    selection, line numbers, etc.) + TextMate token rules (fallback).
 * 3. Injects semantic token color rules via `editor.semanticTokenColorCustomizations`
 *    in the VS Code configuration service. This is the key mechanism for
 *    semantic token highlighting — the theme service reads this setting +
 *    applies the rules to the LSP's semantic token stream.
 *
 * ## Why `semanticTokenColorCustomizations` (not theme JSON)
 *
 * The VS Code theme JSON format has a `semanticTokenColors` field, but loading
 * it requires registering the theme as an extension contribution + applying
 * via `workbench.colorTheme` — which didn't work reliably (the theme service
 * couldn't find the theme by id, causing a white background fallback).
 *
 * `editor.semanticTokenColorCustomizations` is a user configuration setting
 * that injects semantic token rules into WHATEVER theme is active, regardless
 * of how it was registered. This works with `defineTheme`-registered themes
 * and is the recommended approach for programmatic theme setup in
 * `@codingame/monaco-vscode-api`.
 */

import * as monaco from 'monaco-editor';
import { updateUserConfiguration } from '@codingame/monaco-vscode-configuration-service-override';
import type { Theme } from './themes';

/**
 * Apply a theme to the app: CSS variables + Monaco editor theme + semantic
 * token colors.
 *
 * @param theme The theme to apply.
 */
export function applyTheme(theme: Theme): void {
  // 1. Set CSS variables on :root for app chrome.
  const root = document.documentElement;
  for (const [key, value] of Object.entries(theme.colors)) {
    root.style.setProperty(`--${key}`, value);
  }
  root.setAttribute('data-theme', theme.type);

  // 2. Define + set the Monaco editor theme (basic editor colors).
  registerMonacoTheme(theme);
  monaco.editor.setTheme(theme.monacoName);

  // 3. Inject semantic token color rules via the VS Code configuration service.
  //    This is what makes semantic tokens from the LSP actually get colored.
  //    The rules are an array of { scope, foreground, fontStyle } objects.
  const rules = Object.entries(theme.semanticTokenColors).map(([scope, settings]) => ({
    scope,
    foreground: settings.foreground,
    ...(settings.fontStyle ? { fontStyle: settings.fontStyle } : {}),
  }));
  updateUserConfiguration(JSON.stringify({
    'editor.semanticHighlighting.enabled': true,
    'editor.semanticTokenColorCustomizations': {
      enabled: true,
      rules,
    },
  }));

  console.log('[knot:themes] applied theme:', theme.id, 'with', rules.length, 'semantic token rules');
}

/**
 * Register a Monaco theme via the standalone `defineTheme` API. This handles
 * editor chrome colors (background, foreground, selection, etc.) + TextMate
 * token rules (fallback for when the LSP hasn't indexed yet).
 *
 * Called by `applyTheme` + by `themeStore.init()` (which registers all themes
 * upfront so they're available for switching).
 *
 * @param theme The theme to register.
 */
export function registerMonacoTheme(theme: Theme): void {
  monaco.editor.defineTheme(theme.monacoName, {
    base: theme.type === 'dark' ? 'vs-dark' : 'vs',
    inherit: true,
    rules: theme.monacoRules.map((rule) => ({
      token: rule.scope,
      foreground: rule.settings.foreground?.replace('#', ''),
      fontStyle: rule.settings.fontStyle,
    })),
    colors: {
      'editor.background': theme.colors['bg-editor'],
      'editor.foreground': theme.colors['fg-editor'],
      'editor.lineHighlightBackground': theme.colors['bg-editor-line-highlight'],
      'editor.lineHighlightBorder': '#00000000',
      'editor.selectionBackground': theme.colors['bg-editor-selection'],
      'editorCursor.foreground': theme.colors['fg-editor-cursor'],
      'editorWhitespace.foreground': theme.colors['fg-editor-whitespace'],
      'editorIndentGuide.background': theme.colors['fg-editor-whitespace'],
      'editorLineNumber.foreground': theme.colors['fg-muted'],
      'editorLineNumber.activeForeground': theme.colors['fg-subtle'],
    },
  });
}
