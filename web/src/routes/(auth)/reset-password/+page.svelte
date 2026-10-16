<script lang="ts">
  import { resolve } from '$app/paths';
  import { page } from '$app/state';

  import Banner from '#lib/components/Banner.svelte';
  import Button from '#lib/components/Button.svelte';
  import { authRequest, describeError } from '#lib/api.js';
  import { validateNewPassword, type AuthErrors, type NewPasswordValues } from '#lib/auth-forms.js';
  import { fieldError, help, input, label, link } from '#lib/ui.js';

  const token = $derived(page.url.searchParams.get('token') ?? '');

  let values = $state<NewPasswordValues>({ password: '', confirm: '' });
  let errors = $state<AuthErrors<NewPasswordValues>>({});
  let error = $state('');
  let done = $state(false);
  let busy = $state(false);

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    errors = validateNewPassword(values);
    if (Object.keys(errors).length > 0) return;
    busy = true;
    error = '';
    try {
      await authRequest<void>('POST', '/v1/auth/reset-password', {
        body: { token, password: values.password },
      });
      done = true;
    } catch (err) {
      error = describeError(err);
    } finally {
      busy = false;
    }
  }
</script>

<svelte:head>
  <title>Nueva contraseña · taut</title>
</svelte:head>

{#if done}
  <div>
    <h1 class="text-2xl font-semibold text-strong">Contraseña cambiada</h1>
    <p class="mt-2 text-subtle">Cerramos todas tus sesiones anteriores. Entra con la nueva.</p>
  </div>
  <Button variant="primary" class="w-full" href={resolve('/(auth)/login')}>Entrar</Button>
{:else if !token}
  <div>
    <h1 class="text-2xl font-semibold text-strong">Enlace incompleto</h1>
    <p class="mt-2 text-subtle">
      Al enlace le falta el código. Abre el correo y vuelve a pulsarlo.
    </p>
  </div>
  <p class="text-sm text-subtle">
    <a class={link} href={resolve('/(auth)/forgot-password')}>Pedir otro enlace</a>
  </p>
{:else}
  <div>
    <h1 class="text-2xl font-semibold text-strong">Nueva contraseña</h1>
    <p class="mt-1 text-subtle">Elige una contraseña que no uses en otro sitio.</p>
  </div>

  {#if error}<Banner tone="danger">{error}</Banner>{/if}

  <form class="space-y-4" onsubmit={submit} novalidate>
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
        <p class={help}>Entre 8 y 128 caracteres.</p>
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
    <Button type="submit" variant="primary" class="w-full" disabled={busy}>
      {busy ? 'Guardando…' : 'Guardar contraseña'}
    </Button>
  </form>
{/if}
