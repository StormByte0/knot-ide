/**
 * Theme definitions.
 *
 * Two built-in themes adapted from the old VS Code extension's
 * `extensions/vscode/themes/knot-dark.json` + `knot-light.json` (Tokyo
 * Night-inspired palette). The editor colors are preserved; UI chrome colors
 * are derived to match.
 *
 * Each theme provides:
 * - `id` — unique key stored in editor settings
 * - `name` — display name in the Settings/View menu
 * - `type` — `'dark'` or `'light'` (affects Monaco base theme + system chrome)
 * - `monacoName` — Monaco theme name to register + apply
 * - `colors` — flat map of CSS variable name → color value
 * - `monacoRules` — tokenColors for Monaco syntax highlighting (ported from
 *   the old VS Code theme's `tokenColors` array)
 */

/** A complete theme definition. */
export interface Theme {
  /** Unique id, e.g. `'knot-dark'`. Stored in editor settings. */
  id: string;
  /** Display name, e.g. `'Knot Dark'`. */
  name: string;
  /** `'dark'` or `'light'`. */
  type: 'dark' | 'light';
  /** Monaco theme name (registered by applyTheme). */
  monacoName: string;
  /** Flat map of CSS variable name (without `--`) → color value. */
  colors: Record<string, string>;
  /** Monaco token colors (syntax highlighting rules). */
  monacoRules: MonacoTokenRule[];
}

/** A Monaco syntax highlighting rule (matches VS Code's tokenColors format). */
export interface MonacoTokenRule {
  /** Scope selector, e.g. `'entity.name.function.passage.twee'`. */
  scope: string;
  /** Settings for this scope. */
  settings: {
    /** Foreground color. */
    foreground?: string;
    /** Font style, e.g. `'bold'`. */
    fontStyle?: string;
  };
}

/**
 * Knot Dark — adapted from `extensions/vscode/themes/knot-dark.json`.
 *
 * Editor colors preserved verbatim (Tokyo Night palette). UI chrome colors
 * derived to complement the editor background (`#1a1b26`).
 */
export const KNOT_DARK: Theme = {
  id: 'knot-dark',
  name: 'Knot Dark',
  type: 'dark',
  monacoName: 'knot-dark',
  colors: {
    // Editor
    'bg-editor': '#1a1b26',
    'fg-editor': '#c0caf5',
    'bg-editor-line-highlight': '#1f2335',
    'bg-editor-selection': '#33467c',
    'fg-editor-cursor': '#c0caf5',
    'fg-editor-whitespace': '#3b4261',

    // App chrome
    'bg-app': '#1a1b26',
    'bg-toolbar': '#16161e',
    'bg-sidebar': '#16161e',
    'bg-panel': '#1f2335',
    'bg-tab-strip': '#16161e',
    'bg-tab': '#1f2335',
    'bg-tab-active': '#1a1b26',
    'bg-status-bar': '#7aa2f7',
    'bg-input': '#1f2335',
    'bg-hover': '#2a2e44',
    'bg-active-selection': '#33467c',
    'bg-context-menu': '#1f2335',

    // Foreground
    'fg-default': '#c0caf5',
    'fg-muted': '#565f89',
    'fg-subtle': '#9aa5ce',
    'fg-tab': '#9aa5ce',
    'fg-tab-active': '#c0caf5',
    'fg-status-bar': '#ffffff',
    'fg-input': '#c0caf5',
    'fg-context-menu': '#c0caf5',

    // Borders + accents
    'border-default': '#2a2e44',
    'border-subtle': '#1f2335',
    'accent': '#7aa2f7',
    'accent-hover': '#89b4fa',
    'danger': '#f7768e',
    'warning': '#e0af68',
    'success': '#9ece6a',
  },
  monacoRules: [
    { scope: 'entity.name.function.passage.twee', settings: { foreground: '#e0af68', fontStyle: 'bold' } },
    { scope: 'entity.name.passage.twee', settings: { foreground: '#e0af68' } },
    { scope: 'entity.name.tag.passage.twee', settings: { foreground: '#e0af68' } },
    { scope: 'entity.name.function.macro.twee', settings: { foreground: '#7aa2f7' } },
    { scope: 'support.function.macro.twee', settings: { foreground: '#bb9af7' } },
    { scope: 'string.other.link.twee', settings: { foreground: '#73daca' } },
    { scope: 'entity.name.tag.link.twee', settings: { foreground: '#73daca' } },
    { scope: 'string.other.image.twee', settings: { foreground: '#9ece6a' } },
    { scope: 'punctuation.definition.header.twee', settings: { foreground: '#ff9e64' } },
    { scope: 'punctuation.definition.link.begin.twee', settings: { foreground: '#73daca' } },
    { scope: 'punctuation.definition.link.end.twee', settings: { foreground: '#73daca' } },
    { scope: 'punctuation.definition.macro.begin.twee', settings: { foreground: '#7aa2f7' } },
    { scope: 'punctuation.definition.macro.end.twee', settings: { foreground: '#7aa2f7' } },
    { scope: 'meta.tag.twee', settings: { foreground: '#bb9af7' } },
    { scope: 'meta.metadata.twee', settings: { foreground: '#565f89' } },
    { scope: 'comment.block.twee', settings: { foreground: '#565f89' } },
  ],
};

