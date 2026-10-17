<script lang="ts">
  import { DropdownMenu } from 'bits-ui';
  import type { Snippet } from 'svelte';

  import { menu } from '#lib/ui.js';

  interface Props {
    /** A link item: Enter or a click follow it. */
    href?: string;
    /** An action item: runs on Enter, Space or a click, and the menu closes. */
    onSelect?: () => void;
    disabled?: boolean;
    children: Snippet;
  }

  /** One entry of a `Menu`: a link or an action. */
  let { href, onSelect, disabled = false, children }: Props = $props();
</script>

{#if href}
  <DropdownMenu.Item {disabled} {onSelect}>
    {#snippet child({ props })}
      <a {...props} {href} class={menu.item}>{@render children()}</a>
    {/snippet}
  </DropdownMenu.Item>
{:else}
  <DropdownMenu.Item {disabled} {onSelect} class={menu.item}>{@render children()}</DropdownMenu.Item
  >
{/if}
