<script lang="ts">
  import { onMount } from 'svelte';

  import IncidentList from '#lib/components/IncidentList.svelte';
  import { describeError, request } from '#lib/api.js';
  import { selectedTenant } from '#lib/tenant.svelte.js';
  import type { Incident } from '#lib/types.js';

  const tenantId = $derived(selectedTenant.current?.id ?? '');

  let filter = $state<'open' | 'all'>('all');
  let incidents = $state<Incident[]>([]);
  let loaded = $state(false);
  let error = $state('');

  async function load() {
    try {
      incidents = await request<Incident[]>('GET', `/v1/incidents?status=${filter}`, {
        tenant: tenantId,
      });
      error = '';
    } catch (err) {
      error = describeError(err);
    } finally {
      loaded = true;
    }
  }

  function show(next: 'open' | 'all') {
    filter = next;
    void load();
  }

  onMount(() => {
    void load();
    const timer = setInterval(load, 10_000);
    return () => clearInterval(timer);
  });

  const tab = 'rounded-lg px-3 py-1 text-sm';
</script>

<svelte:head>
  <title>Incidentes · taut</title>
</svelte:head>

<div class="flex flex-wrap items-end justify-between gap-4">
  <div>
    <h1 class="text-xl font-semibold text-neutral-900">Incidentes</h1>
    <p class="mt-1 text-sm text-neutral-500">
      Se abren cuando una falla dura el periodo de confirmación y se cierran tras el de
      recuperación.
    </p>
  </div>
  <div class="flex gap-1 rounded-lg bg-neutral-100 p-1">
    <button
      class={[tab, filter === 'all' ? 'bg-white shadow-xs' : 'text-neutral-500']}
      onclick={() => show('all')}>Todos</button
    >
    <button
      class={[tab, filter === 'open' ? 'bg-white shadow-xs' : 'text-neutral-500']}
      onclick={() => show('open')}>Abiertos</button
    >
  </div>
</div>

{#if error}
  <p class="mt-6 rounded-lg bg-red-50 px-4 py-3 text-sm text-red-700">{error}</p>
{/if}

<div class="mt-6">
  {#if loaded && incidents.length === 0 && !error}
    <p
      class="rounded-xl border border-dashed border-neutral-300 p-10 text-center text-sm text-neutral-600"
    >
      {filter === 'open'
        ? 'Ningún incidente abierto. Todo en orden.'
        : 'Todavía no hubo incidentes.'}
    </p>
  {:else if incidents.length > 0}
    <IncidentList {incidents} {tenantId} />
  {/if}
</div>
