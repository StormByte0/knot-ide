<script lang="ts">
  /**
   * Layout root — recursive renderer for the layout tree.
   *
   * Given a {@link LayoutNode}, renders either:
   * - A {@link SplitView} (for split nodes), which recurses back into this
   *   component for each child.
   * - A {@link DockPanel} (for panel nodes), which renders the tab strip +
   *   active tab content.
   *
   * This is the entry point for the entire layout. `App.svelte` renders a
   * single `<LayoutRoot node={layoutStore.root} />`.
   */

  import SplitView from './SplitView.svelte';
  import DockPanel from './DockPanel.svelte';
  import type { LayoutNode } from './types';

  interface Props {
    /** The node to render. */
    node: LayoutNode;
  }

  let { node }: Props = $props();
</script>

{#if node.type === 'split'}
  <SplitView {node} />
{:else}
  <DockPanel panelId={node.id} />
{/if}
