<script lang="ts">
  import { DropdownMenu } from 'bits-ui';
  import type { Snippet } from 'svelte';
  import type { ClassValue } from 'svelte/elements';

  import { focusRing, menu } from '#lib/ui.js';

  interface Props {
    /** Names the menu for screen readers: «Organizaciones», «Cuenta». */
    label: string;
    /** Content of the button that opens the menu. */
    trigger: Snippet;
    triggerClass?: ClassValue;
    /** `top` for a menu at the bottom of the screen; it flips by itself when there is no room. */
    side?: 'top' | 'bottom';
    /** Runs every time the menu opens: refresh what it lists. */
    onopen?: () => void;
    /** The items: `MenuItem`, `MenuRadioItem` inside `DropdownMenu.RadioGroup`, separators. */
    children: Snippet;
  }

  /**
   * Dropdown menu on bits-ui: a button and a floating list of actions. Arrow keys, Home, End and
   * typing a name move between items; Escape, Tab or a click outside close it and the focus goes
   * back to the button.
   */
  let { label, trigger, triggerClass, side = 'bottom', onopen, children }: Props = $props();
</script>

<DropdownMenu.Root onOpenChange={(open) => open && onopen?.()}>
  <DropdownMenu.Trigger class={['cursor-pointer', focusRing, triggerClass]}>
    {@render trigger()}
  </DropdownMenu.Trigger>
  <DropdownMenu.Portal>
    <DropdownMenu.Content aria-label={label} {side} align="start" sideOffset={4} class={menu.panel}>
      {@render children()}
    </DropdownMenu.Content>
  </DropdownMenu.Portal>
</DropdownMenu.Root>
