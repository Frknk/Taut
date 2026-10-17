<script lang="ts">
  import { AlertDialog, Dialog } from 'bits-ui';
  import XIcon from 'phosphor-svelte/lib/XIcon';
  import type { Snippet } from 'svelte';
  import { buttonClass, dialog } from '#lib/ui.js';

  interface Props {
    /** Opens and closes the dialog. The content mounts while it is open. */
    open: boolean;
    title: string;
    description?: string;
    /**
     * `alertdialog` for irreversible confirmations: a click outside does not close it and the
     * focus starts on the safe button.
     */
    alert?: boolean;
    /** Body between the description and the buttons: a short form, a list. */
    children?: Snippet;
    /** The buttons: the safe one first, the main one last (on the right). */
    actions: Snippet;
  }

  let {
    open = $bindable(),
    title,
    description,
    alert = false,
    children,
    actions,
  }: Props = $props();

  let actionBar = $state<HTMLDivElement>();

  // The safe button is the first of `actions`; bits-ui would focus the panel itself.
  function focusSafeAction(event: Event) {
    const safe = actionBar?.querySelector<HTMLElement>('button:not(:disabled), a[href]');
    if (!safe) return;
    event.preventDefault();
    safe.focus();
  }
</script>

{#snippet body()}
  <!-- Title and Description are the same parts in Dialog and AlertDialog. -->
  <Dialog.Title level={2} class="pr-8 text-xl font-semibold text-strong">{title}</Dialog.Title>
  {#if description}
    <Dialog.Description class="mt-2 text-subtle">{description}</Dialog.Description>
  {/if}
  {#if children}<div class="mt-4">{@render children()}</div>{/if}
  <div bind:this={actionBar} class="mt-6 flex flex-wrap justify-end gap-2">{@render actions()}</div>
{/snippet}

{#if alert}
  <AlertDialog.Root bind:open>
    <AlertDialog.Portal>
      <AlertDialog.Overlay class={dialog.overlay} />
      <AlertDialog.Content class={dialog.content} onOpenAutoFocus={focusSafeAction}>
        {@render body()}
      </AlertDialog.Content>
    </AlertDialog.Portal>
  </AlertDialog.Root>
{:else}
  <Dialog.Root bind:open>
    <Dialog.Portal>
      <Dialog.Overlay class={dialog.overlay} />
      <Dialog.Content class={dialog.content}>
        {@render body()}
        <!-- Last in the tab order: the focus starts on the first field, not on the X. -->
        <Dialog.Close
          class={[buttonClass('ghost', 'sm', true), 'absolute top-4 right-4']}
          aria-label="Cerrar"
        >
          <XIcon size={16} aria-hidden="true" />
        </Dialog.Close>
      </Dialog.Content>
    </Dialog.Portal>
  </Dialog.Root>
{/if}
