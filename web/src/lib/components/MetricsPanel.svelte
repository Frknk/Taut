<script lang="ts">
  import { onMount } from 'svelte';

  import Banner from '#lib/components/Banner.svelte';
  import LatencyChart from '#lib/components/LatencyChart.svelte';
  import LayerCard from '#lib/components/LayerCard.svelte';
  import TabList from '#lib/components/TabList.svelte';
  import TabPanel from '#lib/components/TabPanel.svelte';
  import Tabs from '#lib/components/Tabs.svelte';
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

  onMount(() => {
    void load();
    const timer = setInterval(load, 60_000);
    return () => clearInterval(timer);
  });

  const windowMs = $derived(RANGES.find((r) => r.value === range)?.ms ?? 86_400_000);
</script>

<section class="mt-6">
  <Tabs bind:value={range} onchange={() => void load()}>
    <div class="flex flex-wrap items-center justify-between gap-3">
      <h2 class="text-base font-semibold text-strong">Latencia y disponibilidad</h2>
      <TabList options={RANGES} label="Rango" />
    </div>
    <TabPanel value={range}>
      {#if error}
        <Banner tone="danger" class="mt-3">{error}</Banner>
      {/if}

      {#if metrics}
        <div class={['mt-3', loading && 'opacity-60']}>
          <LayerCard flush>
            <div class="grid grid-cols-[repeat(auto-fit,minmax(9rem,1fr))]">
              {#each RANGES as option (option.value)}
                <div
                  class="flex flex-col gap-1 p-4 shadow-[inset_1px_0_0_var(--hairline)] first:shadow-none"
                >
                  <span class="text-xs text-subtle">Disponibilidad {option.label}</span>
                  <span class="text-3xl font-semibold text-strong">
                    {formatPercent(metrics.uptime[option.value])}
                  </span>
                </div>
              {/each}
              <div class="flex flex-col gap-1 p-4 shadow-[inset_1px_0_0_var(--hairline)]">
                <span class="text-xs text-subtle">Latencia p50</span>
                <span class="text-3xl font-semibold text-strong">
                  {formatLatency(metrics.latency.p50_ms)}
                </span>
              </div>
              <div class="flex flex-col gap-1 p-4 shadow-[inset_1px_0_0_var(--hairline)]">
                <span class="text-xs text-subtle">Latencia p95</span>
                <span class="text-3xl font-semibold text-strong">
                  {formatLatency(metrics.latency.p95_ms)}
                </span>
              </div>
            </div>
          </LayerCard>

          <LayerCard class="mt-3">
            {#if metrics.series.length === 0}
              <p class="py-10 text-center text-subtle">Sin chequeos en este periodo.</p>
            {:else}
              <LatencyChart
                points={metrics.series}
                bucketS={metrics.bucket_s}
                from={now - windowMs}
                to={now}
              />
            {/if}
          </LayerCard>
          <p class="mt-2 text-xs text-subtle">
            {metrics.checks} chequeos, {metrics.failures} con falla. La latencia solo cuenta los chequeos
            sanos.
          </p>
        </div>
      {/if}
    </TabPanel>
  </Tabs>
</section>
