<script lang="ts">
  import type { ClassValue } from 'svelte/elements';

  /**
   * A check that draws itself once: the ring, then the tick. Only for rare successes (an email
   * confirmed, 2FA turned on); the text next to it says what happened.
   */
  let { class: className = 'size-10 text-success' }: { class?: ClassValue } = $props();
</script>

<svg
  class={['shrink-0', className]}
  viewBox="0 0 40 40"
  fill="none"
  stroke="currentColor"
  stroke-width="2.5"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <!-- pathLength=1 lets one keyframe draw any path: the dash goes from 1 (hidden) to 0. -->
  <circle cx="20" cy="20" r="17" pathLength="1" class="animate-draw [stroke-dasharray:1]" />
  <path
    d="M13 20.5l5 5 9-10"
    pathLength="1"
    class="animate-draw [stroke-dasharray:1] [animation-delay:250ms]"
  />
</svg>
