<script lang="ts">
  import { Tabs } from 'bits-ui';
  import type { Snippet } from 'svelte';
  import type { ClassValue } from 'svelte/elements';

  import { focusRing } from '#lib/ui.js';

  interface Props {
    /** The selected tab: one panel follows the selection, so its content is always current. */
    value: string;
    class?: ClassValue;
    children: Snippet;
  }

  /** The content a `TabList` controls, labelled by the selected tab. */
  let { value, class: className, children }: Props = $props();
</script>

<Tabs.Content {value} class={['rounded-lg', focusRing, className]}>
  {@render children()}
</Tabs.Content>
