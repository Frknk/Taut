<script lang="ts">
  import { STATUS_LABELS, STATUS_TONES } from '#lib/format.js';
  import type { MonitorStatus } from '#lib/types.js';

  let { status }: { status: MonitorStatus } = $props();
</script>

<span
  class={[
    'inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-medium',
    STATUS_TONES[status],
  ]}
>
  <span class="size-1.5 rounded-full bg-current"></span>
  {STATUS_LABELS[status]}
</span>
