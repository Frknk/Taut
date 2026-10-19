<script lang="ts">
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon';

  import { pop } from '#lib/motion.js';

  /**
   * The label of a copy button: «Copiar URL» until it copied, then a check and «Copiada» for a
   * moment. The new label pops in place, so the confirmation is seen where the click was.
   */
  let { copied, label, done }: { copied: boolean; label: string; done: string } = $props();
</script>

{#key copied}
  <span class="inline-flex items-center gap-1" in:pop>
    {#if copied}<CheckIcon size={14} aria-hidden="true" />{/if}
    {copied ? done : label}
  </span>
{/key}
