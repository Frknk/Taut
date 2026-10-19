<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { ClassValue } from 'svelte/elements';

  import { badgeClass, type Tone } from '#lib/ui.js';

  let {
    tone = 'neutral',
    class: className,
    children,
  }: { tone?: Tone; class?: ClassValue; children: Snippet } = $props();
</script>

<span class={[badgeClass(tone), className]}>{@render children()}</span>
