<script lang="ts">
  import Banner from '#lib/components/Banner.svelte';
  import Button from '#lib/components/Button.svelte';
  import Dialog from '#lib/components/Dialog.svelte';
  import { ApiError, describeError } from '#lib/api.js';
  import type { AuthErrors } from '#lib/auth-forms.js';
  import { validateReauth, type ReauthValues } from '#lib/mfa-form.js';
  import { reauthPrompt } from '#lib/reauth.svelte.js';
  import { session } from '#lib/session.svelte.js';
  import { fieldError, help, input, label } from '#lib/ui.js';

  /**
   * Asks for the password (and the second factor, when the account has one) when an action
   * needs a recent authentication. Mounted once in the app layout; `reauthPrompt` opens it.
   */
  const formId = $props.id();
  const mfaEnabled = $derived(session.user?.mfa_enabled ?? false);

  let values = $state<ReauthValues>({ password: '', code: '' });
  let errors = $state<AuthErrors<ReauthValues>>({});
  let error = $state('');
  let busy = $state(false);

  // Closing the dialog any other way (Escape, Cancel) is giving up.
  $effect(() => {
    if (!reauthPrompt.open) {
      values = { password: '', code: '' };
      errors = {};
      error = '';
    }
  });

  function cancel() {
    reauthPrompt.resolve(false);
  }

  async function confirm(event: SubmitEvent) {
    event.preventDefault();
    errors = validateReauth(values, mfaEnabled);
    if (Object.keys(errors).length > 0) return;
    busy = true;
    error = '';
    try {
      await session.reauth(values.password, mfaEnabled ? values.code : '');
      reauthPrompt.resolve(true);
    } catch (err) {
      if (err instanceof ApiError && err.code === 'unauthorized') {
        error = mfaEnabled ? 'Contraseña o código incorrectos.' : 'Contraseña incorrecta.';
      } else {
        error = describeError(err);
      }
    } finally {
      busy = false;
    }
  }
</script>

<Dialog
  bind:open={reauthPrompt.open}
  title="Confirma que eres tú"
  description="Esta acción es delicada: vuelve a escribir tu contraseña para continuar."
>
  <form id={formId} class="space-y-4" onsubmit={confirm} novalidate>
    {#if error}<Banner tone="danger">{error}</Banner>{/if}
    <div>
      <label class={label} for="{formId}-password">Contraseña</label>
      <input
        id="{formId}-password"
        class={input}
        type="password"
        autocomplete="current-password"
        bind:value={values.password}
        aria-invalid={errors.password ? true : undefined}
      />
      {#if errors.password}<p class={fieldError}>{errors.password}</p>{/if}
    </div>
    {#if mfaEnabled}
      <div>
        <label class={label} for="{formId}-code">Código de verificación</label>
        <input
          id="{formId}-code"
          class={[input, 'font-mono']}
          inputmode="numeric"
          autocomplete="one-time-code"
          bind:value={values.code}
          aria-invalid={errors.code ? true : undefined}
        />
        {#if errors.code}
          <p class={fieldError}>{errors.code}</p>
        {:else}
          <p class={help}>El de tu app de autenticación, o un código de respaldo.</p>
        {/if}
      </div>
    {/if}
  </form>
  {#snippet actions()}
    <Button variant="ghost" onclick={cancel} disabled={busy}>Cancelar</Button>
    <Button type="submit" form={formId} variant="primary" disabled={busy}>
      {busy ? 'Comprobando…' : 'Confirmar'}
    </Button>
  {/snippet}
</Dialog>
