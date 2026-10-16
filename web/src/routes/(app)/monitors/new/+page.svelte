<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';

  import MonitorForm from '#lib/components/MonitorForm.svelte';
  import { describeError, request } from '#lib/api.js';
  import { emptyForm } from '#lib/monitor-form.js';
  import { selectedTenant } from '#lib/tenant.svelte.js';
  import type { Monitor, MonitorInput } from '#lib/types.js';

  let error = $state('');

  async function create(input: MonitorInput) {
    error = '';
    try {
      const monitor = await request<Monitor>('POST', '/v1/monitors', {
        tenant: selectedTenant.current?.id,
        body: input,
      });
      await goto(resolve('/(app)/monitors/[id]', { id: monitor.id }));
    } catch (err) {
      error = describeError(err);
    }
  }
</script>

<svelte:head>
  <title>Nuevo monitor · taut</title>
</svelte:head>

<a href={resolve('/(app)/monitors')} class="text-sm text-neutral-500 hover:text-neutral-900">
  ← Monitores
</a>
<h1 class="mt-2 text-xl font-semibold text-neutral-900">Nuevo monitor</h1>
{#if error}
  <p class="mt-6 rounded-lg bg-red-50 px-4 py-3 text-sm text-red-700">{error}</p>
{/if}
<div class="mt-6 max-w-2xl">
  <MonitorForm initial={emptyForm()} submitLabel="Crear monitor" onsubmit={create} />
</div>
