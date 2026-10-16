<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';

  import IncidentList from '#lib/components/IncidentList.svelte';
  import StatusBadge from '#lib/components/StatusBadge.svelte';
  import { API_URL, describeError, request } from '#lib/api.js';
  import { KIND_LABELS, formatDateTime, formatSeconds, timeAgo } from '#lib/format.js';
  import { selectedTenant } from '#lib/tenant.svelte.js';
  import type { CheckResult, Incident, Monitor } from '#lib/types.js';

  const id = $derived(page.params.id ?? '');
  const tenantId = $derived(selectedTenant.current?.id ?? '');

  let monitor = $state<Monitor | null>(null);
  let checks = $state<CheckResult[]>([]);
  let incidents = $state<Incident[]>([]);
  let error = $state('');
  let copied = $state(false);

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
    if (!monitor || !confirm(`¿Eliminar «${monitor.name}» y todo su historial?`)) return;
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

  {#if monitor.type === 'heartbeat'}
    <section class="mt-6 rounded-xl border border-neutral-200 bg-white p-6 text-sm">
      <h2 class="font-medium text-neutral-900">URL de ping</h2>
      <p class="mt-1 text-neutral-500">
        Tu tarea programada la llama al terminar; si no llega en
        {formatSeconds(monitor.interval_s + monitor.grace_period_s)}, se abre un incidente.
        {#if monitor.last_ping_at}Último ping {timeAgo(monitor.last_ping_at)}.{:else}Pendiente hasta
          el primer ping.{/if}
      </p>
      <div class="mt-3 flex gap-2">
        <code class="min-w-0 flex-1 truncate rounded-lg bg-neutral-100 px-3 py-2">{pingUrl}</code>
        <button class={button} onclick={copyPingUrl}>{copied ? 'Copiada' : 'Copiar'}</button>
      </div>
      <pre
        class="mt-3 overflow-x-auto rounded-lg bg-neutral-900 px-3 py-2 text-xs text-neutral-100">0 3 * * * /usr/local/bin/backup.sh && curl -fsS {pingUrl} || curl -fsS {pingUrl}/fail</pre>
    </section>
  {/if}

  {#if incidents.length > 0}
    <section class="mt-6">
      <h2 class="text-sm font-medium text-neutral-900">Incidentes</h2>
      <div class="mt-2">
        <IncidentList {incidents} {tenantId} showMonitor={false} />
      </div>
    </section>
  {/if}

  <section class="mt-6">
    <h2 class="text-sm font-medium text-neutral-900">Últimos chequeos</h2>
    {#if checks.length === 0}
      <p class="mt-2 text-sm text-neutral-500">
        {monitor.active ? 'Todavía no hay resultados.' : 'El monitor está pausado.'}
      </p>
    {:else}
      <div class="mt-2 overflow-x-auto rounded-xl border border-neutral-200 bg-white">
        <table class="w-full text-left text-sm">
          <thead class="border-b border-neutral-200 text-xs text-neutral-500">
            <tr>
              <th class="px-4 py-2 font-medium">Cuándo</th>
              <th class="px-4 py-2 font-medium">Resultado</th>
              <th class="px-4 py-2 font-medium">Código</th>
              <th class="px-4 py-2 font-medium">Latencia</th>
              <th class="px-4 py-2 font-medium">Causa</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-neutral-100">
            {#each checks as check (check.at + check.region)}
              <tr>
                <td
                  class="px-4 py-2 whitespace-nowrap text-neutral-600"
                  title={formatDateTime(check.at)}
                >
                  {timeAgo(check.at)}
                </td>
                <td class="px-4 py-2">
                  <span class={check.ok ? 'text-emerald-700' : 'font-medium text-red-700'}>
                    {check.ok ? 'OK' : 'Falla'}
                  </span>
                </td>
                <td class="px-4 py-2 text-neutral-600">{check.status ?? '—'}</td>
                <td class="px-4 py-2 text-neutral-600">
                  {check.latency_ms === null ? '—' : `${check.latency_ms} ms`}
                </td>
                <td class="px-4 py-2 text-neutral-600">{check.error ?? ''}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </section>
{/if}
