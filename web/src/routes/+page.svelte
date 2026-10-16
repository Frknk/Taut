<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';

  import Badge from '#lib/components/Badge.svelte';
  import Banner from '#lib/components/Banner.svelte';
  import Button from '#lib/components/Button.svelte';
  import Empty from '#lib/components/Empty.svelte';
  import Logo from '#lib/components/Logo.svelte';
  import { describeError, request } from '#lib/api.js';
  import { ROLE_LABELS } from '#lib/format.js';
  import { session } from '#lib/session.svelte.js';
  import { selectedTenant } from '#lib/tenant.svelte.js';
  import type { TenantWithRole } from '#lib/types.js';
  import { fieldError, focusRing, input, label } from '#lib/ui.js';

  let tenants = $state<TenantWithRole[]>([]);
  let loaded = $state(false);
  let loadError = $state('');

  let name = $state('');
  let slug = $state('');
  let slugTouched = $state(false);
  let createError = $state('');
  let creating = $state(false);
  let showForm = $state(false);

  // Anonymous visitors go to the login page; the (app) pages send users without a tenant here.
  $effect(() => {
    if (session.status === 'anonymous') void goto(resolve('/(auth)/login'));
  });

  onMount(async () => {
    if (session.status !== 'authenticated') return;
    try {
      tenants = await request<TenantWithRole[]>('GET', '/v1/tenants');
    } catch (error) {
      loadError = describeError(error);
    } finally {
      loaded = true;
    }
  });

  /** Lowercase, ASCII, hyphen-separated version of a name. */
  function slugify(text: string): string {
    return text
      .normalize('NFD')
      .replace(/[̀-ͯ]/g, '')
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '')
      .slice(0, 40);
  }

  $effect(() => {
    if (!slugTouched) slug = slugify(name);
  });

  function open(tenant: TenantWithRole) {
    selectedTenant.select(tenant);
    void goto(resolve('/(app)/monitors'));
  }

  async function create(event: SubmitEvent) {
    event.preventDefault();
    createError = '';
    creating = true;
    try {
      open(await request<TenantWithRole>('POST', '/v1/tenants', { body: { name, slug } }));
    } catch (error) {
      createError = describeError(error);
    } finally {
      creating = false;
    }
  }
</script>

<svelte:head>
  <title>Organizaciones · taut</title>
</svelte:head>

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
      <form class="space-y-4" onsubmit={create}>
        <h2 class="font-medium text-default">Crear una organización</h2>
        <div>
          <label class={label} for="tenant-name">Nombre</label>
          <input id="tenant-name" class={input} bind:value={name} required maxlength="100" />
        </div>
        <div>
          <label class={label} for="tenant-slug">Slug (dirección de la página de estado)</label>
          <input
            id="tenant-slug"
            class={[input, 'font-mono']}
            bind:value={slug}
            oninput={() => (slugTouched = true)}
            required
            minlength="3"
            maxlength="40"
            pattern="[a-z0-9][a-z0-9\-]*[a-z0-9]"
          />
        </div>
        {#if createError}<p class={fieldError}>{createError}</p>{/if}
        <Button type="submit" variant="primary" class="w-full" disabled={creating}>
          {creating ? 'Creando…' : 'Crear y entrar'}
        </Button>
      </form>
    {/if}
  </section>
</main>
