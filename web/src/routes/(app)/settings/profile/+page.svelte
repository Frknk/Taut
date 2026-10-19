<script lang="ts">
  import Banner from '#lib/components/Banner.svelte';
  import Button from '#lib/components/Button.svelte';
  import LayerCard from '#lib/components/LayerCard.svelte';
  import { authRequest, describeError } from '#lib/api.js';
  import {
    validatePasswordChange,
    type AuthErrors,
    type PasswordChangeValues,
  } from '#lib/auth-forms.js';
  import { session } from '#lib/session.svelte.js';
  import type { SessionResponse } from '#lib/types.js';
  import { fieldError, help, input, label } from '#lib/ui.js';

  let values = $state<PasswordChangeValues>({ current: '', password: '', confirm: '' });
  let errors = $state<AuthErrors<PasswordChangeValues>>({});
  let error = $state('');
  let notice = $state('');
  let busy = $state(false);

  async function changePassword(event: SubmitEvent) {
    event.preventDefault();
    errors = validatePasswordChange(values, session.user?.email);
    if (Object.keys(errors).length > 0) return;
    busy = true;
    error = '';
    notice = '';
    try {
      const response = await authRequest<SessionResponse>('PUT', '/v1/auth/password', {
        body: { current_password: values.current, new_password: values.password },
      });
      session.apply(response);
      values = { current: '', password: '', confirm: '' };
      notice = 'Contraseña cambiada. Cerramos tus sesiones en otros dispositivos.';
    } catch (err) {
      error = describeError(err);
    } finally {
      busy = false;
    }
  }
</script>

<svelte:head>
  <title>Perfil · Taut</title>
</svelte:head>

<LayerCard title="Cuenta">
  <dl class="grid gap-3 text-sm sm:grid-cols-2">
    <div>
      <dt class="text-subtle">Nombre</dt>
      <dd class="font-medium text-default">{session.user?.name}</dd>
    </div>
    <div>
      <dt class="text-subtle">Correo</dt>
      <dd class="font-medium text-default">{session.user?.email}</dd>
    </div>
  </dl>
</LayerCard>

<LayerCard title="Contraseña">
  {#if error}<Banner tone="danger" class="mb-4">{error}</Banner>{/if}
  {#if notice}<Banner tone="success" class="mb-4">{notice}</Banner>{/if}
  <form class="space-y-4" onsubmit={changePassword} novalidate>
    <div>
      <label class={label} for="current">Contraseña actual</label>
      <input
        id="current"
        class={input}
        type="password"
        autocomplete="current-password"
        bind:value={values.current}
        aria-invalid={errors.current ? true : undefined}
      />
      {#if errors.current}<p class={fieldError}>{errors.current}</p>{/if}
    </div>
    <div>
      <label class={label} for="password">Nueva contraseña</label>
      <input
        id="password"
        class={input}
        type="password"
        autocomplete="new-password"
        bind:value={values.password}
        aria-invalid={errors.password ? true : undefined}
      />
      {#if errors.password}
        <p class={fieldError}>{errors.password}</p>
      {:else}
        <p class={help}>Entre 8 y 128 caracteres.</p>
      {/if}
    </div>
    <div>
      <label class={label} for="confirm">Repite la nueva contraseña</label>
      <input
        id="confirm"
        class={input}
        type="password"
        autocomplete="new-password"
        bind:value={values.confirm}
        aria-invalid={errors.confirm ? true : undefined}
      />
      {#if errors.confirm}<p class={fieldError}>{errors.confirm}</p>{/if}
    </div>
    <Button type="submit" variant="primary" disabled={busy}>
      {busy ? 'Guardando…' : 'Cambiar contraseña'}
    </Button>
  </form>
</LayerCard>