/**
 * Knot Light — adapted from `extensions/vscode/themes/knot-light.json`.
 *
 * Editor colors preserved. UI chrome derived for a light Tokyo Night palette.
 */
export const KNOT_LIGHT: Theme = {
  id: 'knot-light',
  name: 'Knot Light',
  type: 'light',
  monacoName: 'knot-light',
  colors: {
    // Editor
    'bg-editor': '#f5f1eb',
    'fg-editor': '#343b58',
    'bg-editor-line-highlight': '#e8e2d8',
    'bg-editor-selection': '#c9d3e0',
    'fg-editor-cursor': '#343b58',
    'fg-editor-whitespace': '#a9b1c6',

    // App chrome
    'bg-app': '#f5f1eb',
    'bg-toolbar': '#ebe5dc',
    'bg-sidebar': '#ebe5dc',
    'bg-panel': '#e8e2d8',
    'bg-tab-strip': '#ebe5dc',
    'bg-tab': '#e8e2d8',
    'bg-tab-active': '#f5f1eb',
    'bg-status-bar': '#34548a',
    'bg-input': '#ffffff',
    'bg-hover': '#dcd5c8',
    'bg-active-selection': '#c9d3e0',
    'bg-context-menu': '#ffffff',

    // Foreground
    'fg-default': '#343b58',
    'fg-muted': '#9aa5ce',
    'fg-subtle': '#5c6a7e',
    'fg-tab': '#5c6a7e',
    'fg-tab-active': '#343b58',
    'fg-status-bar': '#ffffff',
    'fg-input': '#343b58',
    'fg-context-menu': '#343b58',

    // Borders + accents
    'border-default': '#dcd5c8',
    'border-subtle': '#e8e2d8',
    'accent': '#34548a',
    'accent-hover': '#2d3556',
    'danger': '#8c4351',
    'warning': '#8c5a00',
    'success': '#336f3c',
  },
  monacoRules: [
    { scope: 'entity.name.function.passage.twee', settings: { foreground: '#8c5a00', fontStyle: 'bold' } },
    { scope: 'entity.name.passage.twee', settings: { foreground: '#8c5a00' } },
    { scope: 'entity.name.tag.passage.twee', settings: { foreground: '#8c5a00' } },
    { scope: 'entity.name.function.macro.twee', settings: { foreground: '#34548a' } },
    { scope: 'support.function.macro.twee', settings: { foreground: '#5c6a7e' } },
    { scope: 'string.other.link.twee', settings: { foreground: '#1c6b8c' } },
    { scope: 'entity.name.tag.link.twee', settings: { foreground: '#1c6b8c' } },
    { scope: 'string.other.image.twee', settings: { foreground: '#336f3c' } },
    { scope: 'punctuation.definition.header.twee', settings: { foreground: '#b85c00' } },
    { scope: 'punctuation.definition.link.begin.twee', settings: { foreground: '#1c6b8c' } },
    { scope: 'punctuation.definition.link.end.twee', settings: { foreground: '#1c6b8c' } },
    { scope: 'punctuation.definition.macro.begin.twee', settings: { foreground: '#34548a' } },
    { scope: 'punctuation.definition.macro.end.twee', settings: { foreground: '#34548a' } },
    { scope: 'meta.tag.twee', settings: { foreground: '#5c6a7e' } },
    { scope: 'meta.metadata.twee', settings: { foreground: '#9aa5ce' } },
    { scope: 'comment.block.twee', settings: { foreground: '#9aa5ce' } },
  ],
};

/** All built-in themes, keyed by id. */
export const BUILT_IN_THEMES: Record<string, Theme> = {
  'knot-dark': KNOT_DARK,
  'knot-light': KNOT_LIGHT,
};

/** Default theme id (used when no preference is stored). */
export const DEFAULT_THEME_ID = 'knot-dark';
