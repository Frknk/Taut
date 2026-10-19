<script lang="ts">
  import { Toggle } from 'bits-ui';
  import TableIcon from 'phosphor-svelte/lib/TableIcon';
  import { formatLatency, nearestIndex, niceCeil, scale, segments, ticks } from '#lib/charts.js';
  import { appear } from '#lib/motion.js';
  import { buttonClass, focusRing } from '#lib/ui.js';
  import type { MetricsPoint } from '#lib/types.js';

  interface Props {
    points: MetricsPoint[];
    bucketS: number;
    /** Start and end of the window, in ms since the epoch. */
    from: number;
    to: number;
  }

  let { points, bucketS, from, to }: Props = $props();

  const HEIGHT = 220;
  const MARGIN = { top: 12, right: 64, bottom: 28, left: 48 };
  const SERIES = [
    { key: 'avg_ms', label: 'Promedio', color: 'var(--viz-series-1)' },
    { key: 'p95_ms', label: 'p95', color: 'var(--viz-series-2)' },
  ] as const;

  let width = $state(640);
  let active = $state<number | null>(null);
  let showTable = $state(false);

  const plotRight = $derived(width - MARGIN.right);
  const plotBottom = HEIGHT - MARGIN.bottom;
  const maxValue = $derived(
    niceCeil(Math.max(0, ...points.map((p) => Math.max(p.avg_ms ?? 0, p.p95_ms ?? 0)))),
  );
  const x = $derived(scale(from, to, MARGIN.left, plotRight));
  const y = $derived(scale(0, maxValue, plotBottom, MARGIN.top));
  const times = $derived(points.map((p) => Date.parse(p.t) + (bucketS * 1000) / 2));
  const yTicks = $derived(ticks(maxValue));
  const xTicks = $derived(ticks(to - from, 4).map((offset) => from + offset));
  const shortRange = $derived(to - from <= 2 * 86_400_000);

  function path(run: { t: number; v: number }[]): string {
    const half = (bucketS * 1000) / 2;
    return run
      .map((p, i) => `${i === 0 ? 'M' : 'L'}${x(p.t + half).toFixed(1)},${y(p.v).toFixed(1)}`)
      .join(' ');
  }

  const lines = $derived(
    SERIES.map((series) => ({
      ...series,
      runs: segments(points, (p) => p[series.key], bucketS * 1000),
    })),
  );

  /** Last value of each series, labelled at the end unless the labels would touch. */
  const endLabels = $derived.by(() => {
    const last = points.toReversed();
    const labels = SERIES.flatMap((series) => {
      const point = last.find((p) => p[series.key] !== null);
      if (!point) return [];
      const value = point[series.key] as number;
      return [{ ...series, value, y: y(value) }];
    });
    if (labels.length === 2 && Math.abs(labels[0].y - labels[1].y) < 14) {
      return labels.filter((label) => label.key === 'p95_ms');
    }
    return labels;
  });

  function formatTick(time: number): string {
    const date = new Date(time);
    return shortRange
      ? date.toLocaleTimeString('es', { hour: '2-digit', minute: '2-digit' })
      : date.toLocaleDateString('es', { day: '2-digit', month: '2-digit' });
  }

  function formatBucket(index: number): string {
    const start = new Date(points[index].t);
    const end = new Date(start.getTime() + bucketS * 1000);
    const options: Intl.DateTimeFormatOptions = shortRange
      ? { hour: '2-digit', minute: '2-digit' }
      : { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' };
    return `${start.toLocaleString('es', options)} – ${end.toLocaleTimeString('es', {
      hour: '2-digit',
      minute: '2-digit',
    })}`;
  }

  function pointerMove(event: PointerEvent) {
    const svg = event.currentTarget as SVGSVGElement;
    const bounds = svg.getBoundingClientRect();
    const px = event.clientX - bounds.left;
    if (px < MARGIN.left || px > plotRight || times.length === 0) {
      active = null;
      return;
    }
    const time = from + ((px - MARGIN.left) / (plotRight - MARGIN.left)) * (to - from);
    active = nearestIndex(times, time);
  }

  function keyDown(event: KeyboardEvent) {
    if (points.length === 0) return;
    if (event.key === 'ArrowRight' || event.key === 'ArrowLeft') {
      event.preventDefault();
      const step = event.key === 'ArrowRight' ? 1 : -1;
      const start = active ?? (step > 0 ? -1 : points.length);
      active = Math.min(points.length - 1, Math.max(0, start + step));
    } else if (event.key === 'Escape') {
      active = null;
    }
  }

  const valueText = $derived.by(() => {
    const index = active ?? points.length - 1;
    const point = points[index];
    if (!point) return 'Sin datos';
    return `${formatBucket(index)}: promedio ${formatLatency(point.avg_ms)}, p95 ${formatLatency(
      point.p95_ms,
    )}, ${point.failures} de ${point.checks} chequeos fallaron`;
  });

  const tooltipLeft = $derived(
    active === null ? 0 : Math.min(Math.max(x(times[active]) + 12, 0), width - 200),
  );
</script>

<div class="text-xs" style:color="var(--viz-ink-secondary)">
  <div class="flex flex-wrap items-center gap-x-4 gap-y-1">
    {#each SERIES as series (series.key)}
      <span class="inline-flex items-center gap-1.5">
        <span class="inline-block h-0.5 w-4 rounded-full" style:background={series.color}></span>
        {series.label}
      </span>
    {/each}
    <span class="inline-flex items-center gap-1.5">
      <span class="inline-block h-1 w-4 rounded-sm" style:background="var(--status-critical)"
      ></span>
      Intervalos con fallas
    </span>
    <!-- A toggle with a fixed name: pressed, the chart shows as a table. -->
    <Toggle.Root
      bind:pressed={showTable}
      class={[buttonClass('ghost', 'sm'), 'ml-auto data-[state=on]:bg-tint']}
    >
      <TableIcon size={14} aria-hidden="true" />
      Ver tabla
    </Toggle.Root>
  </div>

  {#if showTable}
    <div class="mt-3 max-h-72 overflow-auto" in:appear>
      <table class="w-full text-left tabular-nums">
        <thead class="sticky top-0 border-b border-hairline bg-base text-subtle">
          <tr>
            <th scope="col" class="py-1 pr-4 font-medium">Intervalo</th>
            <th scope="col" class="py-1 pr-4 font-medium">Chequeos</th>
            <th scope="col" class="py-1 pr-4 font-medium">Fallas</th>
            <th scope="col" class="py-1 pr-4 font-medium">Promedio</th>
            <th scope="col" class="py-1 font-medium">p95</th>
          </tr>
        </thead>
        <tbody>
          {#each points as point, index (point.t)}
            <tr class="border-t border-hairline">
              <td class="py-1 pr-4">{formatBucket(index)}</td>
              <td class="py-1 pr-4">{point.checks}</td>
              <td class="py-1 pr-4">{point.failures}</td>
              <td class="py-1 pr-4">{formatLatency(point.avg_ms)}</td>
              <td class="py-1">{formatLatency(point.p95_ms)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    <div class="relative mt-2" bind:clientWidth={width} in:appear>
      <svg
        {width}
        height={HEIGHT}
        role="slider"
        aria-label="Latencia por intervalo, promedio y p95. Flechas para recorrer los intervalos."
        aria-valuemin={0}
        aria-valuemax={Math.max(points.length - 1, 0)}
        aria-valuenow={active ?? points.length - 1}
        aria-valuetext={valueText}
        tabindex="0"
        class={['block touch-none rounded-sm', focusRing]}
        onpointermove={pointerMove}
        onpointerleave={() => (active = null)}
        onkeydown={keyDown}
        onblur={() => (active = null)}
      >
        {#each yTicks as tick (tick)}
          <line
            x1={MARGIN.left}
            x2={plotRight}
            y1={y(tick)}
            y2={y(tick)}
            stroke={tick === 0 ? 'var(--viz-axis)' : 'var(--viz-grid)'}
            stroke-width="1"
          />
          <text
            x={MARGIN.left - 8}
            y={y(tick)}
            text-anchor="end"
            dominant-baseline="middle"
            fill="var(--viz-muted)"
            font-size="12"
            class="tabular-nums">{formatLatency(tick)}</text
          >
        {/each}
        {#each xTicks as tick (tick)}
          <text
            x={x(tick)}
            y={HEIGHT - 8}
            text-anchor={tick === from ? 'start' : tick === to ? 'end' : 'middle'}
            fill="var(--viz-muted)"
            font-size="12"
            class="tabular-nums">{formatTick(tick)}</text
          >
        {/each}

        {#each points as point, index (point.t)}
          {#if point.failures > 0}
            <rect
              x={x(Date.parse(point.t))}
              y={plotBottom - 4}
              width={Math.max(
                2,
                x(Date.parse(point.t) + bucketS * 1000) - x(Date.parse(point.t)) - 2,
              )}
              height="4"
              rx="1"
              fill="var(--status-critical)"
              opacity={active === null || active === index ? 1 : 0.5}
            />
          {/if}
        {/each}

        {#each lines as line (line.key)}
          {#each line.runs as run, i (i)}
            {#if run.length === 1}
              <circle
                cx={x(run[0].t + (bucketS * 1000) / 2)}
                cy={y(run[0].v)}
                r="2.5"
                fill={line.color}
              />
            {:else}
              <path
                d={path(run)}
                fill="none"
                stroke={line.color}
                stroke-width="2"
                stroke-linejoin="round"
                stroke-linecap="round"
              />
            {/if}
          {/each}
        {/each}

        {#each endLabels as label (label.key)}
          <text
            x={plotRight + 8}
            y={label.y}
            dominant-baseline="middle"
            fill="var(--viz-ink-secondary)"
            font-size="12"
            class="tabular-nums">{formatLatency(label.value)}</text
          >
        {/each}

        {#if active !== null}
          <line
            x1={x(times[active])}
            x2={x(times[active])}
            y1={MARGIN.top}
            y2={plotBottom}
            stroke="var(--viz-axis)"
            stroke-width="1"
          />
          {#each SERIES as series (series.key)}
            {@const value = points[active][series.key]}
            {#if value !== null}
              <circle
                cx={x(times[active])}
                cy={y(value)}
                r="4"
                fill={series.color}
                stroke="var(--viz-surface)"
                stroke-width="2"
              />
            {/if}
          {/each}
        {/if}
      </svg>

      {#if active !== null}
        {@const point = points[active]}
        <div
          class="pointer-events-none absolute top-2 w-48 rounded-md bg-base px-3 py-2 shadow-md ring ring-line"
          style:left="{tooltipLeft}px"
        >
          <p style:color="var(--viz-muted)">{formatBucket(active)}</p>
          {#each SERIES as series (series.key)}
            <p class="mt-1 flex items-center gap-2">
              <span class="inline-block h-0.5 w-3 rounded-full" style:background={series.color}
              ></span>
              <span class="font-semibold tabular-nums" style:color="var(--viz-ink)">
                {formatLatency(point[series.key])}
              </span>
              <span>{series.label}</span>
            </p>
          {/each}
          <p class="mt-1">
            <span class="font-semibold tabular-nums" style:color="var(--viz-ink)">
              {point.failures}
            </span>
            de {point.checks} chequeos fallaron
          </p>
        </div>
      {/if}
    </div>
  {/if}
</div>
