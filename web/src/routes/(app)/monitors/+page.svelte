<script lang="ts">
  import { onMount } from 'svelte';
  import { resolve } from '$app/paths';

  import PlusIcon from 'phosphor-svelte/lib/PlusIcon';

  import Banner from '#lib/components/Banner.svelte';
  import Button from '#lib/components/Button.svelte';
  import Empty from '#lib/components/Empty.svelte';
  import LayerCard from '#lib/components/LayerCard.svelte';
  import StatusBadge from '#lib/components/StatusBadge.svelte';
  import { describeError, request } from '#lib/api.js';
  import { KIND_LABELS, PLAN_LABELS, formatSeconds, timeAgo } from '#lib/format.js';
  import { selectedTenant } from '#lib/tenant.svelte.js';
  import type { Monitor } from '#lib/types.js';
  import { focusRing, table } from '#lib/ui.js';

  let monitors = $state<Monitor[]>([]);
  let loaded = $state(false);
  let error = $state('');

  const tenantId = $derived(selectedTenant.current?.id ?? '');
  const tenant = $derived(selectedTenant.details);
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
  <title>Monitores · Taut</title>
</svelte:head>

<div class="flex flex-wrap items-end justify-between gap-4">
  <div>
    <h1 class="text-2xl font-semibold text-strong">Monitores</h1>
    {#if tenant}
      <p class="mt-0.5 text-[13px]/[18px] text-subtle tabular-nums">
        {monitors.length - heartbeats} / {tenant.limits.monitors} monitores · {heartbeats} / {tenant
          .limits.heartbeats} heartbeats (plan {PLAN_LABELS[tenant.plan]})
      </p>
    {/if}
  </div>
  <Button variant="primary" href={resolve('/(app)/monitors/new')}>
    <PlusIcon size={16} aria-hidden="true" />
    Nuevo monitor
  </Button>
</div>

{#if error}
  <Banner tone="danger" class="mt-6">{error}</Banner>
{/if}

{#if loaded && monitors.length === 0 && !error}
  <div class="mt-6">
    <Empty
      title="Todavía no hay monitores"
      text="Un monitor comprueba una URL, un puerto o un heartbeat cada pocos segundos y te avisa cuando falla."
    >
      <Button variant="primary" href={resolve('/(app)/monitors/new')}>
        <PlusIcon size={16} aria-hidden="true" />
        Crea el primero
      </Button>
    </Empty>
  </div>
{:else if monitors.length > 0}
  <LayerCard class="mt-6" flush>
    <table class={table.root}>
      <thead>
        <tr>
          <th scope="col" class={table.th}>Nombre</th>
          <th scope="col" class={table.th}>Estado</th>
          <th scope="col" class={table.th}>Último chequeo</th>
          <th scope="col" class={[table.th, 'text-right']}>Latencia</th>
          <th scope="col" class={table.th}><span class="sr-only">Acciones</span></th>
        </tr>
      </thead>
      <tbody>
        {#each monitors as monitor (monitor.id)}
          <tr class={table.row}>
            <td class={[table.td, 'max-w-80']}>
              <a
                href={resolve('/(app)/monitors/[id]', { id: monitor.id })}
                class={[
                  'block truncate rounded-sm font-medium text-default hover:underline',
                  focusRing,
                ]}
              >
                {monitor.name}
              </a>
              <span class="block truncate text-xs text-subtle">
                {KIND_LABELS[monitor.type]} · cada {formatSeconds(monitor.interval_s)}
                {#if monitor.target}· <code>{monitor.target}</code>{/if}
              </span>
            </td>
            <td class={table.td}><StatusBadge status={monitor.status} /></td>
            <td class={[table.td, 'whitespace-nowrap text-subtle']}>
              {#if monitor.last_check_at}
                <span class={monitor.last_check_ok ? 'text-success' : 'font-medium text-danger'}>
                  {monitor.last_check_ok ? 'OK' : 'Falla'}
                </span>
                · {timeAgo(monitor.last_check_at)}
              {:else}
                Sin chequeos aún
              {/if}
            </td>
            <td class={[table.td, 'text-right whitespace-nowrap tabular-nums']}>
              {monitor.last_latency_ms === null ? '—' : `${monitor.last_latency_ms} ms`}
            </td>
            <td class={[table.td, 'text-right']}>
              <Button size="sm" onclick={() => toggle(monitor)}>
                {monitor.active ? 'Pausar' : 'Reanudar'}
              </Button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </LayerCard>
{/if}
