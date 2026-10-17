<script lang="ts">
  import { afterNavigate, goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import { Dialog } from 'bits-ui';
  import { onMount, type Snippet } from 'svelte';
  import ListIcon from 'phosphor-svelte/lib/ListIcon';
  import XIcon from 'phosphor-svelte/lib/XIcon';

  import Logo from '#lib/components/Logo.svelte';
  import ReauthDialog from '#lib/components/ReauthDialog.svelte';
  import Sidebar from '#lib/components/Sidebar.svelte';
  import { session } from '#lib/session.svelte.js';
  import { selectedTenant } from '#lib/tenant.svelte.js';
  import { buttonClass, focusRing } from '#lib/ui.js';

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

  // The list feeds the switcher, and corrects a renamed tenant or a changed role.
  onMount(() => {
    selectedTenant.refresh().catch(() => {
      // The switcher retries when it opens; the pages show their own errors.
    });
  });

  // Below `lg` the sidebar is a drawer over the content, opened from the top bar. bits-ui
  // traps the focus, closes it with Escape or a click outside and gives the focus back.
  let drawerOpen = $state(false);

  afterNavigate(() => {
    drawerOpen = false;
  });

  // Past `lg` the drawer is hidden: leaving it open would keep the page scroll locked.
  onMount(() => {
    const wide = window.matchMedia('(min-width: 64rem)');
    const reset = () => {
      if (wide.matches) drawerOpen = false;
    };
    wide.addEventListener('change', reset);
    return () => wide.removeEventListener('change', reset);
  });
</script>

{#if session.status === 'authenticated' && selectedTenant.current}
  <div class="min-h-screen lg:grid lg:grid-cols-[15rem_minmax(0,1fr)]">
    <header
      class="sticky top-0 z-10 flex h-12 items-center gap-2 border-b border-hairline bg-base px-2 lg:hidden"
    >
      <Dialog.Root bind:open={drawerOpen}>
        <Dialog.Trigger class={buttonClass('ghost', 'base', true)} aria-label="Abrir navegación">
          <ListIcon size={18} aria-hidden="true" />
        </Dialog.Trigger>
        <Dialog.Portal>
          <Dialog.Overlay class="fixed inset-0 z-40 bg-scrim lg:hidden" />
          <Dialog.Content
            class={[
              'fixed inset-y-0 left-0 z-40 w-60 border-r border-hairline bg-base shadow-lg lg:hidden',
              'transition-[translate] duration-200 starting:-translate-x-full focus-visible:outline-none',
            ]}
          >
            <Dialog.Title class="sr-only">Navegación</Dialog.Title>
            <Sidebar drawer />
            <!-- After the sidebar: the focus starts on its first link, not on the X. -->
            <Dialog.Close
              class={[buttonClass('ghost', 'sm', true), 'absolute top-3 right-3']}
              aria-label="Cerrar navegación"
            >
              <XIcon size={16} aria-hidden="true" />
            </Dialog.Close>
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>
      <a
        href={resolve('/(app)/monitors')}
        class={['rounded-sm text-strong', focusRing]}
        aria-label="Taut, inicio"
      >
        <Logo class="w-5" />
      </a>
      <span class="min-w-0 truncate font-medium text-default">{selectedTenant.current.name}</span>
    </header>

    <aside
      aria-label="Navegación"
      class="sticky top-0 hidden h-screen w-60 border-r border-hairline bg-base lg:block"
    >
      <Sidebar />
    </aside>

    <main class="mx-auto w-full max-w-5xl px-4 py-8 lg:px-8">
      <!-- A new tenant remounts the page, so it loads that tenant's data. -->
      {#if !selectedTenant.switching}
        {#key selectedTenant.current.id}
          {@render children()}
        {/key}
      {/if}
    </main>
    <ReauthDialog />
  </div>
{/if}
