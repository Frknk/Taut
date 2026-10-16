<script lang="ts">
  import type { Snippet } from 'svelte';

  let { title, text, children }: { title: string; text?: string; children?: Snippet } = $props();
</script>

<div
  class="flex flex-col items-center gap-1 rounded-lg border border-dashed border-interact px-6 py-10 text-center"
>
  <p class="text-base font-semibold text-strong">{title}</p>
  {#if text}<p class="max-w-[44ch] text-subtle">{text}</p>{/if}
  {#if children}<div class="mt-3">{@render children()}</div>{/if}
</div>
