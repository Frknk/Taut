<script lang="ts">
  import { onMount } from 'svelte';

  import Banner from '#lib/components/Banner.svelte';
  import Empty from '#lib/components/Empty.svelte';
  import IncidentList from '#lib/components/IncidentList.svelte';
  import TabList from '#lib/components/TabList.svelte';
  import TabPanel from '#lib/components/TabPanel.svelte';
  import Tabs from '#lib/components/Tabs.svelte';
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

  const FILTERS: { value: 'open' | 'all'; label: string }[] = [
    { value: 'all', label: 'Todos' },
    { value: 'open', label: 'Abiertos' },
  ];

  onMount(() => {
    void load();
    const timer = setInterval(load, 10_000);
    return () => clearInterval(timer);
  });
</script>

<svelte:head>
  <title>Incidentes · Taut</title>
</svelte:head>

<Tabs bind:value={filter} onchange={() => void load()}>
  <div class="flex flex-wrap items-end justify-between gap-4">
    <div>
      <h1 class="text-2xl font-semibold text-strong">Incidentes</h1>
      <p class="mt-0.5 max-w-prose text-[13px]/[18px] text-subtle">
        Se abren cuando una falla dura el periodo de confirmación y se cierran tras el de
        recuperación.
      </p>
    </div>
    <TabList options={FILTERS} label="Filtro" />
  </div>

  {#if error}
    <Banner tone="danger" class="mt-6">{error}</Banner>
  {/if}

  <TabPanel value={filter} class="mt-6">
    {#if loaded && incidents.length === 0 && !error}
      <Empty
        title={filter === 'open' ? 'Ningún incidente abierto' : 'Todavía no hubo incidentes'}
        text={filter === 'open'
          ? 'Todo en orden. Si un monitor falla, el incidente aparece aquí.'
          : 'Cuando un monitor falle el tiempo de confirmación, el incidente aparece aquí.'}
      />
    {:else if incidents.length > 0}
      <IncidentList {incidents} {tenantId} headingLevel={2} />
    {/if}
  </TabPanel>
</Tabs>
