<script lang="ts">
  import type { ClassValue } from 'svelte/elements';

  /**
   * The Taut mark alone, in one ink (`currentColor`). The rounded ground only exists in the
   * favicon. Without `label` the mark is decorative and the surrounding link must name it.
   * `animated` builds it piece by piece and then breathes, for loading screens only.
   */
  let {
    class: className = 'w-6',
    label,
    animated = false,
  }: { class?: ClassValue; label?: string; animated?: boolean } = $props();

  // Each piece rises 60ms after the previous one; the breathing waits for the last.
  const piece = (index: number) => (animated ? `animation-delay: ${index * 60}ms` : undefined);
</script>

<svg
  class={[
    'block fill-current',
    animated &&
      'animate-breathe [animation-delay:400ms] *:animate-rise-in *:[animation-fill-mode:backwards]',
    className,
  ]}
  viewBox="50 33 162 155"
  role={label ? 'img' : undefined}
  aria-label={label}
  aria-hidden={label ? undefined : 'true'}
>
  <polygon points="68,95 50,64 68,33 176,33 194,64 86,64" style={piece(0)} />
  <polygon points="68,95 104,95 68,157 50,126" style={piece(1)} />
  <polygon
    points="194,64 212,95 194,126 158,126 122,188 86,188 68,157 104,157 140,95 176,95"
    style={piece(2)}
  />
</svg>
