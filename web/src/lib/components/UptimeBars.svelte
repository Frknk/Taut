<script lang="ts">
  import { formatPercent, uptimeLevel } from '#lib/charts.js';
  import type { DayBar } from '#lib/types.js';

  let { days, label }: { days: DayBar[]; label: string } = $props();

  const HEIGHT = 32;
  const GAP = 2;

  let width = $state(600);
  let active = $state<number | null>(null);

  const slot = $derived(width / Math.max(days.length, 1));
  const barWidth = $derived(Math.max(1, Math.min(24, slot - GAP)));

  function formatDay(day: string): string {
    return new Date(`${day}T00:00:00Z`).toLocaleDateString('es', {
      day: 'numeric',
      month: 'short',
      year: 'numeric',
      timeZone: 'UTC',
    });
  }

  function keyDown(event: KeyboardEvent) {
    if (event.key === 'ArrowRight' || event.key === 'ArrowLeft') {
      event.preventDefault();
      const step = event.key === 'ArrowRight' ? 1 : -1;
      const start = active ?? days.length;
      active = Math.min(days.length - 1, Math.max(0, start + (active === null ? -1 : step)));
    } else if (event.key === 'Escape') {
      active = null;
    }
  }

  function pointerMove(event: PointerEvent) {
    const bounds = (event.currentTarget as SVGSVGElement).getBoundingClientRect();
    const index = Math.floor((event.clientX - bounds.left) / slot);
    active = index >= 0 && index < days.length ? index : null;
  }

  const valueText = $derived.by(() => {
    const day = days[active ?? days.length - 1];
    if (!day) return 'Sin datos';
    if (day.checks === 0) return `${formatDay(day.day)}: sin datos`;
    return `${formatDay(day.day)}: ${formatPercent(day.uptime)} disponible, ${day.failures} de ${day.checks} chequeos fallaron`;
  });

  const tooltipLeft = $derived(
    active === null ? 0 : Math.min(Math.max(active * slot - 80, 0), width - 176),
  );
</script>

<div class="relative" bind:clientWidth={width}>
  <svg
    {width}
    height={HEIGHT}
    role="slider"
    aria-label="{label}: disponibilidad diaria de los últimos 90 días. Flechas para recorrer los días."
    aria-valuemin={0}
    aria-valuemax={Math.max(days.length - 1, 0)}
    aria-valuenow={active ?? days.length - 1}
    aria-valuetext={valueText}
    tabindex="0"
    class="block touch-none outline-none focus-visible:ring-2 focus-visible:ring-neutral-400"
    onpointermove={pointerMove}
    onpointerleave={() => (active = null)}
    onkeydown={keyDown}
    onblur={() => (active = null)}
  >
    {#each days as day, index (day.day)}
      <rect
        x={index * slot + (slot - barWidth) / 2}
        y="0"
        width={barWidth}
        height={HEIGHT}
        rx="1.5"
        fill="var(--status-{uptimeLevel(day.uptime)})"
        opacity={active === null || active === index ? 1 : 0.55}
      />
    {/each}
  </svg>
  {#if active !== null}
    {@const day = days[active]}
    <div
      class="pointer-events-none absolute top-10 z-10 w-44 rounded-lg border border-neutral-200 bg-white px-3 py-2 text-xs shadow-sm"
      style:left="{tooltipLeft}px"
    >
      <p class="text-neutral-500">{formatDay(day.day)}</p>
      {#if day.checks === 0}
        <p class="mt-1 text-neutral-700">Sin datos</p>
      {:else}
        <p class="mt-1">
          <span class="font-semibold text-neutral-900">{formatPercent(day.uptime)}</span>
          <span class="text-neutral-600"> disponible</span>
        </p>
        <p class="text-neutral-600">{day.failures} de {day.checks} chequeos fallaron</p>
      {/if}
    </div>
  {/if}
</div>
