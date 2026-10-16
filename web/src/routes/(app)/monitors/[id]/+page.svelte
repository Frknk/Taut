<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';

  import ArrowLeftIcon from 'phosphor-svelte/lib/ArrowLeftIcon';

  import Banner from '#lib/components/Banner.svelte';
  import Button from '#lib/components/Button.svelte';
  import Dialog from '#lib/components/Dialog.svelte';
  import IncidentList from '#lib/components/IncidentList.svelte';
  import LayerCard from '#lib/components/LayerCard.svelte';
  import MetricsPanel from '#lib/components/MetricsPanel.svelte';
  import StatusBadge from '#lib/components/StatusBadge.svelte';
  import { API_URL, describeError, request } from '#lib/api.js';
  import { KIND_LABELS, formatDateTime, formatSeconds, timeAgo } from '#lib/format.js';
  import { selectedTenant } from '#lib/tenant.svelte.js';
  import type { CheckResult, Incident, Monitor } from '#lib/types.js';
  import { backLink, table } from '#lib/ui.js';

  const id = $derived(page.params.id ?? '');
  const tenantId = $derived(selectedTenant.current?.id ?? '');

  let monitor = $state<Monitor | null>(null);
  let checks = $state<CheckResult[]>([]);
  let incidents = $state<Incident[]>([]);
  let error = $state('');
  let copied = $state(false);
  let confirmingDelete = $state(false);

  const pingUrl = $derived(
    monitor?.heartbeat_token ? `${API_URL}/h/${monitor.heartbeat_token}` : '',
  );

  async function load() {
    try {
      [monitor, checks, incidents] = await Promise.all([
        request<Monitor>('GET', `/v1/monitors/${id}`, { tenant: tenantId }),
        request<CheckResult[]>('GET', `/v1/monitors/${id}/checks?limit=20`, { tenant: tenantId }),
        request<Incident[]>('GET', `/v1/incidents?monitor_id=${id}&limit=10`, { tenant: tenantId }),
      ]);
      error = '';
    } catch (err) {
      error = describeError(err);
    }
  }

  async function copyPingUrl() {
    try {
      await navigator.clipboard.writeText(pingUrl);
      copied = true;
      setTimeout(() => (copied = false), 2000);
    } catch {
      copied = false;
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
    confirmingDelete = false;
    try {
      await request<void>('DELETE', `/v1/monitors/${id}`, { tenant: tenantId });
      await goto(resolve('/(app)/monitors'));
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
  <title>{monitor?.name ?? 'Monitor'} · taut</title>
</svelte:head>

<a href={resolve('/(app)/monitors')} class={backLink}>
  <ArrowLeftIcon size={14} aria-hidden="true" />
  Monitores
</a>

{#if error}
  <Banner tone="danger" class="mt-6">{error}</Banner>
{/if}

{#if monitor}
  <div class="mt-2 flex flex-wrap items-center gap-3">
    <h1 class="text-2xl font-semibold text-strong">{monitor.name}</h1>
    <StatusBadge status={monitor.status} />
    <div class="ml-auto flex gap-2">
      <Button href={resolve('/(app)/monitors/[id]/edit', { id })}>Editar</Button>
      <Button onclick={toggle}>{monitor.active ? 'Pausar' : 'Reanudar'}</Button>
      <Button variant="secondary-destructive" onclick={() => (confirmingDelete = true)}>
        Eliminar
      </Button>
    </div>
  </div>

  <Dialog
    bind:open={confirmingDelete}
    alert
    title="¿Eliminar «{monitor.name}»?"
    description="Se borran el monitor, sus chequeos y sus incidentes. Esto no se puede deshacer."
  >
    {#snippet actions()}
      <Button variant="ghost" onclick={() => (confirmingDelete = false)}>Cancelar</Button>
      <Button variant="destructive" onclick={remove}>Eliminar monitor</Button>
    {/snippet}
  </Dialog>

  <LayerCard title="Configuración" class="mt-6">
    <dl class="grid gap-x-8 gap-y-4 sm:grid-cols-2">
      <div>
        <dt class="text-xs text-subtle">Tipo</dt>
        <dd class="mt-0.5 font-medium text-default">{KIND_LABELS[monitor.type]}</dd>
      </div>
      {#if monitor.target}
        <div>
          <dt class="text-xs text-subtle">Destino</dt>
          <dd class="mt-0.5 break-all text-default"><code>{monitor.target}</code></dd>
        </div>
      {/if}
      <div>
        <dt class="text-xs text-subtle">Frecuencia</dt>
        <dd class="mt-0.5 font-medium text-default">Cada {formatSeconds(monitor.interval_s)}</dd>
      </div>
      {#if monitor.type === 'heartbeat'}
        <div>
          <dt class="text-xs text-subtle">Periodo de gracia</dt>
          <dd class="mt-0.5 font-medium text-default">{formatSeconds(monitor.grace_period_s)}</dd>
        </div>
      {:else}
        <div>
          <dt class="text-xs text-subtle">Timeout</dt>
          <dd class="mt-0.5 font-medium text-default">{formatSeconds(monitor.timeout_s)}</dd>
        </div>
        <div>
          <dt class="text-xs text-subtle">Confirmación / recuperación</dt>
          <dd class="mt-0.5 font-medium text-default">
            {formatSeconds(monitor.confirmation_period_s)} / {formatSeconds(
              monitor.recovery_period_s,
            )}
          </dd>
        </div>
      {/if}
      {#if monitor.type === 'http' || monitor.type === 'keyword'}
        <div>
          <dt class="text-xs text-subtle">Código esperado</dt>
          <dd class="mt-0.5 font-medium text-default">
            {monitor.expected_status ?? 'Cualquier 2xx'}
          </dd>
        </div>
      {/if}
      {#if monitor.keyword}
        <div>
          <dt class="text-xs text-subtle">Texto esperado</dt>
          <dd class="mt-0.5 font-medium text-default">«{monitor.keyword}»</dd>
        </div>
      {/if}
      <div>
        <dt class="text-xs text-subtle">Creado</dt>
        <dd class="mt-0.5 font-medium text-default">{formatDateTime(monitor.created_at)}</dd>
      </div>
    </dl>
  </LayerCard>

  {#if monitor.type === 'heartbeat'}
    <LayerCard title="URL de ping" class="mt-6">
      {#snippet actions()}
        <Button variant="ghost" size="sm" onclick={copyPingUrl}>
          {copied ? 'Copiada' : 'Copiar URL'}
        </Button>
      {/snippet}
      <p class="text-subtle">
        Tu tarea programada la llama al terminar; si no llega en
        {formatSeconds(monitor.interval_s + monitor.grace_period_s)}, se abre un incidente.
        {#if monitor.last_ping_at}Último ping {timeAgo(monitor.last_ping_at)}.{:else}Pendiente hasta
          el primer ping.{/if}
      </p>
      <code class="mt-3 block truncate rounded-md bg-recessed px-3 py-2 ring ring-hairline"
        >{pingUrl}</code
      >
      <pre
        class="mt-3 overflow-x-auto rounded-md bg-recessed px-3 py-2 font-mono text-xs text-default ring ring-hairline">0 3 * * * /usr/local/bin/backup.sh && curl -fsS {pingUrl} || curl -fsS {pingUrl}/fail</pre>
    </LayerCard>
  {/if}

  {#if monitor.type !== 'heartbeat'}
    <MetricsPanel monitorId={id} {tenantId} />
  {/if}

  {#if incidents.length > 0}
    <section class="mt-6">
      <h2 class="text-base font-semibold text-strong">Incidentes</h2>
      <div class="mt-2">
        <IncidentList {incidents} {tenantId} showMonitor={false} />
      </div>
    </section>
  {/if}

  {#if checks.length === 0}
    <section class="mt-6">
      <h2 class="text-base font-semibold text-strong">Últimos chequeos</h2>
      <p class="mt-2 text-subtle">
        {monitor.active ? 'Todavía no hay resultados.' : 'El monitor está pausado.'}
      </p>
    </section>
  {:else}
    <LayerCard title="Últimos chequeos" class="mt-6" flush>
      <table class={table.root}>
        <thead>
          <tr>
            <th scope="col" class={table.th}>Cuándo</th>
            <th scope="col" class={table.th}>Resultado</th>
            <th scope="col" class={table.th}>Código</th>
            <th scope="col" class={[table.th, 'text-right']}>Latencia</th>
            <th scope="col" class={table.th}>Causa</th>
          </tr>
        </thead>
        <tbody>
          {#each checks as check (check.at + check.region)}
            <tr class={table.row}>
              <td
                class={[table.td, 'whitespace-nowrap text-subtle']}
                title={formatDateTime(check.at)}
              >
                {timeAgo(check.at)}
              </td>
              <td class={table.td}>
                <span class={check.ok ? 'text-success' : 'font-medium text-danger'}>
                  {check.ok ? 'OK' : 'Falla'}
                </span>
              </td>
              <td class={[table.td, 'tabular-nums']}>{check.status ?? '—'}</td>
              <td class={[table.td, 'text-right whitespace-nowrap tabular-nums']}>
                {check.latency_ms === null ? '—' : `${check.latency_ms} ms`}
              </td>
              <td class={[table.td, 'text-subtle']}>{check.error ?? ''}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </LayerCard>
  {/if}
{/if}
