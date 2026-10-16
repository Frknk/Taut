<script lang="ts">
  import { onMount } from 'svelte';

  import { API_URL } from '#lib/api.js';
  import { describeHealth, type Health } from '#lib/health.js';

  let health = $state<Health | null>(null);
  let checked = $state(false);

  onMount(async () => {
    try {
      const response = await fetch(`${API_URL}/health`);
      health = response.ok ? ((await response.json()) as Health) : null;
    } catch {
      health = null;
    } finally {
      checked = true;
    }
  });

  const tone = $derived(
    !checked
      ? 'bg-neutral-100 text-neutral-600'
      : health?.status === 'ok'
        ? 'bg-emerald-50 text-emerald-700'
        : 'bg-red-50 text-red-700',
  );
</script>

<svelte:head>
  <title>taut</title>
</svelte:head>

<main class="flex min-h-screen items-center justify-center bg-neutral-100 p-6">
  <section class="w-full max-w-md rounded-xl bg-white p-8 shadow-sm">
    <h1 class="text-2xl font-semibold text-neutral-900">taut</h1>
    <p class="mt-1 text-sm text-neutral-500">Monitoreo de disponibilidad multi-tenant</p>
    <p class={['mt-6 rounded-lg px-4 py-3 text-sm font-medium', tone]}>
      {checked ? describeHealth(health) : 'Comprobando la API…'}
    </p>
    <p class="mt-4 text-xs text-neutral-400">API: {API_URL}</p>
  </section>
</main>
