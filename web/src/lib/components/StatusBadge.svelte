<script lang="ts">
  import { untrack } from 'svelte';

  import Badge from '#lib/components/Badge.svelte';
  import { STATUS_GLYPHS, STATUS_LABELS, STATUS_TONES } from '#lib/format.js';
  import type { MonitorStatus } from '#lib/types.js';

  /** `label` replaces the panel wording, as the public status page does. */
  let { status, label }: { status: MonitorStatus; label?: string } = $props();

  const glyph = $derived(STATUS_GLYPHS[status]);

  // A poll that changes the status pings the glyph once; the first value does not.
  let previous = untrack(() => status);
  let changes = $state(0);
  $effect(() => {
    if (status === previous) return;
    previous = status;
    changes += 1;
  });
</script>

<!-- The tones cross-fade too: a status change is not a hover, so it may take 300ms. -->
<Badge tone={STATUS_TONES[status]} class="transition-colors duration-300">
  {#key changes}
    <span class={['flex items-center', changes > 0 && 'animate-ping-once']} aria-hidden="true">
      {#if glyph === 'dot'}
        <span class={['size-1.5 rounded-full', status === 'up' ? 'bg-success' : 'bg-danger']}
        ></span>
      {:else if glyph === 'pause'}
        <span class="flex h-2 w-1.5 justify-between">
          <span class="w-0.5 bg-current"></span><span class="w-0.5 bg-current"></span>
        </span>
      {:else}
        <span class="size-2 rounded-full ring-[1.5px] ring-current ring-inset"></span>
      {/if}
    </span>
  {/key}
  {label ?? STATUS_LABELS[status]}
</Badge>
