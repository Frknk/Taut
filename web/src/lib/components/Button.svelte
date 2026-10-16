<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { ClassValue, HTMLAnchorAttributes, HTMLButtonAttributes } from 'svelte/elements';

  import { buttonClass, type ButtonSize, type ButtonVariant } from '#lib/ui.js';

  interface Common {
    variant?: ButtonVariant;
    size?: ButtonSize;
    /** Icon-only button: width equals height. Needs an `aria-label`. */
    square?: boolean;
    class?: ClassValue;
    children: Snippet;
  }

  type Props = Common &
    (
      | (Omit<HTMLButtonAttributes, 'class' | 'children'> & { href?: undefined })
      | (Omit<HTMLAnchorAttributes, 'class' | 'children'> & { href: string })
    );

  let {
    variant = 'secondary',
    size = 'base',
    square = false,
    class: className,
    children,
    ...rest
  }: Props = $props();

  const classes = $derived([buttonClass(variant, size, square), className]);
</script>

{#if rest.href !== undefined}
  <a class={classes} {...rest as HTMLAnchorAttributes}>{@render children()}</a>
{:else}
  <button type="button" class={classes} {...rest as HTMLButtonAttributes}>
    {@render children()}
  </button>
{/if}
