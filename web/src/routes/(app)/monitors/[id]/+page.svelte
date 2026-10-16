<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';

  import StatusBadge from '#lib/components/StatusBadge.svelte';
  import { describeError, request } from '#lib/api.js';
  import { KIND_LABELS, formatDateTime, formatSeconds } from '#lib/format.js';
  import { selectedTenant } from '#lib/tenant.svelte.js';
  import type { Monitor } from '#lib/types.js';

  const id = $derived(page.params.id ?? '');
  const tenantId = $derived(selectedTenant.current?.id ?? '');

  let monitor = $state<Monitor | null>(null);
  let error = $state('');

  async function load() {
    try {
      monitor = await request<Monitor>('GET', `/v1/monitors/${id}`, { tenant: tenantId });
      error = '';
    } catch (err) {
      error = describeError(err);
    }
  }

  async function toggle() {
    if (!monitor) return;
    try {
      monitor = await request<Monitor>(
        'POST',
        `/v1/monitors/${id}/${monitor.active ? 'pause' : 'resume'}`,
        { tenant: tenantId },
      );
    } catch (err) {
      error = describeError(err);
    }
  }

  async function remove() {
    if (!monitor || !confirm(`¿Eliminar «${monitor.name}» y todo su historial?`)) return;
    try {
      await request<void>('DELETE', `/v1/monitors/${id}`, { tenant: tenantId });
      await goto(resolve('/(app)/monitors'));
    } catch (err) {
      error = describeError(err);
    }
  }

  onMount(load);

  const button =
    'rounded-lg border border-neutral-300 bg-white px-3 py-1.5 text-sm text-neutral-700 hover:bg-neutral-50';
</script>

<svelte:head>
  <title>{monitor?.name ?? 'Monitor'} · taut</title>
</svelte:head>

<a href={resolve('/(app)/monitors')} class="text-sm text-neutral-500 hover:text-neutral-900">
  ← Monitores
</a>

{#if error}
  <p class="mt-6 rounded-lg bg-red-50 px-4 py-3 text-sm text-red-700">{error}</p>
{/if}

{#if monitor}
  <div class="mt-2 flex flex-wrap items-center gap-3">
    <h1 class="text-xl font-semibold text-neutral-900">{monitor.name}</h1>
    <StatusBadge status={monitor.status} />
    <div class="ml-auto flex gap-2">
      <a href={resolve('/(app)/monitors/[id]/edit', { id })} class={button}>Editar</a>
      <button class={button} onclick={toggle}>{monitor.active ? 'Pausar' : 'Reanudar'}</button>
      <button class={[button, 'text-red-700']} onclick={remove}>Eliminar</button>
    </div>
  </div>

  <dl
    class="mt-6 grid gap-x-8 gap-y-4 rounded-xl border border-neutral-200 bg-white p-6 text-sm sm:grid-cols-2"
  >
    <div>
      <dt class="text-neutral-500">Tipo</dt>
      <dd class="font-medium text-neutral-900">{KIND_LABELS[monitor.type]}</dd>
    </div>
    {#if monitor.target}
      <div>
        <dt class="text-neutral-500">Destino</dt>
        <dd class="font-medium break-all text-neutral-900">{monitor.target}</dd>
      </div>
    {/if}
    <div>
      <dt class="text-neutral-500">Frecuencia</dt>
      <dd class="font-medium text-neutral-900">Cada {formatSeconds(monitor.interval_s)}</dd>
    </div>
    {#if monitor.type === 'heartbeat'}
      <div>
        <dt class="text-neutral-500">Periodo de gracia</dt>
        <dd class="font-medium text-neutral-900">{formatSeconds(monitor.grace_period_s)}</dd>
      </div>
    {:else}
      <div>
        <dt class="text-neutral-500">Timeout</dt>
        <dd class="font-medium text-neutral-900">{formatSeconds(monitor.timeout_s)}</dd>
      </div>
      <div>
        <dt class="text-neutral-500">Confirmación / recuperación</dt>
        <dd class="font-medium text-neutral-900">
          {formatSeconds(monitor.confirmation_period_s)} / {formatSeconds(
            monitor.recovery_period_s,
          )}
        </dd>
      </div>
    {/if}
    {#if monitor.type === 'http' || monitor.type === 'keyword'}
      <div>
        <dt class="text-neutral-500">Código esperado</dt>
        <dd class="font-medium text-neutral-900">{monitor.expected_status ?? 'Cualquier 2xx'}</dd>
      </div>
    {/if}
    {#if monitor.keyword}
      <div>
        <dt class="text-neutral-500">Texto esperado</dt>
        <dd class="font-medium text-neutral-900">«{monitor.keyword}»</dd>
      </div>
    {/if}
    <div>
      <dt class="text-neutral-500">Creado</dt>
      <dd class="font-medium text-neutral-900">{formatDateTime(monitor.created_at)}</dd>
    </div>
  </dl>
{/if}
