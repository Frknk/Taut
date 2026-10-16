<script lang="ts">
  import type { ClassValue } from 'svelte/elements';

  /**
   * The taut mark alone, in one ink (`currentColor`). The rounded ground only exists in the
   * favicon. Without `label` the mark is decorative and the surrounding link must name it.
   */
  let { class: className = 'w-6', label }: { class?: ClassValue; label?: string } = $props();
</script>

<svg
  class={['block fill-current', className]}
  viewBox="50 33 162 155"
  role={label ? 'img' : undefined}
  aria-label={label}
  aria-hidden={label ? undefined : 'true'}
>
  <polygon points="68,95 50,64 68,33 176,33 194,64 86,64" />
  <polygon points="68,95 104,95 68,157 50,126" />
  <polygon points="194,64 212,95 194,126 158,126 122,188 86,188 68,157 104,157 140,95 176,95" />
</svg>
