<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';

  import Banner from '#lib/components/Banner.svelte';
  import Button from '#lib/components/Button.svelte';
  import Logo from '#lib/components/Logo.svelte';
  import { API_URL, describeError, request } from '#lib/api.js';
  import { describeHealth, type Health } from '#lib/health.js';
  import { selectedTenant } from '#lib/tenant.svelte.js';
  import type { Tenant } from '#lib/types.js';
  import { fieldError, focusRing, input, label } from '#lib/ui.js';

  let health = $state<Health | null>(null);
  let tenants = $state<Tenant[]>([]);
  let loaded = $state(false);
  let loadError = $state('');

  let name = $state('');
  let slug = $state('');
  let slugTouched = $state(false);
  let createError = $state('');
  let creating = $state(false);

  onMount(async () => {
    try {
      const response = await fetch(`${API_URL}/health`);
      health = response.ok ? ((await response.json()) as Health) : null;
      tenants = await request<Tenant[]>('GET', '/v1/tenants');
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

  function open(tenant: Tenant) {
    selectedTenant.select(tenant);
    void goto(resolve('/(app)/monitors'));
  }

  async function create(event: SubmitEvent) {
    event.preventDefault();
    createError = '';
    creating = true;
    try {
      open(await request<Tenant>('POST', '/v1/tenants', { body: { name, slug } }));
    } catch (error) {
      createError = describeError(error);
    } finally {
      creating = false;
    }
  }
</script>

<svelte:head>
  <title>taut</title>
</svelte:head>

<main class="flex min-h-screen items-center justify-center p-6">
  <section class="w-full max-w-md space-y-6 rounded-xl bg-base p-8 ring ring-hairline">
    <div>
      <Logo class="w-10 text-strong" label="taut" />
      <h1 class="mt-6 text-2xl font-semibold text-strong">Monitoreo de disponibilidad</h1>
      <p class="mt-1 text-subtle">Entra a una organización o crea una nueva.</p>
      <p class="mt-2 text-xs text-subtle">
        {loaded ? describeHealth(health) : 'Comprobando la API…'} · <code>{API_URL}</code>
      </p>
    </div>

    {#if loadError}
      <Banner tone="danger">{loadError}</Banner>
    {/if}

    {#if tenants.length > 0}
      <div>
        <h2 class="font-medium text-default">Entrar a un tenant</h2>
        <ul class="mt-2 divide-y divide-hairline overflow-hidden rounded-lg ring ring-hairline">
          {#each tenants as tenant (tenant.id)}
            <li>
              <button
                class={[
                  'flex w-full cursor-pointer items-center justify-between px-4 py-2.5 text-left hover:bg-tint',
                  focusRing,
                  'focus-visible:-outline-offset-2',
                ]}
                onclick={() => open(tenant)}
              >
                <span class="font-medium text-default">{tenant.name}</span>
                <code class="text-subtle">/{tenant.slug}</code>
              </button>
            </li>
          {/each}
        </ul>
      </div>
    {/if}

    <form class="space-y-4" onsubmit={create}>
      <h2 class="font-medium text-default">Crear un tenant</h2>
      <div>
        <label class={label} for="tenant-name">Nombre de la organización</label>
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
  </section>
</main>
