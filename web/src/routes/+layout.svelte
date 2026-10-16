<script lang="ts">
  import '@fontsource-variable/geist';
  import '@fontsource-variable/geist-mono';
  import './layout.css';
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

{#if session.status === 'booting'}
  <div class="flex min-h-screen items-center justify-center text-sm text-subtle" role="status">
    Cargando…
  </div>
{:else}
  {@render children()}
{/if}
