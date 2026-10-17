<script lang="ts">
  import { onMount } from 'svelte';
  import { renderSVG } from 'uqr';

  import Badge from '#lib/components/Badge.svelte';
  import Banner from '#lib/components/Banner.svelte';
  import Button from '#lib/components/Button.svelte';
  import Dialog from '#lib/components/Dialog.svelte';
  import LayerCard from '#lib/components/LayerCard.svelte';
  import { ApiError, authRequest, describeError } from '#lib/api.js';
  import type { AuthErrors } from '#lib/auth-forms.js';
  import { formatDateTime } from '#lib/format.js';
  import {
    backupCodesText,
    validateEnrollCode,
    validateReauth,
    mfaBody,
    type ReauthValues,
  } from '#lib/mfa-form.js';
  import { session } from '#lib/session.svelte.js';
  import type { BackupCodes, MfaEnrollment, MfaStatus } from '#lib/types.js';
  import { fieldError, help, input, label } from '#lib/ui.js';

  let status = $state<MfaStatus | null>(null);
  let error = $state('');
  let notice = $state('');
  let busy = $state(false);

  // Enrollment in progress: the QR, the secret and the confirmation code.
  let enrollment = $state<MfaEnrollment | null>(null);
  let code = $state('');
  let codeError = $state('');

  // Backup codes just generated: shown once, then gone.
  let codes = $state<string[] | null>(null);
  let copied = $state(false);

  // Turning 2FA off: password and code in a dialog.
  let disabling = $state(false);
  let disableValues = $state<ReauthValues>({ password: '', code: '' });
  let disableErrors = $state<AuthErrors<ReauthValues>>({});
  let disableError = $state('');
  const disableFormId = $props.id();

  // The QR must read the same under any theme, so it keeps fixed black modules on white: a
  // camera needs the contrast, and an inverted QR confuses some authenticator apps.
  const qr = $derived(
    enrollment
      ? renderSVG(enrollment.otpauth_uri, {
          pixelSize: 4,
          border: 2,
          blackColor: '#000000',
          whiteColor: '#ffffff',
        })
      : '',
  );

  async function load() {
    try {
      status = await authRequest<MfaStatus>('GET', '/v1/auth/mfa');
      error = '';
    } catch (err) {
      error = describeError(err);
    }
  }

  async function enroll() {
    busy = true;
    error = '';
    notice = '';
    try {
      enrollment = await authRequest<MfaEnrollment>('POST', '/v1/auth/mfa/enroll');
      code = '';
      codeError = '';
    } catch (err) {
      error = describeError(err);
    } finally {
      busy = false;
    }
  }

  async function confirm(event: SubmitEvent) {
    event.preventDefault();
    codeError = validateEnrollCode(code) ?? '';
    if (codeError) return;
    busy = true;
    try {
      const result = await authRequest<BackupCodes>('POST', '/v1/auth/mfa/confirm', {
        body: mfaBody(code),
      });
      enrollment = null;
      codes = result.backup_codes;
      copied = false;
      notice = 'Verificación en dos pasos activada. Guarda estos códigos de respaldo.';
      await Promise.all([load(), session.refresh()]);
    } catch (err) {
      if (err instanceof ApiError && err.code === 'unauthorized') {
        codeError = 'Código incorrecto. Prueba con el siguiente que muestre la app.';
      } else if (err instanceof ApiError && err.code === 'bad_request') {
        // The pending secret expired (10 minutes): start over.
        enrollment = null;
        error = 'La activación caducó; vuelve a empezar.';
      } else {
        error = describeError(err);
      }
    } finally {
      busy = false;
    }
  }

  function cancelEnrollment() {
    enrollment = null;
    code = '';
    codeError = '';
  }

  async function regenerate() {
    busy = true;
    error = '';
    notice = '';
    try {
      const result = await authRequest<BackupCodes>('POST', '/v1/auth/mfa/backup-codes');
      codes = result.backup_codes;
      copied = false;
      notice = 'Códigos nuevos. Los anteriores ya no sirven.';
      await load();
    } catch (err) {
      error = describeError(err);
    } finally {
      busy = false;
    }
  }

  async function copyCodes() {
    if (!codes) return;
    try {
      await navigator.clipboard.writeText(backupCodesText(codes));
      copied = true;
    } catch {
      copied = false;
    }
  }

  function askDisable() {
    disableValues = { password: '', code: '' };
    disableErrors = {};
    disableError = '';
    disabling = true;
  }

  async function disable(event: SubmitEvent) {
    event.preventDefault();
    disableErrors = validateReauth(disableValues, true);
    if (Object.keys(disableErrors).length > 0) return;
    busy = true;
    disableError = '';
    try {
      await authRequest<void>('POST', '/v1/auth/mfa/disable', {
        body: { password: disableValues.password, ...mfaBody(disableValues.code) },
      });
      disabling = false;
      codes = null;
      notice = 'Verificación en dos pasos desactivada.';
      await Promise.all([load(), session.refresh()]);
    } catch (err) {
      disableError =
        err instanceof ApiError && err.code === 'unauthorized'
          ? 'Contraseña o código incorrectos.'
          : describeError(err);
    } finally {
      busy = false;
    }
  }

  onMount(load);
</script>

<svelte:head>
  <title>Seguridad · Taut</title>
</svelte:head>

