/**
 * Theme store — reactive, global.
 *
 * Single source of truth for the active theme. Reads the preference from
 * {@link editorSettingsStore} (the `theme` field) on load, applies it via
 * {@link applyTheme}, and re-applies whenever the preference changes.
 *
 * ## Svelte 5 runes
 *
 * Uses `$state` — file must be `*.svelte.ts` so the Svelte compiler
 * processes it.
 */

import { BUILT_IN_THEMES, DEFAULT_THEME_ID, type Theme } from './themes';
import { editorSettingsStore } from '$lib/settings/editorSettings.svelte';
import { applyTheme, registerMonacoTheme } from './applyTheme';

class ThemeStore {
  /** Active theme id (e.g. `'knot-dark'`). Reactive — changes re-apply the theme. */
  activeThemeId = $state<string>(DEFAULT_THEME_ID);

  /** Whether the theme has been applied at least once. */
  applied = $state(false);

  /** Get the active {@link Theme} object, or `null` if not found. */
  get activeTheme(): Theme | null {
    return BUILT_IN_THEMES[this.activeThemeId] ?? null;
  }

  /**
   * Initialize the theme system. Reads the preference from editor settings,
   * registers all Monaco themes, and applies the active theme.
   *
   * Call on app startup AFTER `editorSettingsStore.load()`.
   */
  init(): void {
    // Read the preference from editor settings (stored as the `theme` field).
    const stored = editorSettingsStore.settings.theme;
    if (stored && BUILT_IN_THEMES[stored]) {
      this.activeThemeId = stored;
    } else {
      this.activeThemeId = DEFAULT_THEME_ID;
    }

    // Register all built-in themes with Monaco (so they're available by name).
    for (const theme of Object.values(BUILT_IN_THEMES)) {
      registerMonacoTheme(theme);
    }

    // Apply the active theme (CSS vars + Monaco).
    this.apply();
    this.applied = true;
  }

  /**
   * Set the active theme by id. Applies immediately + persists to editor
   * settings. No-op if the id is not a built-in theme.
   *
   * NOTE: we do NOT use a `$effect` to reactively re-apply on change. `$effect`
   * in a `.svelte.ts` file only works when called synchronously during a
   * component's initialization — `init()` is called from an async `onMount`,
   * which runs outside the reactive effect scope, so the effect would never
   * re-run. Direct calls in `setTheme` are reliable regardless of call site.
   */
  setTheme(themeId: string): void {
    if (!BUILT_IN_THEMES[themeId]) {
      console.warn('[knot:themes] unknown theme id:', themeId);
      return;
    }
    this.activeThemeId = themeId;
    this.apply();
    // Persist the preference to editor settings (fire-and-forget).
    void editorSettingsStore.update('theme', themeId);
  }

  /** Apply the active theme: CSS variables + Monaco theme. */
  private apply(): void {
    const theme = this.activeTheme;
    if (!theme) {
      console.warn('[knot:themes] no active theme to apply');
      return;
    }
    applyTheme(theme);
  }
}

/** Singleton theme store. */
export const themeStore = new ThemeStore();
