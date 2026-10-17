<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/state';

  import Banner from '#lib/components/Banner.svelte';
  import Logo from '#lib/components/Logo.svelte';
  import StatusBadge from '#lib/components/StatusBadge.svelte';
  import UptimeBars from '#lib/components/UptimeBars.svelte';
  import { STATUS_URL } from '#lib/api.js';
  import { UPTIME_LEVELS, formatPercent } from '#lib/charts.js';
  import { formatDateTime, formatSeconds } from '#lib/format.js';
  import type { MonitorStatus, StatusPage } from '#lib/types.js';
  import { card, link, table } from '#lib/ui.js';

  const slug = $derived(page.params.slug ?? '');

  let status = $state<StatusPage | null>(null);
  let missing = $state(false);
  let failed = $state(false);
  let showFailures = $state(false);

  async function load() {
    try {
      const response = await fetch(`${STATUS_URL}/s/${encodeURIComponent(slug)}`);
      missing = response.status === 404;
      failed = !response.ok && !missing;
      if (response.ok) status = (await response.json()) as StatusPage;
    } catch {
      failed = true;
    }
  }

  onMount(() => {
    void load();
    const timer = setInterval(load, 30_000);
    return () => clearInterval(timer);
  });

  const BANNERS = {
    operational: { tone: 'success', text: 'Todos los sistemas operativos' },
    partial_outage: { tone: 'warning', text: 'Algunos sistemas tienen problemas' },
    major_outage: { tone: 'danger', text: 'Interrupción general' },
  } as const;

  /** Public wording: visitors don't pause monitors, so «En pausa» and «Sin datos aún». */
  const PUBLIC_LABELS: Record<MonitorStatus, string> = {
    up: 'Operativo',
    down: 'Caído',
    paused: 'En pausa',
    pending: 'Sin datos aún',
  };

  const openIncidents = $derived(status?.incidents.filter((i) => i.resolved_at === null) ?? []);
  const pastIncidents = $derived(status?.incidents.filter((i) => i.resolved_at !== null) ?? []);
  const failureDays = $derived(
    status?.monitors.flatMap((monitor) =>
      monitor.days
        .filter((day) => day.failures > 0)
        .map((day) => ({ monitor: monitor.name, ...day })),
    ) ?? [],
  );
</script>

<svelte:head>
  <title>{status ? `Estado de ${status.tenant.name}` : 'Página de estado'}</title>
</svelte:head>

<main class="min-h-screen px-4 py-10">
  <div class="mx-auto max-w-3xl">
    {#if missing}
      <p class="text-center text-subtle">No existe una página de estado «{slug}».</p>
    {:else if failed && !status}
      <p class="text-center text-subtle">No se pudo cargar la página de estado. Reintentando…</p>
    {:else if status}
      {@const banner = BANNERS[status.status]}
      <h1 class="text-2xl font-semibold text-strong">{status.tenant.name}</h1>
      <Banner tone={banner.tone} large class="mt-4">{banner.text}</Banner>

      {#if openIncidents.length > 0}
        <section class={[card, 'mt-6 p-5']}>
          <h2 class="font-semibold text-danger">Incidentes activos</h2>
          <ul class="mt-2 space-y-1 text-default">
            {#each openIncidents as incident (incident.monitor_name + incident.started_at)}
              <li>
                <span class="font-medium">{incident.monitor_name}</span> con problemas desde el
                {formatDateTime(incident.started_at)} ({formatSeconds(incident.duration_s)})
              </li>
            {/each}
          </ul>
        </section>
      {/if}

      <section class={[card, 'mt-6 space-y-6 p-5']}>
        {#if status.monitors.length === 0}
          <p class="text-subtle">Esta organización todavía no publica monitores.</p>
        {/if}
        {#each status.monitors as monitor, index (index)}
          <div>
            <div class="flex items-center justify-between gap-4">
              <span class="font-medium text-default">{monitor.name}</span>
              <StatusBadge status={monitor.status} label={PUBLIC_LABELS[monitor.status]} />
            </div>
            <div class="mt-2">
              <UptimeBars days={monitor.days} label={monitor.name} />
            </div>
            <div class="mt-1 flex justify-between text-xs" style:color="var(--viz-muted)">
              <span>hace 90 días</span>
              <span class="text-subtle tabular-nums">
                {formatPercent(monitor.uptime_90d)} de disponibilidad
              </span>
              <span>hoy</span>
            </div>
          </div>
        {/each}
        {#if status.monitors.length > 0}
          <div
            class="flex flex-wrap gap-x-4 gap-y-1 border-t border-hairline pt-4 text-xs"
            style:color="var(--viz-ink-secondary)"
          >
            <span>Disponibilidad diaria:</span>
            {#each UPTIME_LEVELS as level (level.level)}
              <span class="inline-flex items-center gap-1.5">
                <span
                  class="inline-block size-2.5 rounded-xs"
                  style:background="var(--status-{level.level})"
                ></span>
                {level.label}
              </span>
            {/each}
            <button
              class={['ml-auto cursor-pointer', link]}
              onclick={() => (showFailures = !showFailures)}
            >
              {showFailures ? 'Ocultar días con fallas' : 'Ver días con fallas'}
            </button>
          </div>
          {#if showFailures}
            {#if failureDays.length === 0}
              <p class="text-xs text-subtle">Ningún día con fallas en los últimos 90 días.</p>
            {:else}
              <div class="overflow-x-auto">
                <table class={[table.root, 'tabular-nums']}>
                  <thead>
                    <tr>
                      <th scope="col" class={table.th}>Día</th>
                      <th scope="col" class={table.th}>Monitor</th>
                      <th scope="col" class={[table.th, 'text-right']}>Disponibilidad</th>
                      <th scope="col" class={[table.th, 'text-right']}>Fallas</th>
                    </tr>
                  </thead>
                  <tbody>
                    {#each failureDays as day (day.monitor + day.day)}
                      <tr class={table.row}>
                        <td class={table.td}>{day.day}</td>
                        <td class={table.td}>{day.monitor}</td>
                        <td class={[table.td, 'text-right']}>{formatPercent(day.uptime)}</td>
                        <td class={[table.td, 'text-right']}>{day.failures} de {day.checks}</td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </div>
            {/if}
          {/if}
        {/if}
      </section>

      {#if pastIncidents.length > 0}
        <section class="mt-6">
          <h2 class="text-base font-semibold text-strong">Incidentes de los últimos 7 días</h2>
          <ul class="mt-2 space-y-1 text-subtle">
            {#each pastIncidents as incident (incident.monitor_name + incident.started_at)}
              <li>
                {incident.monitor_name}: {formatDateTime(incident.started_at)}, resuelto tras
                {formatSeconds(incident.duration_s)}
              </li>
            {/each}
          </ul>
        </section>
      {/if}

      <footer class="mt-12 flex flex-col items-center gap-3 text-xs text-subtle">
        <Logo class="w-5 text-strong" />
        <p>Actualizado {formatDateTime(status.generated_at)} · con Taut</p>
      </footer>
    {/if}
  </div>
</main>
