<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import type { Snippet } from 'svelte';

  import { selectedTenant } from '#lib/tenant.svelte.js';

  let { children }: { children: Snippet } = $props();

  // Every page under (app) acts on a tenant; without one, back to the picker.
  $effect(() => {
    if (!selectedTenant.current) void goto(resolve('/'));
  });

  const links = [
    { href: resolve('/(app)/monitors'), label: 'Monitores' },
    { href: resolve('/(app)/incidents'), label: 'Incidentes' },
    { href: resolve('/(app)/channels'), label: 'Canales' },
  ];

  function switchTenant() {
    selectedTenant.clear();
  }
</script>

{#if selectedTenant.current}
  <div class="min-h-screen bg-neutral-50">
    <header class="border-b border-neutral-200 bg-white">
      <div class="mx-auto flex max-w-5xl flex-wrap items-center gap-x-6 gap-y-2 px-4 py-3">
        <a href={resolve('/(app)/monitors')} class="text-lg font-semibold text-neutral-900">taut</a>
        <nav class="flex gap-4 text-sm">
          {#each links as link (link.href)}
            <a
              href={link.href}
              class={page.url.pathname.startsWith(link.href)
                ? 'font-medium text-neutral-900'
                : 'text-neutral-500 hover:text-neutral-900'}
            >
              {link.label}
            </a>
          {/each}
        </nav>
        <div class="ml-auto flex items-center gap-3 text-sm text-neutral-500">
          <span class="font-medium text-neutral-800">{selectedTenant.current.name}</span>
          <button class="hover:text-neutral-900" onclick={switchTenant}>Cambiar</button>
        </div>
      </div>
    </header>
    <main class="mx-auto max-w-5xl px-4 py-8">
      {@render children()}
    </main>
  </div>
{/if}
