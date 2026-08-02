/**
 * Settings type definitions.
 *
 * Two tiers (per phase1-plan.md §3.3):
 * - **EditorSettings** — global, per-user. Stored in `<appData>/settings.json`.
 *   Covers font, tab size, word wrap, minimap, theme, Tweego path.
 * - **ProjectSettings** — per-workspace. Stored in `.knot/config.json` at the
 *   workspace root. Covers story format, build config, include/exclude patterns,
 *   Story Map layout.
 *
 * These interfaces are the source of truth for both the frontend stores and
 * the Rust backend's JSON serialization. The Rust side returns raw JSON
 * strings; the frontend parses into these types.
 */

/** Global editor settings (per-user, cross-workspace). */
export interface EditorSettings {
  /** Monaco font family CSS string. */
  fontFamily: string;
  /** Font size in pixels. */
  fontSize: number;
  /** Number of spaces per tab. */
  tabSize: number;
  /** Word wrap mode: `'on'` wraps at viewport width, `'off'` scrolls. */
  wordWrap: 'on' | 'off';
  /** Whether the minimap (overview ruler) is shown. */
  minimap: boolean;
  /** Whether bracket pairs are colorized. */
  bracketPairColorization: boolean;
  /** Monaco theme id. Task 7 (Themes) will add custom themes; for now it's `'vs-dark'` or `'vs'`. */
  theme: string;
  /** Path to the Tweego executable, or `null` if not configured. */
  tweegoPath: string | null;
}

/** Per-workspace project settings. */
export interface ProjectSettings {
  /** Story format for this project. */
  storyFormat: 'sugarcube' | 'harlowe' | 'chapbook' | 'snowman';
  /** Build configuration (Tweego invocation params). */
  buildConfig: BuildConfig;
  /** Glob patterns for files to include in the build (empty = all). */
  includePatterns: string[];
  /** Glob patterns for files to exclude from the build. */
  excludePatterns: string[];
  /** Default Story Map layout algorithm. */
  storymapLayout: 'hierarchical' | 'force-directed' | 'manual';
}

/** Build configuration within {@link ProjectSettings}. */
export interface BuildConfig {
  /** Output directory (relative to workspace root). */
  outputDir: string;
  /** Output format: `'html'` for a single HTML file, `'zip'` for a bundle. */
  outputFormat: 'html' | 'zip';
  /** Additional Tweego command-line flags. */
  tweegoFlags: string[];
  /** Path to the Tweego executable (overrides the global editor setting). */
  tweegoPath?: string;
}

/** Default editor settings (used when no settings file exists). */
export const DEFAULT_EDITOR_SETTINGS: EditorSettings = {
  fontFamily: 'Consolas, "Courier New", monospace',
  fontSize: 14,
  tabSize: 2,
  wordWrap: 'on',
  minimap: true,
  bracketPairColorization: true,
  theme: 'vs-dark',
  tweegoPath: null,
};

/** Default project settings (used when no config file exists). */
export const DEFAULT_PROJECT_SETTINGS: ProjectSettings = {
  storyFormat: 'sugarcube',
  buildConfig: {
    outputDir: 'build',
    outputFormat: 'html',
    tweegoFlags: [],
  },
  includePatterns: [],
  excludePatterns: [],
  storymapLayout: 'manual',
};
