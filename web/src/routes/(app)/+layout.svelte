<script lang="ts">
  import { afterNavigate, goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import { onMount, tick, type Snippet } from 'svelte';
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

  // Below `lg` the sidebar is a drawer over the content, opened from the top bar.
  let drawerOpen = $state(false);
  let openButton = $state<HTMLButtonElement>();
  let closeButton = $state<HTMLButtonElement>();

  async function openDrawer() {
    drawerOpen = true;
    await tick();
    closeButton?.focus();
  }

  async function closeDrawer() {
    if (!drawerOpen) return;
    drawerOpen = false;
    // The top bar stops being inert after the update.
    await tick();
    openButton?.focus();
  }

  afterNavigate(() => {
    drawerOpen = false;
  });

  // Growing past `lg` with the drawer open would leave the content inert.
  onMount(() => {
    const wide = window.matchMedia('(min-width: 64rem)');
    const reset = () => {
      if (wide.matches) drawerOpen = false;
    };
    wide.addEventListener('change', reset);
    return () => wide.removeEventListener('change', reset);
  });

  function keyDown(event: KeyboardEvent) {
    // A menu or dialog inside the sidebar handles its own Escape first.
    if (event.key !== 'Escape' || event.defaultPrevented || !drawerOpen) return;
    // Dialogs are portalled to <body>, so they are not inside the sidebar.
    if (event.target instanceof Element && event.target.closest('[role$="dialog"]')) return;
    void closeDrawer();
  }
</script>

<svelte:window onkeydown={keyDown} />

{#if session.status === 'authenticated' && selectedTenant.current}
  <div class="min-h-screen lg:grid lg:grid-cols-[15rem_minmax(0,1fr)]">
    <header
      class="sticky top-0 z-10 flex h-12 items-center gap-2 border-b border-hairline bg-base px-2 lg:hidden"
      inert={drawerOpen}
    >
      <button
        bind:this={openButton}
        type="button"
        class={buttonClass('ghost', 'base', true)}
        aria-label="Abrir navegación"
        aria-expanded={drawerOpen}
        aria-controls="sidebar"
        onclick={() => void openDrawer()}
      >
        <ListIcon size={18} aria-hidden="true" />
      </button>
      <a
        href={resolve('/(app)/monitors')}
        class={['rounded-sm text-strong', focusRing]}
        aria-label="Taut, inicio"
      >
        <Logo class="w-5" />
      </a>
      <span class="min-w-0 truncate font-medium text-default">{selectedTenant.current.name}</span>
    </header>

    {#if drawerOpen}
      <!-- Pointer shortcut only: the close button and Escape do the same from the keyboard. -->
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="fixed inset-0 z-20 bg-scrim lg:hidden" onclick={() => void closeDrawer()}></div>
    {/if}

    <aside
      id="sidebar"
      aria-label="Navegación"
      class={[
        'w-60 border-r border-hairline bg-base',
        'max-lg:fixed max-lg:inset-y-0 max-lg:left-0 max-lg:z-30 max-lg:shadow-lg',
        // Visibility only animates on close: opening shows the drawer at once, so it can take
        // the focus in the same frame.
        drawerOpen
          ? 'max-lg:translate-x-0 max-lg:transition-[translate] max-lg:duration-200'
          : 'max-lg:invisible max-lg:-translate-x-full max-lg:transition-[translate,visibility] max-lg:duration-200',
        'lg:sticky lg:top-0 lg:h-screen',
      ]}
    >
      <button
        bind:this={closeButton}
        type="button"
        class={[buttonClass('ghost', 'sm', true), 'absolute top-3 right-3 lg:hidden']}
        aria-label="Cerrar navegación"
        onclick={() => void closeDrawer()}
      >
        <XIcon size={16} aria-hidden="true" />
      </button>
      <Sidebar />
    </aside>

    <main class="mx-auto w-full max-w-5xl px-4 py-8 lg:px-8" inert={drawerOpen}>
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
