<script lang="ts">
  import type { Snippet } from 'svelte';

  import { badgeClass, type Tone } from '#lib/ui.js';

  let { tone = 'neutral', children }: { tone?: Tone; children: Snippet } = $props();
</script>

<span class={badgeClass(tone)}>{@render children()}</span>
