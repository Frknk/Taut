<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/state';

  import UptimeBars from '#lib/components/UptimeBars.svelte';
  import { STATUS_URL } from '#lib/api.js';
  import { UPTIME_LEVELS, formatPercent } from '#lib/charts.js';
  import { formatDateTime, formatSeconds } from '#lib/format.js';
  import type { MonitorStatus, StatusPage } from '#lib/types.js';

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
    operational: {
      icon: '✓',
      text: 'Todos los sistemas operativos',
      tone: 'bg-emerald-50 text-emerald-800',
    },
    partial_outage: {
      icon: '!',
      text: 'Algunos sistemas tienen problemas',
      tone: 'bg-amber-50 text-amber-900',
    },
    major_outage: { icon: '✕', text: 'Interrupción general', tone: 'bg-red-50 text-red-800' },
  } as const;

  const MONITOR: Record<MonitorStatus, { icon: string; text: string; tone: string }> = {
    up: { icon: '✓', text: 'Operativo', tone: 'text-emerald-700' },
    down: { icon: '✕', text: 'Caído', tone: 'text-red-700' },
    paused: { icon: '‖', text: 'En pausa', tone: 'text-neutral-500' },
    pending: { icon: '…', text: 'Sin datos aún', tone: 'text-neutral-500' },
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

<main class="min-h-screen bg-neutral-50 px-4 py-10">
  <div class="mx-auto max-w-3xl">
    {#if missing}
      <p class="text-center text-sm text-neutral-600">No existe una página de estado «{slug}».</p>
    {:else if failed && !status}
      <p class="text-center text-sm text-neutral-600">
        No se pudo cargar la página de estado. Reintentando…
      </p>
    {:else if status}
      {@const banner = BANNERS[status.status]}
      <h1 class="text-2xl font-semibold text-neutral-900">{status.tenant.name}</h1>
      <p class={['mt-4 rounded-xl px-5 py-4 text-base font-medium', banner.tone]}>
        <span aria-hidden="true">{banner.icon}</span>
        {banner.text}
      </p>

      {#if openIncidents.length > 0}
        <section class="mt-6 rounded-xl border border-red-200 bg-white p-5">
          <h2 class="text-sm font-medium text-red-800">Incidentes activos</h2>
          <ul class="mt-2 space-y-1 text-sm text-neutral-700">
            {#each openIncidents as incident (incident.monitor_name + incident.started_at)}
              <li>
                <span class="font-medium">{incident.monitor_name}</span> con problemas desde el
                {formatDateTime(incident.started_at)} ({formatSeconds(incident.duration_s)})
              </li>
            {/each}
          </ul>
        </section>
      {/if}

      <section class="mt-6 space-y-6 rounded-xl border border-neutral-200 bg-white p-5">
        {#if status.monitors.length === 0}
          <p class="text-sm text-neutral-500">Esta organización todavía no publica monitores.</p>
        {/if}
        {#each status.monitors as monitor, index (index)}
          {@const state = MONITOR[monitor.status]}
          <div>
            <div class="flex items-baseline justify-between gap-4 text-sm">
              <span class="font-medium text-neutral-900">{monitor.name}</span>
              <span class={['font-medium', state.tone]}>
                <span aria-hidden="true">{state.icon}</span>
                {state.text}
              </span>
            </div>
            <div class="mt-2">
              <UptimeBars days={monitor.days} label={monitor.name} />
            </div>
            <div class="mt-1 flex justify-between text-xs text-neutral-500">
              <span>hace 90 días</span>
              <span>{formatPercent(monitor.uptime_90d)} de disponibilidad</span>
              <span>hoy</span>
            </div>
          </div>
        {/each}
        {#if status.monitors.length > 0}
          <div
            class="flex flex-wrap gap-x-4 gap-y-1 border-t border-neutral-100 pt-4 text-xs text-neutral-600"
          >
            <span>Disponibilidad diaria:</span>
            {#each UPTIME_LEVELS as level (level.level)}
              <span class="inline-flex items-center gap-1.5">
                <span
                  class="inline-block size-2.5 rounded-sm"
                  style:background="var(--status-{level.level})"
                ></span>
                {level.label}
              </span>
            {/each}
            <button
              class="ml-auto underline decoration-dotted underline-offset-2 hover:text-neutral-900"
              onclick={() => (showFailures = !showFailures)}
            >
              {showFailures ? 'Ocultar días con fallas' : 'Ver días con fallas'}
            </button>
          </div>
          {#if showFailures}
            {#if failureDays.length === 0}
              <p class="text-xs text-neutral-500">Ningún día con fallas en los últimos 90 días.</p>
            {:else}
              <table class="w-full text-left text-xs tabular-nums">
                <thead class="text-neutral-500">
                  <tr>
                    <th class="py-1 font-medium">Día</th>
                    <th class="py-1 font-medium">Monitor</th>
                    <th class="py-1 font-medium">Disponibilidad</th>
                    <th class="py-1 font-medium">Fallas</th>
                  </tr>
                </thead>
                <tbody>
                  {#each failureDays as day (day.monitor + day.day)}
                    <tr class="border-t border-neutral-100 text-neutral-700">
                      <td class="py-1">{day.day}</td>
                      <td class="py-1">{day.monitor}</td>
                      <td class="py-1">{formatPercent(day.uptime)}</td>
                      <td class="py-1">{day.failures} de {day.checks}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            {/if}
          {/if}
        {/if}
      </section>

      {#if pastIncidents.length > 0}
        <section class="mt-6">
          <h2 class="text-sm font-medium text-neutral-900">Incidentes de los últimos 7 días</h2>
          <ul class="mt-2 space-y-1 text-sm text-neutral-600">
            {#each pastIncidents as incident (incident.monitor_name + incident.started_at)}
              <li>
                {incident.monitor_name}: {formatDateTime(incident.started_at)}, resuelto tras
                {formatSeconds(incident.duration_s)}
              </li>
            {/each}
          </ul>
        </section>
      {/if}

      <p class="mt-10 text-center text-xs text-neutral-400">
        Actualizado {formatDateTime(status.generated_at)} · con taut
      </p>
    {/if}
  </div>
</main>
