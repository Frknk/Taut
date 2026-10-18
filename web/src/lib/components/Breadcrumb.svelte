<script lang="ts">
  import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon';

  import type { Crumb } from '#lib/breadcrumb.js';
  import { focusRing } from '#lib/ui.js';

  /**
   * Where the current page sits, «Monitoreo › Monitores › Caído». On narrow screens only the
   * last two steps show: the parent to go back to and the page itself.
   */
  let { crumbs }: { crumbs: Crumb[] } = $props();
</script>

{#if crumbs.length > 0}
  <nav aria-label="Ruta" class="min-w-0">
    <ol class="flex min-w-0 items-center gap-1.5 text-sm">
      {#each crumbs as crumb, index (index)}
        {@const last = index === crumbs.length - 1}
        <li
          class={[
            'flex min-w-0 items-center gap-1.5',
            index < crumbs.length - 2 && 'max-lg:hidden',
          ]}
        >
          {#if index > 0}
            <CaretRightIcon
              size={12}
              class={['shrink-0 text-subtle', index === crumbs.length - 2 && 'max-lg:hidden']}
              aria-hidden="true"
            />
          {/if}
          {#if last}
            <span class="truncate font-medium text-default" aria-current="page">{crumb.label}</span>
          {:else if crumb.href}
            <a
              href={crumb.href}
              class={['truncate rounded-sm text-subtle hover:text-default', focusRing]}
              >{crumb.label}</a
            >
          {:else}
            <span class="truncate text-subtle">{crumb.label}</span>
          {/if}
        </li>
      {/each}
    </ol>
  </nav>
{/if}
