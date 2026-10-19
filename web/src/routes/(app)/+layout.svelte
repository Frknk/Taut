<script lang="ts">
  import { afterNavigate, goto, onNavigate } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import { Dialog } from 'bits-ui';
  import { onMount, type Snippet } from 'svelte';
  import ListIcon from 'phosphor-svelte/lib/ListIcon';
  import XIcon from 'phosphor-svelte/lib/XIcon';

  import Breadcrumb from '#lib/components/Breadcrumb.svelte';
  import Logo from '#lib/components/Logo.svelte';
  import ReauthDialog from '#lib/components/ReauthDialog.svelte';
  import Sidebar from '#lib/components/Sidebar.svelte';
  import { crumbsFor } from '#lib/breadcrumb.js';
  import { breadcrumb } from '#lib/breadcrumb.svelte.js';
  import { prefersReducedMotion } from '#lib/motion.js';
  import { session } from '#lib/session.svelte.js';
  import { selectedTenant } from '#lib/tenant.svelte.js';
  import { buttonClass, focusRing } from '#lib/ui.js';

  let { children }: { children: Snippet } = $props();

  const crumbs = $derived(crumbsFor(page.url.pathname, breadcrumb.name));

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

  // Moving between sections crossfades the page body (the `content` view transition in
  // layout.css); the sidebar and the top bar stay still. A new query on the same page does not.
  onNavigate((navigation) => {
    if (!document.startViewTransition || prefersReducedMotion()) return;
    if (navigation.from?.route.id === navigation.to?.route.id) return;
    return new Promise((ready) => {
      document.startViewTransition(async () => {
        ready();
        await navigation.complete;
      });
    });
  });

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
  <div
    class="min-h-screen lg:grid lg:grid-cols-[15rem_minmax(0,1fr)] lg:grid-rows-[auto_minmax(0,1fr)]"
  >
    <!-- The top bar: below lg the drawer's button, the mark and the breadcrumb; past lg only the
         breadcrumb, centred on the sidebar's tenant row. -->
    <header
      class="sticky top-0 z-10 flex h-12 items-center gap-2 border-b border-hairline bg-base px-2 lg:col-start-2 lg:h-[72px] lg:bg-canvas lg:px-0"
    >
      <Dialog.Root bind:open={drawerOpen}>
        <Dialog.Trigger
          class={[buttonClass('ghost', 'base', true), 'lg:hidden']}
          aria-label="Abrir navegación"
        >
          <ListIcon size={18} aria-hidden="true" />
        </Dialog.Trigger>
        <Dialog.Portal>
          <Dialog.Overlay
            class="fixed inset-0 z-40 bg-scrim data-[state=closed]:animate-fade-out data-[state=open]:animate-fade-in lg:hidden"
          />
          <Dialog.Content
            class={[
              'fixed inset-y-0 left-0 z-40 w-60 border-r border-hairline bg-base shadow-lg lg:hidden',
              'data-[state=closed]:animate-slide-out-left data-[state=open]:animate-slide-in-left focus-visible:outline-none',
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
        class={['shrink-0 rounded-sm text-strong lg:hidden', focusRing]}
        aria-label="Taut, inicio"
      >
        <Logo class="w-5" />
      </a>
      <div class="min-w-0 flex-1 ps-1 lg:mx-auto lg:max-w-5xl lg:px-8">
        <Breadcrumb {crumbs} />
      </div>
    </header>

    <aside
      aria-label="Navegación"
      class="sticky top-0 hidden h-screen w-60 border-r border-hairline bg-base lg:col-start-1 lg:row-span-2 lg:row-start-1 lg:block"
    >
      <Sidebar />
    </aside>

    <main
      class="mx-auto w-full max-w-5xl px-4 py-8 [view-transition-name:content] lg:col-start-2 lg:px-8"
    >
      <!-- A new tenant remounts the page, so it loads that tenant's data, and fades it in. -->
      {#if !selectedTenant.switching}
        {#key selectedTenant.current.id}
          <div class="animate-fade-in">
            {@render children()}
          </div>
        {/key}
      {/if}
    </main>
    <ReauthDialog />
  </div>
{/if}
