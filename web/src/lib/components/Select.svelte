<script lang="ts" generics="T">
  import type { Snippet } from 'svelte';
  import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon';

  import { inputBox } from '#lib/ui.js';

  /** A native `<select>` with the look of an input and a caret, 6px below its label. */
  let {
    id,
    value = $bindable(),
    disabled = false,
    children,
  }: { id: string; value: T; disabled?: boolean; children: Snippet } = $props();
</script>

<span class="relative mt-1.5 block">
  <select {id} class={[inputBox, 'cursor-pointer appearance-none pr-9']} bind:value {disabled}>
    {@render children()}
  </select>
  <CaretDownIcon
    size={14}
    class="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-subtle"
    aria-hidden="true"
  />
</span>
