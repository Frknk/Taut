<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';

  import MonitorForm from '#lib/components/MonitorForm.svelte';
  import { describeError, request } from '#lib/api.js';
  import { formFromMonitor } from '#lib/monitor-form.js';
  import { selectedTenant } from '#lib/tenant.svelte.js';
  import type { Monitor, MonitorInput } from '#lib/types.js';

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
  <title>Editar monitor · taut</title>
</svelte:head>

<a
  href={resolve('/(app)/monitors/[id]', { id })}
  class="text-sm text-neutral-500 hover:text-neutral-900"
>
  ← {monitor?.name ?? 'Monitor'}
</a>
<h1 class="mt-2 text-xl font-semibold text-neutral-900">Editar monitor</h1>
{#if error}
  <p class="mt-6 rounded-lg bg-red-50 px-4 py-3 text-sm text-red-700">{error}</p>
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
