<script lang="ts">
  import { tick, type Snippet } from 'svelte';
  import type { ClassValue } from 'svelte/elements';

  import { focusRing, menu } from '#lib/ui.js';

  interface Props {
    /** Names the menu for screen readers: «Organizaciones», «Cuenta». */
    label: string;
    /** Content of the button that opens the menu. */
    trigger: Snippet;
    triggerClass?: ClassValue;
    /** `above` for a menu at the bottom of the screen. */
    placement?: 'below' | 'above';
    /** Runs every time the menu opens: refresh what it lists. */
    onopen?: () => void;
    /** The items (`role="menuitem"` or `menuitemradio`), with a function that closes the menu. */
    children: Snippet<[close: () => void]>;
  }

  /**
   * Dropdown menu: a button and a floating list of actions. Arrow keys move between items,
   * Escape, Tab or a click outside close it, and the focus goes back to the button.
   */
  let { label, trigger, triggerClass, placement = 'below', onopen, children }: Props = $props();

  const id = $props.id();
  let open = $state(false);
  let root = $state<HTMLDivElement>();
  let button = $state<HTMLButtonElement>();
  let panel = $state<HTMLDivElement>();

  function items(): HTMLElement[] {
    return panel ? [...panel.querySelectorAll<HTMLElement>('[role^="menuitem"]')] : [];
  }

  async function show() {
    open = true;
    onopen?.();
    await tick();
    const all = items();
    (all.find((item) => item.getAttribute('aria-checked') === 'true') ?? all[0])?.focus();
  }

  function close(refocus = true) {
    if (!open) return;
    open = false;
    if (refocus) button?.focus();
  }

  function keyDown(event: KeyboardEvent) {
    const all = items();
    const index = all.indexOf(document.activeElement as HTMLElement);
    if (event.key === 'Escape') {
      event.preventDefault();
      close();
    } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      const step = event.key === 'ArrowDown' ? 1 : -1;
      all[(index + step + all.length) % all.length]?.focus();
    } else if (event.key === 'Home' || event.key === 'End') {
      event.preventDefault();
      (event.key === 'Home' ? all[0] : all.at(-1))?.focus();
    } else if (event.key === 'Tab') {
      close(false);
    }
  }

  function triggerKeyDown(event: KeyboardEvent) {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      void show();
    }
  }

  function pointerDown(event: PointerEvent) {
    if (open && root && !root.contains(event.target as Node)) close(false);
  }
</script>

<svelte:window onpointerdown={pointerDown} />

<div class="relative" bind:this={root}>
  <button
    bind:this={button}
    type="button"
    class={['cursor-pointer', focusRing, triggerClass]}
    aria-haspopup="menu"
    aria-expanded={open}
    aria-controls={open ? `${id}-menu` : undefined}
    onclick={() => (open ? close() : void show())}
    onkeydown={triggerKeyDown}
  >
    {@render trigger()}
  </button>
  {#if open}
    <!-- The keys move the focus between the items: the listener sits on their container. -->
    <!-- svelte-ignore a11y_interactive_supports_focus -->
    <div
      bind:this={panel}
      id="{id}-menu"
      role="menu"
      aria-label={label}
      class={[menu.panel, placement === 'above' ? 'bottom-full mb-1' : 'top-full mt-1', 'left-0']}
      onkeydown={keyDown}
    >
      {@render children(() => close())}
    </div>
  {/if}
</div>
