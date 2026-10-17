<script lang="ts">
  import { DropdownMenu } from 'bits-ui';
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon';
  import type { Snippet } from 'svelte';

  import { menu } from '#lib/ui.js';

  interface Props {
    value: string;
    /** Typeahead text, when the item shows more than its name. */
    textValue?: string;
    /** `false` keeps the menu open, to try the options one after another (the theme). */
    closeOnSelect?: boolean;
    children: Snippet;
  }

  /**
   * One option of a `DropdownMenu.RadioGroup` inside a `Menu`: `menuitemradio`, with a check
   * at the end when it is the chosen one.
   */
  let { value, textValue, closeOnSelect = true, children: label }: Props = $props();
</script>

<DropdownMenu.RadioItem {value} {textValue} {closeOnSelect} class={menu.item}>
  {#snippet children({ checked })}
    {@render label()}
    <CheckIcon size={14} class={checked ? 'ml-auto' : 'invisible ml-auto'} aria-hidden="true" />
  {/snippet}
</DropdownMenu.RadioItem>
