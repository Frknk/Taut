<script lang="ts">
  // Shows the token that Turnstile binds and drives `reset()` and unmounting from buttons.
  import Turnstile from '#lib/components/Turnstile.svelte';

  let { sitekey = 'test-sitekey' }: { sitekey?: string } = $props();

  let token = $state('');
  let widget = $state<Turnstile>();
  let shown = $state(true);
</script>

{#if shown}
  <Turnstile action="login" {sitekey} bind:token bind:this={widget} />
{/if}
<p>Token: {token || 'ninguno'}</p>
<button type="button" onclick={() => widget?.reset()}>Reiniciar</button>
<button type="button" onclick={() => (shown = false)}>Quitar</button>
