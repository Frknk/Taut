<script lang="ts">
  import { resolve } from '$app/paths';

  import Banner from '#lib/components/Banner.svelte';
  import Button from '#lib/components/Button.svelte';
  import Step from '#lib/components/Step.svelte';
  import Turnstile from '#lib/components/Turnstile.svelte';
  import { authRequest, describeError } from '#lib/api.js';
  import { looksLikeEmail } from '#lib/auth-forms.js';
  import { captchaBody, captchaPending } from '#lib/turnstile.js';
  import { fieldError, input, label, link } from '#lib/ui.js';

  let email = $state('');
  let fieldMessage = $state('');
  let error = $state('');
  let sent = $state(false);
  let busy = $state(false);
  let captcha = $state('');
  let widget = $state<Turnstile>();

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    fieldMessage = looksLikeEmail(email) ? '' : 'Escribe un correo válido.';
    if (fieldMessage) return;
    error = captchaPending(captcha);
    if (error) return;
    busy = true;
    try {
      await authRequest<void>('POST', '/v1/auth/forgot-password', {
        body: { email, ...captchaBody(captcha) },
      });
      sent = true;
    } catch (err) {
      error = describeError(err);
    } finally {
      busy = false;
      // Cloudflare accepts each token once: the next attempt needs a fresh one.
      widget?.reset();
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
  <Step>
    <Banner tone="success">
      Listo. Si <strong>{email.trim()}</strong> tiene cuenta, el correo está en camino.
    </Banner>
  </Step>
{:else}
  <Step>
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
      <Turnstile action="forgot_password" bind:token={captcha} bind:this={widget} />
      <Button type="submit" variant="primary" class="w-full" disabled={busy}>
        {busy ? 'Enviando…' : 'Enviar enlace'}
      </Button>
    </form>
  </Step>
{/if}

<p class="text-sm text-subtle">
  <a class={link} href={resolve('/(auth)/login')}>Volver a entrar</a>
</p>
