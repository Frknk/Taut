<script lang="ts">
  import { onMount } from 'svelte';

  import LatencyChart from '#lib/components/LatencyChart.svelte';
  import { describeError, request } from '#lib/api.js';
  import { formatLatency, formatPercent } from '#lib/charts.js';
  import type { Metrics, MetricsRange } from '#lib/types.js';

  let { monitorId, tenantId }: { monitorId: string; tenantId: string } = $props();

  const RANGES: { value: MetricsRange; label: string; ms: number }[] = [
    { value: '24h', label: '24 h', ms: 86_400_000 },
    { value: '7d', label: '7 días', ms: 7 * 86_400_000 },
    { value: '30d', label: '30 días', ms: 30 * 86_400_000 },
  ];

  let range = $state<MetricsRange>('24h');
  let metrics = $state<Metrics | null>(null);
  let loading = $state(false);
  let error = $state('');
  let now = $state(Date.now());

  async function load() {
    loading = true;
    try {
      metrics = await request<Metrics>('GET', `/v1/monitors/${monitorId}/metrics?range=${range}`, {
        tenant: tenantId,
      });
      now = Date.now();
      error = '';
    } catch (err) {
      error = describeError(err);
    } finally {
      loading = false;
    }
  }

  function select(next: MetricsRange) {
    range = next;
    void load();
  }

  onMount(() => {
    void load();
    const timer = setInterval(load, 60_000);
    return () => clearInterval(timer);
  });

  const windowMs = $derived(RANGES.find((r) => r.value === range)?.ms ?? 86_400_000);
  const tab = 'rounded-lg px-3 py-1 text-sm';
  const tile = 'rounded-xl border border-neutral-200 bg-white px-4 py-3';
</script>

<section class="mt-6">
  <div class="flex flex-wrap items-center justify-between gap-3">
    <h2 class="text-sm font-medium text-neutral-900">Latencia y disponibilidad</h2>
    <div class="flex gap-1 rounded-lg bg-neutral-100 p-1">
      {#each RANGES as option (option.value)}
        <button
          class={[tab, range === option.value ? 'bg-white shadow-xs' : 'text-neutral-500']}
          onclick={() => select(option.value)}
        >
          {option.label}
        </button>
      {/each}
    </div>
  </div>

  {#if error}
    <p class="mt-3 rounded-lg bg-red-50 px-4 py-3 text-sm text-red-700">{error}</p>
  {/if}

  {#if metrics}
    <div class={['mt-3 transition-opacity', loading && 'opacity-60']}>
      <div class="grid grid-cols-2 gap-3 sm:grid-cols-5">
        {#each RANGES as option (option.value)}
          <div class={tile}>
            <p class="text-xs text-neutral-500">Disponibilidad {option.label}</p>
            <p class="mt-1 text-lg font-semibold text-neutral-900">
              {formatPercent(metrics.uptime[option.value])}
            </p>
          </div>
        {/each}
        <div class={tile}>
          <p class="text-xs text-neutral-500">Latencia p50</p>
          <p class="mt-1 text-lg font-semibold text-neutral-900">
            {formatLatency(metrics.latency.p50_ms)}
          </p>
        </div>
        <div class={tile}>
          <p class="text-xs text-neutral-500">Latencia p95</p>
          <p class="mt-1 text-lg font-semibold text-neutral-900">
            {formatLatency(metrics.latency.p95_ms)}
          </p>
        </div>
      </div>

      <div class="mt-3 rounded-xl border border-neutral-200 bg-white p-4">
        {#if metrics.series.length === 0}
          <p class="py-10 text-center text-sm text-neutral-500">Sin chequeos en este periodo.</p>
        {:else}
          <LatencyChart
            points={metrics.series}
            bucketS={metrics.bucket_s}
            from={now - windowMs}
            to={now}
          />
        {/if}
      </div>
      <p class="mt-2 text-xs text-neutral-500">
        {metrics.checks} chequeos, {metrics.failures} con falla. La latencia solo cuenta los chequeos
        sanos.
      </p>
    </div>
  {/if}
</section>
