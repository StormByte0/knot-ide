<script lang="ts">
  /**
   * Single status bar item.
   *
   * Pure presentation: a label prefix, a value, an optional tone for state
   * coloring, and an optional click handler. Owns no state. Reads nothing
   * from the store — the parent ({@link StatusBar.svelte}) decides what to
   * render and passes it down as props.
   *
   * ## Why a separate component
   *
   * Every item in the bar shares the same DOM structure (span + value span),
   * the same hover affordance, and the same tone → class mapping. Centralizing
   * it here keeps {@link StatusBar.svelte} focused on **what to show** rather
   * than **how to render each row** (CONVENTIONS §2.3 — single responsibility).
   */

  /** Visual tone for the value. Maps to a CSS class for color. */
  type Tone = 'default' | 'idle' | 'warning' | 'success' | 'error';

  interface Props {
    /** Prefix label, e.g. `"LSP:"`. Rendered muted before the value. */
    label?: string;
    /** Main value text. */
    value: string;
    /** Tone for the value. Defaults to `'default'` (white). */
    tone?: Tone;
    /** Tooltip on hover. */
    title?: string;
    /** Click handler. When provided, the item renders as a button. */
    onclick?: () => void;
  }

  let { label, value, tone = 'default', title, onclick }: Props = $props();
</script>

{#if onclick}
  <button class="status-item tone-{tone}" {title} {onclick} type="button">
    {#if label}<span class="label">{label}</span>{/if}
    <span class="value">{value}</span>
  </button>
{:else}
  <span class="status-item tone-{tone}" {title}>
    {#if label}<span class="label">{label}</span>{/if}
    <span class="value">{value}</span>
  </span>
{/if}

<style>
  .status-item {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 0 6px;
    height: 100%;
    font-size: 12px;
    color: var(--fg-status-bar);
    background: transparent;
    border: none;
    cursor: default;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  button.status-item {
    cursor: pointer;
  }

  button.status-item:hover {
    background: color-mix(in srgb, var(--fg-status-bar) 12%, transparent);
  }

  .label {
    color: color-mix(in srgb, var(--fg-status-bar) 70%, transparent);
  }

  .value {
    color: var(--fg-status-bar);
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Tone → color. Only the value gets the tone color; the label stays muted.
     Use color-mix with the status bar fg so tones are readable on any bar bg. */
  .tone-default .value { color: var(--fg-status-bar); }
  .tone-idle .value { color: color-mix(in srgb, var(--fg-status-bar) 55%, transparent); }
  .tone-warning .value { color: var(--warning); }
  .tone-success .value { color: var(--success); }
  .tone-error .value { color: var(--danger); }
</style>
