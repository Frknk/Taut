<script lang="ts" generics="T extends string">
  import { Tabs } from 'bits-ui';

  import { focusRing } from '#lib/ui.js';

  interface Props {
    options: { value: T; label: string }[];
    /** Names the list for screen readers: «Rango», «Filtro». */
    label: string;
  }

  /** The tabs of a `Tabs`, drawn as a segmented control. */
  let { options, label }: Props = $props();
</script>

<Tabs.List
  aria-label={label}
  class="inline-flex h-9 max-w-full items-stretch overflow-x-auto rounded-lg bg-recessed px-0.5"
>
  {#each options as option (option.value)}
    <Tabs.Trigger
      value={option.value}
      class={[
        'my-0.5 flex cursor-pointer items-center rounded-md px-2.5 text-sm whitespace-nowrap',
        'text-subtle hover:text-default',
        'data-[state=active]:bg-base data-[state=active]:text-default data-[state=active]:shadow-xs data-[state=active]:ring data-[state=active]:ring-line',
        focusRing,
      ]}
    >
      {option.label}
    </Tabs.Trigger>
  {/each}
</Tabs.List>
