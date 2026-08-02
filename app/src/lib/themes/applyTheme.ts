/**
 * Theme application — CSS variables + Monaco theme registration.
 *
 * `applyTheme(theme)` does two things:
 * 1. Sets CSS custom properties on `:root` (e.g. `--bg-editor`, `--fg-default`)
 *    so all components using `var(--...)` update reactively.
 * 2. Calls `monaco.editor.setTheme(theme.monacoName)` to switch Monaco's theme.
 *
 * `registerMonacoTheme(theme)` defines a Monaco theme (colors + token rules)
 * so it can be referenced by name. Called once per theme at startup.
 */

import * as monaco from 'monaco-editor';
import type { Theme } from './themes';

/**
 * Apply a theme to the app: set CSS variables + switch Monaco theme.
 *
 * @param theme The theme to apply.
 */
export function applyTheme(theme: Theme): void {
  // 1. Set CSS variables on :root.
  const root = document.documentElement;
  for (const [key, value] of Object.entries(theme.colors)) {
    root.style.setProperty(`--${key}`, value);
  }
  // Set a data attribute so components can branch on light/dark if needed
  // (e.g. for scrollbars, which can't use CSS vars in all browsers).
  root.setAttribute('data-theme', theme.type);

  // 2. Switch Monaco theme.
  monaco.editor.setTheme(theme.monacoName);

  console.log('[knot:themes] applied theme:', theme.id);
}

/**
 * Register a Monaco theme by name. Safe to call multiple times — Monaco
 * replaces the existing theme definition.
 *
 * @param theme The theme to register.
 */
export function registerMonacoTheme(theme: Theme): void {
  // Build the Monaco theme data structure.
  const themeData: monaco.editor.IStandaloneThemeData = {
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
      // Explicitly disable the line highlight border — the inherited vs-dark/vs
      // base theme sets one that hurts readability on dark backgrounds.
      'editor.lineHighlightBorder': '#00000000',
      'editor.selectionBackground': theme.colors['bg-editor-selection'],
      'editorCursor.foreground': theme.colors['fg-editor-cursor'],
      'editorWhitespace.foreground': theme.colors['fg-editor-whitespace'],
      'editorIndentGuide.background': theme.colors['fg-editor-whitespace'],
      // Line numbers: muted version of the editor foreground so they don't
      // compete with the code for attention.
      'editorLineNumber.foreground': theme.colors['fg-muted'],
      'editorLineNumber.activeForeground': theme.colors['fg-subtle'],
    },
  };

  monaco.editor.defineTheme(theme.monacoName, themeData);
  console.log('[knot:themes] registered Monaco theme:', theme.monacoName);
}
