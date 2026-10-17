<script lang="ts">
  import '@fontsource-variable/geist';
  import '@fontsource-variable/geist-mono';
  import './layout.css';
  import { Tooltip } from 'bits-ui';
  import { onMount } from 'svelte';
  import favicon from '#lib/assets/favicon.svg';
  import { session } from '#lib/session.svelte.js';
  import type { LayoutProps } from './$types';

  let { children }: LayoutProps = $props();

  // One refresh with the cookie decides whether there is a session before any page renders.
  onMount(() => void session.boot());
</script>

<svelte:head>
  <link rel="icon" href={favicon} />
</svelte:head>

<!-- One provider: moving between tooltips skips the delay. -->
<Tooltip.Provider delayDuration={300}>
  {#if session.status === 'booting'}
    <div class="flex min-h-screen items-center justify-center text-sm text-subtle" role="status">
      Cargando…
    </div>
  {:else}
    {@render children()}
  {/if}
</Tooltip.Provider>
