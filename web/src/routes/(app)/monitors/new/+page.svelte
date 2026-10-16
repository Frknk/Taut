<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';

  import ArrowLeftIcon from 'phosphor-svelte/lib/ArrowLeftIcon';

  import Banner from '#lib/components/Banner.svelte';
  import MonitorForm from '#lib/components/MonitorForm.svelte';
  import { describeError, request } from '#lib/api.js';
  import { emptyForm } from '#lib/monitor-form.js';
  import { selectedTenant } from '#lib/tenant.svelte.js';
  import type { Monitor, MonitorInput } from '#lib/types.js';
  import { backLink } from '#lib/ui.js';

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

<a href={resolve('/(app)/monitors')} class={backLink}>
  <ArrowLeftIcon size={14} aria-hidden="true" />
  Monitores
</a>
<h1 class="mt-2 text-2xl font-semibold text-strong">Nuevo monitor</h1>
{#if error}
  <Banner tone="danger" class="mt-6">{error}</Banner>
{/if}
<div class="mt-6 max-w-2xl">
  <MonitorForm initial={emptyForm()} submitLabel="Crear monitor" onsubmit={create} />
</div>
