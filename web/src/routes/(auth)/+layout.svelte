<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import type { Snippet } from 'svelte';

  import Logo from '#lib/components/Logo.svelte';
  import { session } from '#lib/session.svelte.js';

  let { children }: { children: Snippet } = $props();

  // Signed in already: these pages make no sense, go pick an organization.
  $effect(() => {
    if (session.status === 'authenticated') void goto(resolve('/'));
  });
</script>

<main class="flex min-h-screen items-center justify-center p-6">
  <section class="w-full max-w-md space-y-6 rounded-xl bg-base p-8 ring ring-hairline">
    <Logo class="w-10 text-strong" label="taut" />
    {@render children()}
  </section>
</main>
