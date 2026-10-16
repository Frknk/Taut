<script lang="ts">
  import { onMount } from 'svelte';
  import { resolve } from '$app/paths';

  import StatusBadge from '#lib/components/StatusBadge.svelte';
  import { describeError, request } from '#lib/api.js';
  import { KIND_LABELS, formatSeconds, timeAgo } from '#lib/format.js';
  import { selectedTenant } from '#lib/tenant.svelte.js';
  import type { Monitor } from '#lib/types.js';

  const QUOTA = 10;

  let monitors = $state<Monitor[]>([]);
  let loaded = $state(false);
  let error = $state('');

  const tenantId = $derived(selectedTenant.current?.id ?? '');
  const heartbeats = $derived(monitors.filter((monitor) => monitor.type === 'heartbeat').length);

  async function load() {
    try {
      monitors = await request<Monitor[]>('GET', '/v1/monitors', { tenant: tenantId });
      error = '';
    } catch (err) {
      error = describeError(err);
    } finally {
      loaded = true;
    }
  }

  async function toggle(monitor: Monitor) {
    const action = monitor.active ? 'pause' : 'resume';
    try {
      const updated = await request<Monitor>('POST', `/v1/monitors/${monitor.id}/${action}`, {
        tenant: tenantId,
      });
      monitors = monitors.map((m) => (m.id === updated.id ? updated : m));
    } catch (err) {
      error = describeError(err);
    }
  }

  onMount(() => {
    void load();
    const timer = setInterval(load, 10_000);
    return () => clearInterval(timer);
  });
</script>

<svelte:head>
  <title>Monitores · taut</title>
</svelte:head>

<div class="flex flex-wrap items-end justify-between gap-4">
  <div>
    <h1 class="text-xl font-semibold text-neutral-900">Monitores</h1>
    <p class="mt-1 text-sm text-neutral-500">
      {monitors.length - heartbeats} / {QUOTA} monitores · {heartbeats} / {QUOTA} heartbeats (plan free)
    </p>
  </div>
  <a
    href={resolve('/(app)/monitors/new')}
    class="rounded-lg bg-neutral-900 px-4 py-2 text-sm font-medium text-white hover:bg-neutral-700"
  >
    Nuevo monitor
  </a>
</div>

{#if error}
  <p class="mt-6 rounded-lg bg-red-50 px-4 py-3 text-sm text-red-700">{error}</p>
{/if}

{#if loaded && monitors.length === 0 && !error}
  <div class="mt-10 rounded-xl border border-dashed border-neutral-300 p-10 text-center">
    <p class="text-sm text-neutral-600">Todavía no hay monitores.</p>
    <a
      href={resolve('/(app)/monitors/new')}
      class="mt-2 inline-block text-sm font-medium underline"
    >
      Crea el primero
    </a>
  </div>
{:else if monitors.length > 0}
  <ul class="mt-6 divide-y divide-neutral-100 rounded-xl border border-neutral-200 bg-white">
    {#each monitors as monitor (monitor.id)}
      <li class="flex flex-wrap items-center gap-x-4 gap-y-2 px-4 py-3">
        <StatusBadge status={monitor.status} />
        <a
          href={resolve('/(app)/monitors/[id]', { id: monitor.id })}
          class="min-w-0 flex-1 hover:underline"
        >
          <span class="block truncate font-medium text-neutral-900">{monitor.name}</span>
          <span class="block truncate text-xs text-neutral-500">
            {KIND_LABELS[monitor.type]}
            {#if monitor.target}· {monitor.target}{/if}
            · cada {formatSeconds(monitor.interval_s)}
          </span>
        </a>
        <span class="text-right text-xs text-neutral-500">
          {#if monitor.last_check_at}
            <span class={monitor.last_check_ok ? 'text-emerald-700' : 'text-red-700'}>
              {monitor.last_check_ok ? 'OK' : 'Falla'}
            </span>
            {timeAgo(monitor.last_check_at)}
            {#if monitor.last_latency_ms !== null}· {monitor.last_latency_ms} ms{/if}
          {:else}
            Sin chequeos aún
          {/if}
        </span>
        <button
          class="rounded-lg border border-neutral-300 px-3 py-1 text-xs text-neutral-700 hover:bg-neutral-50"
          onclick={() => toggle(monitor)}
        >
          {monitor.active ? 'Pausar' : 'Reanudar'}
        </button>
      </li>
    {/each}
  </ul>
{/if}
