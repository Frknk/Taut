<script lang="ts">
  import { Tooltip } from 'bits-ui';
  import type { Snippet } from 'svelte';
  import type { ClassValue } from 'svelte/elements';

  import { focusRing } from '#lib/ui.js';

  interface Props {
    /** The extra detail: a full date, an error message. */
    content: string;
    class?: ClassValue;
    /** The short text it explains; underlined with dots as a hint there is more. */
    children: Snippet;
  }

  /**
   * A tooltip on bits-ui, in place of `title=`: it also opens on keyboard focus (the text takes a
   * tab stop) and is the trigger's description while it shows. Never the only place for
   * information a task needs.
   */
  let { content, class: className, children }: Props = $props();
</script>

<Tooltip.Root>
  <Tooltip.Trigger>
    {#snippet child({ props })}
      <!-- svelte-ignore a11y_no_noninteractive_tabindex: the tooltip must open from the keyboard too. -->
      <span
        {...props}
        tabindex="0"
        class={[
          'rounded-sm underline decoration-dotted decoration-1 underline-offset-2',
          focusRing,
          className,
        ]}
      >
        {@render children()}
      </span>
    {/snippet}
  </Tooltip.Trigger>
  <Tooltip.Portal>
    <!-- bits-ui does not set the role itself. -->
    <Tooltip.Content
      role="tooltip"
      sideOffset={6}
      class="z-50 max-w-xs rounded-md bg-contrast px-2 py-1 text-xs text-inverse shadow-md"
    >
      {content}
    </Tooltip.Content>
  </Tooltip.Portal>
</Tooltip.Root>
