<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';

  import Banner from '#lib/components/Banner.svelte';
  import Button from '#lib/components/Button.svelte';
  import Step from '#lib/components/Step.svelte';
  import Turnstile from '#lib/components/Turnstile.svelte';
  import { ApiError, authRequest, describeError } from '#lib/api.js';
  import { validateLogin, type AuthErrors, type LoginValues } from '#lib/auth-forms.js';
  import { codeError } from '#lib/mfa-form.js';
  import { session } from '#lib/session.svelte.js';
  import { captchaPending } from '#lib/turnstile.js';
  import { fieldError, help, input, label, link } from '#lib/ui.js';

  let values = $state<LoginValues>({ email: '', password: '' });
  let errors = $state<AuthErrors<LoginValues>>({});
  let error = $state('');
  let unverified = $state(false);
  let resent = $state(false);
  let busy = $state(false);
  let captcha = $state('');
  let widget = $state<Turnstile>();
  /** Token of the first step while the second factor is pending; `''` on the first step. */
  let mfaToken = $state('');
  let code = $state('');
  let mfaError = $state('');

  function finish() {
    const next = page.url.searchParams.get('next');
    void goto(next?.startsWith('/') ? next : resolve('/'));
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    errors = validateLogin(values);
    if (Object.keys(errors).length > 0) return;
    error = captchaPending(captcha);
    if (error) return;
    busy = true;
    unverified = false;
    try {
      const outcome = await session.login(values.email, values.password, captcha);
      if (outcome.status === 'mfa_required') {
        mfaToken = outcome.mfaToken;
        return;
      }
      finish();
    } catch (err) {
      if (err instanceof ApiError && err.code === 'email_unverified') unverified = true;
      else if (err instanceof ApiError && err.code === 'unauthorized')
        error = 'Correo o contraseña incorrectos.';
      else error = describeError(err);
    } finally {
      busy = false;
      // Cloudflare accepts each token once: the next attempt needs a fresh one.
      widget?.reset();
    }
  }

  async function verify(event: SubmitEvent) {
    event.preventDefault();
    mfaError = codeError(code) ?? '';
    if (mfaError) return;
    busy = true;
    try {
      await session.completeMfa(mfaToken, code);
      finish();
    } catch (err) {
      if (err instanceof ApiError && err.code === 'unauthorized') {
        // The token lives five minutes; after that the first step has to happen again.
        mfaError = err.message.includes('token')
          ? 'Pasó demasiado tiempo: vuelve a entrar con tu contraseña.'
          : 'Código incorrecto o ya usado.';
      } else {
        mfaError = describeError(err);
      }
    } finally {
      busy = false;
    }
  }

  function restart() {
    mfaToken = '';
    code = '';
    mfaError = '';
    values = { ...values, password: '' };
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
  <title>Entrar · Taut</title>
</svelte:head>

{#if mfaToken}
  <Step>
    <div>
      <h1 class="text-2xl font-semibold text-strong">Verificación en dos pasos</h1>
      <p class="mt-1 text-subtle">
        Escribe el código de tu app de autenticación para <strong class="font-medium text-default"
          >{values.email}</strong
        >.
      </p>
    </div>

    <form class="space-y-4" onsubmit={verify} novalidate>
      <div>
        <label class={label} for="code">Código de verificación</label>
        <input
          id="code"
          class={[input, 'font-mono']}
          inputmode="numeric"
          autocomplete="one-time-code"
          bind:value={code}
          aria-invalid={mfaError ? true : undefined}
          aria-describedby="code-help"
          required
        />
        {#if mfaError}
          <p class={fieldError}>{mfaError}</p>
        {:else}
          <p id="code-help" class={help}>
            ¿Sin el teléfono? Escribe uno de tus códigos de respaldo (abcd-efgh).
          </p>
        {/if}
      </div>
      <Button type="submit" variant="primary" class="w-full" disabled={busy}>
        {busy ? 'Comprobando…' : 'Verificar'}
      </Button>
    </form>

    <p class="text-sm text-subtle">
      <button type="button" class={link} onclick={restart}>Volver a empezar</button>
    </p>
  </Step>
{:else}
  <Step back>
    <div>
      <h1 class="text-2xl font-semibold text-strong">Entrar</h1>
      <p class="mt-1 text-subtle">Monitoreo de disponibilidad para tu equipo.</p>
    </div>

    {#if error}
      <Banner tone="danger">{error}</Banner>
    {/if}
    {#if unverified}
      <Banner tone="warning">
        Confirma tu correo antes de entrar: busca el mensaje de Taut en tu bandeja.
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
      <Turnstile action="login" bind:token={captcha} bind:this={widget} />
      <Button type="submit" variant="primary" class="w-full" disabled={busy}>
        {busy ? 'Entrando…' : 'Entrar'}
      </Button>
    </form>

    <p class="text-sm text-subtle">
      ¿No tienes cuenta? <a class={link} href={resolve('/(auth)/register')}>Regístrate</a>
    </p>
  </Step>
{/if}
