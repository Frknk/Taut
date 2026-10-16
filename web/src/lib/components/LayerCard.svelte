<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    /** Section title, rendered as `<h2>` in the card's header row. */
    title?: string;
    /** Actions on the right of the header: ghost, small buttons. */
    actions?: Snippet;
    /** No padding in the panel, for tables and metric rows. */
    flush?: boolean;
    class?: string;
    children: Snippet;
  }

  let { title, actions, flush = false, class: className = '', children }: Props = $props();
</script>

<section
  class={['flex flex-col overflow-hidden rounded-lg bg-elevated ring ring-hairline', className]}
>
  {#if title || actions}
    <div class="flex min-h-11 items-center justify-between gap-3 py-2 pr-2 pl-4">
      {#if title}<h2 class="text-sm font-semibold text-strong">{title}</h2>{/if}
      {#if actions}<div class="flex items-center gap-1">{@render actions()}</div>{/if}
    </div>
  {/if}
  <div class={['min-w-0 rounded-lg bg-base ring ring-fill', flush ? 'overflow-x-auto' : 'p-4']}>
    {@render children()}
  </div>
</section>
