<script lang="ts">
  import type { Snippet } from 'svelte';

  import { step } from '#lib/motion.js';

  /**
   * One step of a card that changes in place (login → 2FA, form → «sent»). The new step slides
   * in 8px from the side it moves towards; the first one shows without motion. Its children keep
   * the card's 24px rhythm. `back` comes in from the other side, for «Volver a empezar».
   */
  let { back = false, children }: { back?: boolean; children: Snippet } = $props();
</script>

<div class="space-y-6" in:step={{ back }}>
  {@render children()}
</div>
