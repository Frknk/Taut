<script lang="ts">
  // Cloudflare Turnstile widget for the public forms (registration, login, password recovery).
  // Cloudflare renders it in its own iframe and makes it accessible; it is the one interactive
  // piece of the panel not built on bits-ui (ADR 0009, ADR 0013). Without a sitekey it renders
  // nothing and `token` stays empty, which taut-auth accepts only with the captcha off.
  import { onMount } from 'svelte';

  import { SITEKEY, loadTurnstile, type TurnstileApi } from '#lib/turnstile.js';
  import { fieldError } from '#lib/ui.js';

  interface Props {
    /** Form the widget guards; Cloudflare groups its analytics by it. */
    action: 'login' | 'register' | 'forgot_password';
    /** Token of the solved challenge: `''` until then, after it expires and after `reset()`. */
    token?: string;
    /** Public key of the widget; `PUBLIC_TURNSTILE_SITEKEY` by default. */
    sitekey?: string;
  }

  let { action, token = $bindable(''), sitekey = SITEKEY }: Props = $props();

  let container = $state<HTMLDivElement>();
  let failed = $state(false);
  let api: TurnstileApi | null = null;
  let widgetId: string | undefined;

  onMount(() => {
    if (!sitekey || !container) return;
    const target = container;
    let gone = false;
    loadTurnstile()
      .then((turnstile) => {
        if (gone) return;
        api = turnstile;
        const forced = document.documentElement.dataset.theme;
        widgetId = turnstile.render(target, {
          sitekey,
          action,
          theme: forced === 'light' || forced === 'dark' ? forced : 'auto',
          size: 'flexible',
          language: 'es',
          callback: (value) => {
            token = value;
            failed = false;
          },
          'expired-callback': () => {
            token = '';
          },
          'error-callback': () => {
            token = '';
            failed = true;
          },
        });
      })
      .catch(() => {
        if (!gone) failed = true;
      });
    return () => {
      gone = true;
      if (api && widgetId) api.remove(widgetId);
    };
  });

  /** Asks for a fresh token: Cloudflare accepts each one only once. */
  export function reset(): void {
    token = '';
    if (api && widgetId) api.reset(widgetId);
  }
</script>

{#if sitekey}
  <div>
    <!-- Reserves the widget's height so the form does not jump when it appears. -->
    <div bind:this={container} class="min-h-16.25"></div>
    {#if failed}
      <p class={fieldError}>No se pudo completar la verificación. Recarga la página.</p>
    {/if}
  </div>
{/if}
