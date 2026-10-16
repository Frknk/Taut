<script lang="ts" generics="T extends string">
  import { focusRing } from '#lib/ui.js';

  interface Props {
    options: { value: T; label: string }[];
    value: T;
    /** Names the group for screen readers: «Rango», «Filtro». */
    label: string;
    onchange?: (value: T) => void;
  }

  /** Segmented control: sibling views of the same content. Arrow keys move the selection. */
  let { options, value = $bindable(), label, onchange }: Props = $props();

  let buttons = $state<HTMLButtonElement[]>([]);

  function select(index: number) {
    const next = options[index];
    if (!next) return;
    value = next.value;
    onchange?.(next.value);
    buttons[index]?.focus();
  }

  function keyDown(event: KeyboardEvent, index: number) {
    if (event.key !== 'ArrowRight' && event.key !== 'ArrowLeft') return;
    event.preventDefault();
    const step = event.key === 'ArrowRight' ? 1 : -1;
    select((index + step + options.length) % options.length);
  }
</script>

<div
  class="inline-flex h-9 max-w-full items-stretch overflow-x-auto rounded-lg bg-recessed px-0.5"
  role="tablist"
  aria-label={label}
>
  {#each options as option, index (option.value)}
    {@const selected = option.value === value}
    <button
      bind:this={buttons[index]}
      type="button"
      role="tab"
      aria-selected={selected}
      tabindex={selected ? 0 : -1}
      class={[
        'my-0.5 flex cursor-pointer items-center rounded-md px-2.5 text-sm whitespace-nowrap',
        focusRing,
        selected
          ? 'bg-base text-default shadow-xs ring ring-line'
          : 'text-subtle hover:text-default',
      ]}
      onclick={() => select(index)}
      onkeydown={(event) => keyDown(event, index)}
    >
      {option.label}
    </button>
  {/each}
</div>
