<script lang="ts">
  import { resolve } from '$app/paths';

  import Banner from '#lib/components/Banner.svelte';
  import Button from '#lib/components/Button.svelte';
  import { authRequest, describeError } from '#lib/api.js';
  import { looksLikeEmail } from '#lib/auth-forms.js';
  import { fieldError, input, label, link } from '#lib/ui.js';

  let email = $state('');
  let fieldMessage = $state('');
  let error = $state('');
  let sent = $state(false);
  let busy = $state(false);

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    fieldMessage = looksLikeEmail(email) ? '' : 'Escribe un correo válido.';
    if (fieldMessage) return;
    busy = true;
    error = '';
    try {
      await authRequest<void>('POST', '/v1/auth/forgot-password', { body: { email } });
      sent = true;
    } catch (err) {
      error = describeError(err);
    } finally {
      busy = false;
    }
  }
</script>

<svelte:head>
  <title>Recuperar contraseña · Taut</title>
</svelte:head>

<div>
  <h1 class="text-2xl font-semibold text-strong">Recuperar contraseña</h1>
  <p class="mt-1 text-subtle">
    Si la dirección tiene cuenta, te enviamos un enlace para elegir otra. Vale una hora.
  </p>
</div>

{#if error}<Banner tone="danger">{error}</Banner>{/if}

{#if sent}
  <Banner tone="success">
    Listo. Si <strong>{email.trim()}</strong> tiene cuenta, el correo está en camino.
  </Banner>
{:else}
  <form class="space-y-4" onsubmit={submit} novalidate>
    <div>
      <label class={label} for="email">Correo</label>
      <input
        id="email"
        class={input}
        type="email"
        autocomplete="email"
        bind:value={email}
        aria-invalid={fieldMessage ? true : undefined}
        required
      />
      {#if fieldMessage}<p class={fieldError}>{fieldMessage}</p>{/if}
    </div>
    <Button type="submit" variant="primary" class="w-full" disabled={busy}>
      {busy ? 'Enviando…' : 'Enviar enlace'}
    </Button>
  </form>
{/if}

<p class="text-sm text-subtle">
  <a class={link} href={resolve('/(auth)/login')}>Volver a entrar</a>
</p>
