<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';

  import Banner from '#lib/components/Banner.svelte';
  import Button from '#lib/components/Button.svelte';
  import { ApiError, authRequest, describeError } from '#lib/api.js';
  import { validateLogin, type AuthErrors, type LoginValues } from '#lib/auth-forms.js';
  import { session } from '#lib/session.svelte.js';
  import { fieldError, input, label, link } from '#lib/ui.js';

  let values = $state<LoginValues>({ email: '', password: '' });
  let errors = $state<AuthErrors<LoginValues>>({});
  let error = $state('');
  let unverified = $state(false);
  let resent = $state(false);
  let busy = $state(false);

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    errors = validateLogin(values);
    if (Object.keys(errors).length > 0) return;
    busy = true;
    error = '';
    unverified = false;
    try {
      await session.login(values.email, values.password);
      const next = page.url.searchParams.get('next');
      void goto(next?.startsWith('/') ? next : resolve('/'));
    } catch (err) {
      if (err instanceof ApiError && err.code === 'email_unverified') unverified = true;
      else if (err instanceof ApiError && err.code === 'unauthorized')
        error = 'Correo o contraseña incorrectos.';
      else error = describeError(err);
    } finally {
      busy = false;
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
  <title>Entrar · taut</title>
</svelte:head>

<div>
  <h1 class="text-2xl font-semibold text-strong">Entrar</h1>
  <p class="mt-1 text-subtle">Monitoreo de disponibilidad para tu equipo.</p>
</div>

{#if error}
  <Banner tone="danger">{error}</Banner>
{/if}
{#if unverified}
  <Banner tone="warning">
    Confirma tu correo antes de entrar: busca el mensaje de taut en tu bandeja.
    {#if resent}
      Te enviamos otro enlace.
    {:else}
      <button type="button" class={link} onclick={resend}>Reenviar el correo</button>
    {/if}
  </Banner>
{/if}

<form class="space-y-4" onsubmit={submit} novalidate>
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
    <div class="flex items-baseline justify-between">
      <label class={label} for="password">Contraseña</label>
      <a class={[link, 'text-xs']} href={resolve('/(auth)/forgot-password')}>¿La olvidaste?</a>
    </div>
    <input
      id="password"
      class={input}
      type="password"
      autocomplete="current-password"
      bind:value={values.password}
      aria-invalid={errors.password ? true : undefined}
      required
    />
    {#if errors.password}<p class={fieldError}>{errors.password}</p>{/if}
  </div>
  <Button type="submit" variant="primary" class="w-full" disabled={busy}>
    {busy ? 'Entrando…' : 'Entrar'}
  </Button>
</form>

<p class="text-sm text-subtle">
  ¿No tienes cuenta? <a class={link} href={resolve('/(auth)/register')}>Regístrate</a>
</p>
