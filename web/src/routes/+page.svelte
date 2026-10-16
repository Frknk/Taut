<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';

  import { API_URL, describeError, request } from '#lib/api.js';
  import { describeHealth, type Health } from '#lib/health.js';
  import { selectedTenant } from '#lib/tenant.svelte.js';
  import type { Tenant } from '#lib/types.js';

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

  const input =
    'mt-1 block w-full rounded-lg border border-neutral-300 bg-white px-3 py-2 text-sm focus:border-neutral-500 focus:outline-none';
</script>

<svelte:head>
  <title>taut</title>
</svelte:head>

<main class="flex min-h-screen items-center justify-center bg-neutral-100 p-6">
  <section class="w-full max-w-md space-y-6 rounded-xl bg-white p-8 shadow-sm">
    <div>
      <h1 class="text-2xl font-semibold text-neutral-900">taut</h1>
      <p class="mt-1 text-sm text-neutral-500">Monitoreo de disponibilidad multi-tenant</p>
      <p class="mt-2 text-xs text-neutral-400">
        {loaded ? describeHealth(health) : 'Comprobando la API…'} · {API_URL}
      </p>
    </div>

    {#if loadError}
      <p class="rounded-lg bg-red-50 px-4 py-3 text-sm text-red-700">{loadError}</p>
    {/if}

    {#if tenants.length > 0}
      <div>
        <h2 class="text-sm font-medium text-neutral-700">Entrar a un tenant</h2>
        <ul class="mt-2 divide-y divide-neutral-100 rounded-lg border border-neutral-200">
          {#each tenants as tenant (tenant.id)}
            <li>
              <button
                class="flex w-full items-center justify-between px-4 py-2 text-left text-sm hover:bg-neutral-50"
                onclick={() => open(tenant)}
              >
                <span class="font-medium text-neutral-900">{tenant.name}</span>
                <span class="text-neutral-400">/{tenant.slug}</span>
              </button>
            </li>
          {/each}
        </ul>
      </div>
    {/if}

    <form class="space-y-4" onsubmit={create}>
      <h2 class="text-sm font-medium text-neutral-700">Crear un tenant</h2>
      <div>
        <label class="text-xs text-neutral-500" for="tenant-name">Nombre de la organización</label>
        <input id="tenant-name" class={input} bind:value={name} required maxlength="100" />
      </div>
      <div>
        <label class="text-xs text-neutral-500" for="tenant-slug">
          Slug (dirección de la página de estado)
        </label>
        <input
          id="tenant-slug"
          class={input}
          bind:value={slug}
          oninput={() => (slugTouched = true)}
          required
          minlength="3"
          maxlength="40"
          pattern="[a-z0-9][a-z0-9\-]*[a-z0-9]"
        />
      </div>
      {#if createError}<p class="text-sm text-red-600">{createError}</p>{/if}
      <button
        class="w-full rounded-lg bg-neutral-900 px-4 py-2 text-sm font-medium text-white hover:bg-neutral-700 disabled:opacity-50"
        disabled={creating}
      >
        {creating ? 'Creando…' : 'Crear y entrar'}
      </button>
    </form>
  </section>
</main>
