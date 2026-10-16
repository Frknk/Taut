<script lang="ts">
  import { onMount } from 'svelte';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';

  import Banner from '#lib/components/Banner.svelte';
  import Button from '#lib/components/Button.svelte';
  import { authRequest, describeError } from '#lib/api.js';
  import { link } from '#lib/ui.js';

  let outcome = $state<'checking' | 'ok' | 'failed'>('checking');
  let error = $state('');

  onMount(async () => {
    const token = page.url.searchParams.get('token');
    if (!token) {
      outcome = 'failed';
      error = 'Al enlace le falta el código. Abre el correo y vuelve a pulsarlo.';
      return;
    }
    try {
      await authRequest<void>('POST', '/v1/auth/verify-email', { body: { token } });
      outcome = 'ok';
    } catch (err) {
      outcome = 'failed';
      error = describeError(err);
    }
  });
</script>

<svelte:head>
  <title>Confirmar correo · taut</title>
</svelte:head>

{#if outcome === 'checking'}
  <p class="text-subtle" role="status">Confirmando tu correo…</p>
{:else if outcome === 'ok'}
  <div>
    <h1 class="text-2xl font-semibold text-strong">Correo confirmado</h1>
    <p class="mt-2 text-subtle">Tu cuenta está lista. Entra para crear tu primera organización.</p>
  </div>
  <Button variant="primary" class="w-full" href={resolve('/(auth)/login')}>Entrar</Button>
{:else}
  <div>
    <h1 class="text-2xl font-semibold text-strong">No pudimos confirmar el correo</h1>
    <p class="mt-2 text-subtle">
      El enlace es inválido, venció o ya se usó. Desde la pantalla de entrar puedes pedir otro.
    </p>
  </div>
  <Banner tone="danger">{error}</Banner>
  <p class="text-sm text-subtle">
    <a class={link} href={resolve('/(auth)/login')}>Ir a entrar</a>
  </p>
{/if}
