<script lang="ts">
  import Badge from '#lib/components/Badge.svelte';
  import { STATUS_GLYPHS, STATUS_LABELS, STATUS_TONES } from '#lib/format.js';
  import type { MonitorStatus } from '#lib/types.js';

  /** `label` replaces the panel wording, as the public status page does. */
  let { status, label }: { status: MonitorStatus; label?: string } = $props();

  const glyph = $derived(STATUS_GLYPHS[status]);
</script>

<Badge tone={STATUS_TONES[status]}>
  {#if glyph === 'dot'}
    <span
      class={['size-1.5 rounded-full', status === 'up' ? 'bg-success' : 'bg-danger']}
      aria-hidden="true"
    ></span>
  {:else if glyph === 'pause'}
    <span class="flex h-2 w-1.5 justify-between" aria-hidden="true">
      <span class="w-0.5 bg-current"></span><span class="w-0.5 bg-current"></span>
    </span>
  {:else}
    <span class="size-2 rounded-full ring-[1.5px] ring-current ring-inset" aria-hidden="true"
    ></span>
  {/if}
  {label ?? STATUS_LABELS[status]}
</Badge>
