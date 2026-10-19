<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';

  import Banner from '#lib/components/Banner.svelte';
  import Button from '#lib/components/Button.svelte';
  import Step from '#lib/components/Step.svelte';
  import { ApiError, describeError, request } from '#lib/api.js';
  import { formatDateTime, ROLE_LABELS } from '#lib/format.js';
  import { session } from '#lib/session.svelte.js';
  import { selectedTenant } from '#lib/tenant.svelte.js';
  import type { InvitationPreview, TenantWithRole } from '#lib/types.js';
  import { link } from '#lib/ui.js';

  // The token in the path is the secret of the emailed link: the api shows the invitation to
  // anyone holding it, and accepts it only for the invited account.
  const token = $derived(page.params.token ?? '');
  const here = $derived(page.url.pathname);
  const withNext = (path: string) => `${path}?next=${encodeURIComponent(here)}`;

  let invitation = $state<InvitationPreview | null>(null);
  let loadError = $state('');
  let error = $state('');
  let accepting = $state(false);
  let alreadyMember = $state(false);

  const signedInAs = $derived(session.user?.email ?? '');
  const isForMe = $derived(invitation !== null && signedInAs === invitation.email);

  onMount(async () => {
    try {
      invitation = await request<InvitationPreview>('GET', `/v1/invitations/${token}`);
    } catch (err) {
      if (err instanceof ApiError && err.status === 404) {
        loadError = 'Esta invitación no existe, ya se usó o fue revocada.';
      } else if (err instanceof ApiError && err.status === 400) {
        loadError = 'Esta invitación venció. Pide a quien te invitó que te envíe otra.';
      } else {
        loadError = describeError(err);
      }
    }
  });

  async function accept() {
    accepting = true;
    error = '';
    try {
      const tenant = await request<TenantWithRole>('POST', `/v1/invitations/${token}/accept`);
      selectedTenant.select(tenant);
      await goto(resolve('/(app)/monitors'));
    } catch (err) {
      if (err instanceof ApiError && err.status === 409) alreadyMember = true;
      else if (err instanceof ApiError && err.status === 403) {
        error = 'Esta invitación es para otra dirección de correo.';
      } else error = describeError(err);
    } finally {
      accepting = false;
    }
  }

  async function switchAccount() {
    await session.logout();
    await goto(withNext(resolve('/(auth)/login')));
  }
</script>

<svelte:head>
  <title>Invitación · Taut</title>
</svelte:head>

{#if loadError}
  <Step>
    <div>
      <h1 class="text-2xl font-semibold text-strong">Invitación no disponible</h1>
    </div>
    <Banner tone="warning">{loadError}</Banner>
    <p class="text-sm text-subtle">
      <a class={link} href={resolve('/')}>Ir a tus organizaciones</a>
    </p>
  </Step>
{:else if invitation}
  <Step>
    <div>
      <h1 class="text-2xl font-semibold text-strong">Te invitaron a {invitation.tenant_name}</h1>
      <p class="mt-2 text-subtle">
        Entrarás como <strong class="font-medium text-default"
          >{ROLE_LABELS[invitation.role].toLowerCase()}</strong
        >
        con la cuenta
        <strong class="font-medium text-default">{invitation.email}</strong>. La invitación vale
        hasta el {formatDateTime(invitation.expires_at)}.
      </p>
    </div>

    {#if error}<Banner tone="danger">{error}</Banner>{/if}

    {#if session.status === 'booting'}
      <p class="text-subtle">Cargando…</p>
    {:else if session.status === 'anonymous'}
      <div class="space-y-3">
        <Button variant="primary" class="w-full" href={withNext(resolve('/(auth)/login'))}>
          Entrar para aceptar
        </Button>
        <Button class="w-full" href={withNext(resolve('/(auth)/register'))}>Crear una cuenta</Button
        >
      </div>
      <p class="text-sm text-subtle">
        Si creas la cuenta, usa {invitation.email}, confirma el correo y vuelve a abrir este enlace.
      </p>
    {:else if alreadyMember}
      <Banner tone="info">Ya eres miembro de esta organización.</Banner>
      <Button variant="primary" class="w-full" href={resolve('/')}>Ir a tus organizaciones</Button>
    {:else if isForMe}
      <Button variant="primary" class="w-full" onclick={accept} disabled={accepting}>
        {accepting ? 'Aceptando…' : `Aceptar y entrar a ${invitation.tenant_name}`}
      </Button>
    {:else}
      <Banner tone="warning">
        Entraste como {signedInAs}, pero la invitación es para {invitation.email}.
      </Banner>
      <Button variant="primary" class="w-full" onclick={switchAccount}
        >Entrar con otra cuenta</Button
      >
    {/if}
  </Step>
{:else}
  <Step>
    <p class="text-subtle">Cargando la invitación…</p>
  </Step>
{/if}
