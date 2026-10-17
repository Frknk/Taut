<script lang="ts" generics="T extends string">
  import { Tabs } from 'bits-ui';
  import type { Snippet } from 'svelte';

  interface Props {
    value: T;
    onchange?: (value: T) => void;
    /** Holds a `TabList` and a `TabPanel`, wherever the layout puts them. */
    children: Snippet;
  }

  /**
   * Tabs on bits-ui: sibling views of the same content. Arrow keys, Home and End move the
   * selection in the `TabList`; the `TabPanel` is the `tabpanel` it controls.
   */
  let { value = $bindable(), onchange, children }: Props = $props();
</script>

<Tabs.Root
  bind:value={
    () => value,
    (next) => {
      value = next as T;
      onchange?.(value);
    }
  }
>
  {@render children()}
</Tabs.Root>
