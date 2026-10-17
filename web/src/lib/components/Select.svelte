<script module lang="ts">
  /** One option: the value it sets and the text shown. */
  export interface SelectItem<V> {
    value: V;
    label: string;
    disabled?: boolean;
  }
</script>

<script lang="ts" generics="T extends string | number">
  import { Select } from 'bits-ui';
  import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon';
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon';

  import { inputBox, menu } from '#lib/ui.js';

  /**
   * A select on bits-ui: a button with the look of an input and a floating list of options,
   * 6px below its label (`<label for={id}>` names it). Arrow keys, Home, End and typing move
   * between options. `inline` drops that gap, for a select inside a table row; `onchange` runs
   * after `value` updated.
   */
  let {
    id,
    items,
    value = $bindable(),
    name,
    disabled = false,
    inline = false,
    onchange,
  }: {
    id: string;
    items: SelectItem<T>[];
    value: T;
    /** For a native form submit: bits-ui keeps a hidden input with this name. */
    name?: string;
    disabled?: boolean;
    inline?: boolean;
    onchange?: (value: T) => void;
  } = $props();

  // bits-ui works with strings; numbers (intervals) go through String and back.
  const options = $derived(
    items.map((item) => ({ ...item, value: String(item.value), raw: item.value })),
  );
  const selected = $derived(options.find((option) => option.raw === value));
  let open = $state(false);

  function change(next: string) {
    const option = options.find((o) => o.value === next);
    if (!option || option.raw === value) return;
    value = option.raw;
    onchange?.(option.raw);
  }
</script>

<Select.Root
  type="single"
  bind:open
  value={selected?.value ?? ''}
  onValueChange={change}
  items={options}
  {name}
  {disabled}
>
  <!-- A combobox, not a plain button: screen readers announce the chosen option with the label. -->
  <Select.Trigger
    {id}
    role="combobox"
    aria-controls={open ? `${id}-listbox` : undefined}
    class={[
      inputBox,
      'flex cursor-pointer items-center justify-between gap-2 pr-3 text-left',
      !inline && 'mt-1.5',
    ]}
  >
    <span class="truncate">{selected?.label}</span>
    <CaretDownIcon size={14} class="shrink-0 text-subtle" aria-hidden="true" />
  </Select.Trigger>
  <Select.Portal>
    <!-- The list takes its name from the trigger, which takes it from the label. -->
    <Select.Content id="{id}-listbox" aria-labelledby={id} sideOffset={4} class={menu.panel}>
      <Select.Viewport>
        {#each options as option (option.value)}
          <Select.Item
            value={option.value}
            label={option.label}
            disabled={option.disabled}
            class={menu.item}
          >
            {#snippet children({ selected: isSelected })}
              <span class="flex-1">{option.label}</span>
              <CheckIcon size={14} class={isSelected ? '' : 'invisible'} aria-hidden="true" />
            {/snippet}
          </Select.Item>
        {/each}
      </Select.Viewport>
    </Select.Content>
  </Select.Portal>
</Select.Root>
