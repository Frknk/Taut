<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';

  import ArrowLeftIcon from 'phosphor-svelte/lib/ArrowLeftIcon';

  import Banner from '#lib/components/Banner.svelte';
  import MonitorForm from '#lib/components/MonitorForm.svelte';
  import { describeError, request } from '#lib/api.js';
  import { formFromMonitor } from '#lib/monitor-form.js';
  import { selectedTenant } from '#lib/tenant.svelte.js';
  import type { Monitor, MonitorInput } from '#lib/types.js';
  import { backLink } from '#lib/ui.js';

  const id = $derived(page.params.id ?? '');
  const tenantId = $derived(selectedTenant.current?.id ?? '');

  let monitor = $state<Monitor | null>(null);
  let error = $state('');

  onMount(async () => {
    try {
      monitor = await request<Monitor>('GET', `/v1/monitors/${id}`, { tenant: tenantId });
    } catch (err) {
      error = describeError(err);
    }
  });

  async function save(input: MonitorInput) {
    error = '';
    try {
      await request<Monitor>('PUT', `/v1/monitors/${id}`, { tenant: tenantId, body: input });
      await goto(resolve('/(app)/monitors/[id]', { id }));
    } catch (err) {
      error = describeError(err);
    }
  }
</script>

<svelte:head>
  <title>Editar monitor · Taut</title>
</svelte:head>

<a href={resolve('/(app)/monitors/[id]', { id })} class={backLink}>
  <ArrowLeftIcon size={14} aria-hidden="true" />
  {monitor?.name ?? 'Monitor'}
</a>
<h1 class="mt-2 text-2xl font-semibold text-strong">Editar monitor</h1>
{#if error}
  <Banner tone="danger" class="mt-6">{error}</Banner>
{/if}
{#if monitor}
  <div class="mt-6 max-w-2xl">
    <MonitorForm
      initial={formFromMonitor(monitor)}
      editing
      submitLabel="Guardar cambios"
      onsubmit={save}
    />
  </div>
{/if}
