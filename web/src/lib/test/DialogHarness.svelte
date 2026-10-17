<script lang="ts">
  import Button from '#lib/components/Button.svelte';
  import Dialog from '#lib/components/Dialog.svelte';

  /** A trigger, the dialog under test with a field, and its two buttons. */
  let { alert = false, open = $bindable(false) }: { alert?: boolean; open?: boolean } = $props();
</script>

<Button onclick={() => (open = true)}>Abrir</Button>
<Dialog bind:open {alert} title="Borrar el monitor" description="No se puede deshacer.">
  <label for="harness-name">Nombre</label>
  <input id="harness-name" />
  {#snippet actions()}
    <Button onclick={() => (open = false)}>Cancelar</Button>
    <Button variant="destructive">Borrar</Button>
  {/snippet}
</Dialog>
