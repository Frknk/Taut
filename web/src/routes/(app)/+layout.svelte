<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import type { Snippet } from 'svelte';
  import ArrowUpRightIcon from 'phosphor-svelte/lib/ArrowUpRightIcon';

  import Button from '#lib/components/Button.svelte';
  import Logo from '#lib/components/Logo.svelte';
  import { session } from '#lib/session.svelte.js';
  import { selectedTenant } from '#lib/tenant.svelte.js';
  import { focusRing } from '#lib/ui.js';

  let { children }: { children: Snippet } = $props();

  // Every page under (app) needs a session and a tenant: without session, to the login page;
  // without tenant, to the chooser.
  $effect(() => {
    if (session.status === 'anonymous') {
      void goto(`${resolve('/(auth)/login')}?next=${encodeURIComponent(page.url.pathname)}`);
    } else if (!selectedTenant.current) {
      void goto(resolve('/'));
    }
  });

  const links = [
    { href: resolve('/(app)/monitors'), label: 'Monitores' },
    { href: resolve('/(app)/incidents'), label: 'Incidentes' },
    { href: resolve('/(app)/channels'), label: 'Canales' },
    { href: resolve('/(app)/settings/profile'), match: '/settings', label: 'Configuración' },
  ];

  function switchTenant() {
    selectedTenant.clear();
  }

  const navLink = `inline-flex h-8 items-center gap-1 rounded-md px-2.5 text-sm ${focusRing}`;
</script>

{#if session.status === 'authenticated' && selectedTenant.current}
  <div class="min-h-screen">
    <header class="sticky top-0 z-10 border-b border-hairline bg-base">
      <div class="mx-auto flex max-w-5xl flex-wrap items-center gap-x-6 gap-y-2 px-4 py-2">
        <a
          href={resolve('/(app)/monitors')}
          class={['rounded-sm text-strong', focusRing]}
          aria-label="taut, inicio"
        >
          <Logo class="w-6" />
        </a>
        <nav class="flex gap-1" aria-label="Secciones">
          {#each links as link (link.href)}
            {@const current = page.url.pathname.startsWith(link.match ?? link.href)}
            <a
              href={link.href}
              aria-current={current ? 'page' : undefined}
              class={[
                navLink,
                current ? 'bg-tint font-medium text-default' : 'text-subtle hover:text-default',
              ]}
            >
              {link.label}
            </a>
          {/each}
        </nav>
        <div class="ml-auto flex items-center gap-3 text-sm">
          <a
            href={resolve('/status/[slug]', { slug: selectedTenant.current.slug })}
            target="_blank"
            rel="noopener"
            class={[navLink, 'text-subtle hover:text-default']}
          >
            Página de estado
            <ArrowUpRightIcon size={14} aria-hidden="true" />
          </a>
          <span class="font-medium text-default">{selectedTenant.current.name}</span>
          <Button variant="ghost" size="sm" onclick={switchTenant}>Cambiar</Button>
          <span class="text-subtle" aria-label="Usuario">{session.user?.name}</span>
          <Button variant="ghost" size="sm" onclick={() => void session.logout()}>Salir</Button>
        </div>
      </div>
    </header>
    <main class="mx-auto max-w-5xl px-4 py-8">
      {@render children()}
    </main>
  </div>
{/if}
