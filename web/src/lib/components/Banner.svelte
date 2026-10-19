<script lang="ts">
  import type { Snippet } from 'svelte';
  import CheckCircleIcon from 'phosphor-svelte/lib/CheckCircleIcon';
  import InfoIcon from 'phosphor-svelte/lib/InfoIcon';
  import WarningIcon from 'phosphor-svelte/lib/WarningIcon';
  import XCircleIcon from 'phosphor-svelte/lib/XCircleIcon';

  import { bannerClass } from '#lib/ui.js';

  type BannerTone = 'info' | 'success' | 'warning' | 'danger';

  let {
    tone = 'info',
    large = false,
    class: className = '',
    children,
  }: { tone?: BannerTone; large?: boolean; class?: string; children: Snippet } = $props();

  const ICONS = {
    info: InfoIcon,
    success: CheckCircleIcon,
    warning: WarningIcon,
    danger: XCircleIcon,
  } as const;

  const Icon = $derived(ICONS[tone]);
</script>

<!-- It rises into place when it appears: a notice or an error after an action. -->
<div
  class={[bannerClass(tone, large), 'animate-rise-in', className]}
  role={tone === 'danger' ? 'alert' : tone === 'success' ? 'status' : undefined}
>
  <span class="flex h-lh shrink-0 items-center"
    ><Icon size={large ? 20 : 16} aria-hidden="true" /></span
  >
  <div class="min-w-0">{@render children()}</div>
</div>
