<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';

  import Badge from '#lib/components/Badge.svelte';
  import Banner from '#lib/components/Banner.svelte';
  import Button from '#lib/components/Button.svelte';
  import Empty from '#lib/components/Empty.svelte';
  import Logo from '#lib/components/Logo.svelte';
  import TenantForm from '#lib/components/TenantForm.svelte';
  import { describeError } from '#lib/api.js';
  import { ROLE_LABELS } from '#lib/format.js';
  import { session } from '#lib/session.svelte.js';
  import { pickTenant } from '#lib/tenant-nav.js';
  import { selectedTenant } from '#lib/tenant.svelte.js';
  import type { TenantWithRole } from '#lib/types.js';
  import { focusRing } from '#lib/ui.js';

  let tenants = $state<TenantWithRole[]>([]);
  let loaded = $state(false);
  let loadError = $state('');
  let creating = $state(false);
  let showForm = $state(false);

  // Anonymous visitors go to the login page; the (app) pages send users without a tenant here.
  $effect(() => {
    if (session.status === 'anonymous') void goto(resolve('/(auth)/login'));
  });

  // The page only asks when it has to: the remembered tenant or the only one opens on its own.
  onMount(async () => {
    if (session.status !== 'authenticated') return;
    try {
      tenants = await selectedTenant.refresh();
      const pick = pickTenant(tenants, selectedTenant.current?.id ?? null);
      if (pick) {
        open(pick, true);
        return;
      }
    } catch (error) {
      loadError = describeError(error);
    }
    loaded = true;
  });

  function open(tenant: TenantWithRole, replaceState = false) {
    selectedTenant.select(tenant);
    void goto(resolve('/(app)/monitors'), { replaceState });
  }
</script>

<svelte:head>
  <title>Organizaciones · taut</title>
</svelte:head>

{#if !loaded}
  <div class="flex min-h-screen items-center justify-center text-sm text-subtle" role="status">
    Cargando…
  </div>
{:else}
  <main class="flex min-h-screen items-center justify-center p-6">
    <section class="w-full max-w-md space-y-6 rounded-xl bg-base p-8 ring ring-hairline">
      <div class="flex items-start justify-between gap-4">
        <div>
          <Logo class="w-10 text-strong" label="taut" />
          <h1 class="mt-6 text-2xl font-semibold text-strong">Tus organizaciones</h1>
          <p class="mt-1 text-subtle">Hola, {session.user?.name}. Elige dónde entrar.</p>
        </div>
        <Button variant="ghost" size="sm" onclick={() => void session.logout()}>Salir</Button>
      </div>

      {#if loadError}
        <Banner tone="danger">{loadError}</Banner>
      {/if}

      {#if loaded && tenants.length === 0 && !showForm}
        <Empty
          title="Todavía no perteneces a ninguna organización"
          text="Crea una para empezar a monitorear, o acepta la invitación que te hayan enviado."
        >
          <Button variant="primary" onclick={() => (showForm = true)}>Crear organización</Button>
        </Empty>
      {:else if tenants.length > 0}
        <ul class="divide-y divide-hairline overflow-hidden rounded-lg ring ring-hairline">
          {#each tenants as tenant (tenant.id)}
            <li>
              <button
                class={[
                  'flex w-full cursor-pointer items-center justify-between gap-3 px-4 py-2.5 text-left hover:bg-tint',
                  focusRing,
                  'focus-visible:-outline-offset-2',
                ]}
                onclick={() => open(tenant)}
              >
                <span class="min-w-0">
                  <span class="block truncate font-medium text-default">{tenant.name}</span>
                  <code class="text-xs text-subtle">/{tenant.slug}</code>
                </span>
                <Badge tone={tenant.role === 'owner' ? 'inverted' : 'outline'}>
                  {ROLE_LABELS[tenant.role]}
                </Badge>
              </button>
            </li>
          {/each}
        </ul>
      {/if}

      {#if showForm || tenants.length > 0}
        <div class="space-y-4">
          <h2 class="font-medium text-default">Crear una organización</h2>
          <TenantForm id="new-tenant" bind:creating oncreated={(tenant) => open(tenant)} />
          <Button
            type="submit"
            form="new-tenant"
            variant="primary"
            class="w-full"
            disabled={creating}
          >
            {creating ? 'Creando…' : 'Crear y entrar'}
          </Button>
        </div>
      {/if}
    </section>
  </main>
{/if}
