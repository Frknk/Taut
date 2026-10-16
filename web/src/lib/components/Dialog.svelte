<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    /** Opens and closes the dialog; it stays in the DOM so its transitions keep working. */
    open: boolean;
    title: string;
    description?: string;
    /** `alertdialog` for irreversible confirmations. */
    alert?: boolean;
    /** The buttons: the safe one first, the main one last (on the right). */
    actions: Snippet;
  }

  let { open = $bindable(), title, description, alert = false, actions }: Props = $props();

  const id = $props.id();
  let dialog = $state<HTMLDialogElement>();

  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  });
</script>

<dialog
  bind:this={dialog}
  role={alert ? 'alertdialog' : 'dialog'}
  aria-labelledby="{id}-title"
  aria-describedby={description ? `${id}-text` : undefined}
  class="m-auto w-[min(100%-2rem,440px)] rounded-xl bg-base p-6 text-sm text-default shadow-lg ring ring-line backdrop:bg-scrim"
  onclose={() => (open = false)}
>
  <h2 id="{id}-title" class="text-xl font-semibold text-strong">{title}</h2>
  {#if description}<p id="{id}-text" class="mt-2 text-subtle">{description}</p>{/if}
  <div class="mt-6 flex flex-wrap justify-end gap-2">{@render actions()}</div>
</dialog>
