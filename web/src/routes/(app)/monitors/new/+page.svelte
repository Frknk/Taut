<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';

  import Banner from '#lib/components/Banner.svelte';
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
  <title>Nuevo monitor · Taut</title>
</svelte:head>

<h1 class="text-2xl font-semibold text-strong">Nuevo monitor</h1>
{#if error}
  <Banner tone="danger" class="mt-6">{error}</Banner>
{/if}
<div class="mt-6 max-w-2xl">
  <MonitorForm initial={emptyForm()} submitLabel="Crear monitor" onsubmit={create} />
</div>
