<script lang="ts">
  import { resolve } from '$app/paths';
  import { page } from '$app/state';

  import Banner from '#lib/components/Banner.svelte';
  import Button from '#lib/components/Button.svelte';
  import Step from '#lib/components/Step.svelte';
  import Turnstile from '#lib/components/Turnstile.svelte';
  import { authRequest, describeError } from '#lib/api.js';
  import { validateRegister, type AuthErrors, type RegisterValues } from '#lib/auth-forms.js';
  import { captchaBody, captchaPending } from '#lib/turnstile.js';
  import { fieldError, help, input, label, link } from '#lib/ui.js';

  // Coming from an invitation: the login links take the user back to it.
  const next = $derived(page.url.searchParams.get('next'));
  const loginHref = $derived(
    next?.startsWith('/')
      ? `${resolve('/(auth)/login')}?next=${encodeURIComponent(next)}`
      : resolve('/(auth)/login'),
  );

  let values = $state<RegisterValues>({ name: '', email: '', password: '', confirm: '' });
  let errors = $state<AuthErrors<RegisterValues>>({});
  let error = $state('');
  let sent = $state(false);
  let resent = $state(false);
  let busy = $state(false);
  let captcha = $state('');
  let widget = $state<Turnstile>();

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    errors = validateRegister(values);
    if (Object.keys(errors).length > 0) return;
    error = captchaPending(captcha);
    if (error) return;
    busy = true;
    try {
      await authRequest<unknown>('POST', '/v1/auth/register', {
        body: {
          name: values.name,
          email: values.email,
          password: values.password,
          ...captchaBody(captcha),
        },
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

  async function resend() {
    try {
      await authRequest<void>('POST', '/v1/auth/resend-verification', {
        body: { email: values.email },
      });
      resent = true;
    } catch (err) {
      error = describeError(err);
    }
  }
</script>

<svelte:head>
  <title>Crear cuenta · Taut</title>
</svelte:head>

{#if sent}
  <Step>
    <div>
      <h1 class="text-2xl font-semibold text-strong">Revisa tu correo</h1>
      <p class="mt-2 text-subtle">
        Enviamos un enlace a <strong class="text-default">{values.email.trim()}</strong> para confirmar
        tu dirección. Vale 24 horas.
      </p>
    </div>
    {#if error}<Banner tone="danger">{error}</Banner>{/if}
    <p class="text-sm text-subtle">
      ¿No llega?
      {#if resent}
        Te enviamos otro.
      {:else}
        <button type="button" class={link} onclick={resend}>Reenviar el correo</button>
      {/if}
      · <a class={link} href={loginHref}>Ir a entrar</a>
    </p>
  </Step>
{:else}
  <Step>
    <div>
      <h1 class="text-2xl font-semibold text-strong">Crear cuenta</h1>
      <p class="mt-1 text-subtle">
        Después podrás crear una organización o aceptar una invitación.
      </p>
    </div>

    {#if error}<Banner tone="danger">{error}</Banner>{/if}

    <form class="space-y-4" onsubmit={submit} novalidate>
      <div>
        <label class={label} for="name">Nombre</label>
        <input
          id="name"
          class={input}
          autocomplete="name"
          maxlength="100"
          bind:value={values.name}
          aria-invalid={errors.name ? true : undefined}
          required
        />
        {#if errors.name}<p class={fieldError}>{errors.name}</p>{/if}
      </div>
      <div>
        <label class={label} for="email">Correo</label>
        <input
          id="email"
          class={input}
          type="email"
          autocomplete="email"
          bind:value={values.email}
          aria-invalid={errors.email ? true : undefined}
          required
        />
        {#if errors.email}<p class={fieldError}>{errors.email}</p>{/if}
      </div>
      <div>
        <label class={label} for="password">Contraseña</label>
        <input
          id="password"
          class={input}
          type="password"
          autocomplete="new-password"
          bind:value={values.password}
          aria-invalid={errors.password ? true : undefined}
          required
        />
        {#if errors.password}
          <p class={fieldError}>{errors.password}</p>
        {:else}
          <p class={help}>Entre 8 y 128 caracteres; una frase larga vale más que símbolos.</p>
        {/if}
      </div>
      <div>
        <label class={label} for="confirm">Repite la contraseña</label>
        <input
          id="confirm"
          class={input}
          type="password"
          autocomplete="new-password"
          bind:value={values.confirm}
          aria-invalid={errors.confirm ? true : undefined}
          required
        />
        {#if errors.confirm}<p class={fieldError}>{errors.confirm}</p>{/if}
      </div>
      <Turnstile action="register" bind:token={captcha} bind:this={widget} />
      <Button type="submit" variant="primary" class="w-full" disabled={busy}>
        {busy ? 'Creando…' : 'Crear cuenta'}
      </Button>
    </form>

    <p class="text-sm text-subtle">
      ¿Ya tienes cuenta? <a class={link} href={loginHref}>Entra</a>
    </p>
  </Step>
{/if}