{#if error}<Banner tone="danger">{error}</Banner>{/if}
{#if notice}<Banner tone="success">{notice}</Banner>{/if}

<LayerCard title="Verificación en dos pasos">
  {#snippet actions()}
    {#if status}
      <Badge tone={status.enabled ? 'success' : 'outline'}>
        {status.enabled ? 'Activa' : 'Inactiva'}
      </Badge>
    {/if}
  {/snippet}

  {#if !status}
    <p class="text-sm text-subtle" role="status">Cargando…</p>
  {:else if status.enabled}
    <div class="space-y-4 text-sm">
      <p class="text-default">
        Al entrar se pide, además de la contraseña, el código de tu app de autenticación.
        {#if status.enabled_at}
          <span class="text-subtle">Activa desde {formatDateTime(status.enabled_at)}.</span>
        {/if}
      </p>
      <p class="text-default">
        Códigos de respaldo sin usar:
        <strong class="font-medium">{status.backup_codes_left}</strong> de 8.
        {#if status.backup_codes_left <= 2}
          <span class="text-warning">Genera códigos nuevos antes de quedarte sin ellos.</span>
        {/if}
      </p>
      <div class="flex flex-wrap gap-2">
        <Button onclick={regenerate} disabled={busy}>Generar códigos nuevos</Button>
        <Button variant="secondary-destructive" onclick={askDisable} disabled={busy}>
          Desactivar
        </Button>
      </div>
    </div>
  {:else if enrollment}
    <div class="grid gap-6 sm:grid-cols-[auto_1fr]">
      <div
        class="size-44 shrink-0 overflow-hidden rounded-lg ring ring-hairline [&>svg]:size-full"
        aria-hidden="true"
      >
        {@html qr}
      </div>
      <form class="space-y-4 text-sm" onsubmit={confirm} novalidate>
        <ol class="list-decimal space-y-1 pl-5 text-default">
          <li>Abre tu app de autenticación (Google Authenticator, Aegis, 1Password…).</li>
          <li>Escanea el código QR o escribe la clave a mano.</li>
          <li>Escribe aquí el código de 6 dígitos que muestra la app.</li>
        </ol>
        <p class="text-subtle">
          Clave manual:
          <code class="rounded-sm bg-fill px-1 py-0.5 font-mono text-[0.9em] break-all text-default"
            >{enrollment.secret}</code
          >
        </p>
        <div class="max-w-xs">
          <label class={label} for="enroll-code">Código de la app</label>
          <input
            id="enroll-code"
            class={[input, 'font-mono']}
            inputmode="numeric"
            autocomplete="one-time-code"
            bind:value={code}
            aria-invalid={codeError ? true : undefined}
          />
          {#if codeError}<p class={fieldError}>{codeError}</p>{/if}
        </div>
        <div class="flex flex-wrap gap-2">
          <Button type="submit" variant="primary" disabled={busy}>
            {busy ? 'Comprobando…' : 'Activar'}
          </Button>
          <Button variant="ghost" onclick={cancelEnrollment} disabled={busy}>Cancelar</Button>
        </div>
      </form>
    </div>
  {:else}
    <div class="space-y-4 text-sm">
      <p class="text-default">
        Protege tu cuenta con un segundo paso al entrar: un código temporal de una app de
        autenticación en tu teléfono. Recibirás también códigos de respaldo por si lo pierdes.
      </p>
      <Button variant="primary" onclick={enroll} disabled={busy}>
        {busy ? 'Preparando…' : 'Activar la verificación en dos pasos'}
      </Button>
    </div>
  {/if}
</LayerCard>

{#if codes}
  <LayerCard title="Códigos de respaldo">
    {#snippet actions()}
      <Button size="sm" variant="ghost" onclick={copyCodes}>
        {copied ? 'Copiados' : 'Copiar'}
      </Button>
      <Button size="sm" variant="ghost" onclick={() => (codes = null)}>Ya los guardé</Button>
    {/snippet}
    <p class="text-sm text-default">
      Cada código sirve <strong class="font-medium">una sola vez</strong> para entrar sin el teléfono.
      Guárdalos en un lugar seguro: no volverán a mostrarse.
    </p>
    <ul class="mt-4 grid grid-cols-2 gap-x-6 gap-y-1 font-mono text-sm text-strong sm:grid-cols-4">
      {#each codes as backup (backup)}
        <li>{backup}</li>
      {/each}
    </ul>
  </LayerCard>
{/if}

<Dialog
  bind:open={disabling}
  alert
  title="¿Desactivar la verificación en dos pasos?"
  description="Tu cuenta quedará protegida solo por la contraseña. Confirma con tu contraseña y un código."
>
  <form id={disableFormId} class="space-y-4" onsubmit={disable} novalidate>
    {#if disableError}<Banner tone="danger">{disableError}</Banner>{/if}
    <div>
      <label class={label} for="{disableFormId}-password">Contraseña</label>
      <input
        id="{disableFormId}-password"
        class={input}
        type="password"
        autocomplete="current-password"
        bind:value={disableValues.password}
        aria-invalid={disableErrors.password ? true : undefined}
      />
      {#if disableErrors.password}<p class={fieldError}>{disableErrors.password}</p>{/if}
    </div>
    <div>
      <label class={label} for="{disableFormId}-code">Código de verificación</label>
      <input
        id="{disableFormId}-code"
        class={[input, 'font-mono']}
        inputmode="numeric"
        autocomplete="one-time-code"
        bind:value={disableValues.code}
        aria-invalid={disableErrors.code ? true : undefined}
      />
      {#if disableErrors.code}
        <p class={fieldError}>{disableErrors.code}</p>
      {:else}
        <p class={help}>El de la app, o un código de respaldo.</p>
      {/if}
    </div>
  </form>
  {#snippet actions()}
    <Button variant="ghost" onclick={() => (disabling = false)} disabled={busy}>Cancelar</Button>
    <Button type="submit" form={disableFormId} variant="destructive" disabled={busy}>
      {busy ? 'Desactivando…' : 'Desactivar'}
    </Button>
  {/snippet}
</Dialog>
