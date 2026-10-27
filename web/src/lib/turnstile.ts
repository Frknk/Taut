// Cloudflare Turnstile, the captcha of registration, login and password recovery. The panel
// is a SPA, so the script loads once with explicit rendering and each form renders its own
// widget (components/Turnstile.svelte). The token goes to taut-auth as `captcha_token`.

/** Public key of the widget, from `PUBLIC_TURNSTILE_SITEKEY` at build time; empty turns it off. */
export const SITEKEY: string = (
  (import.meta.env.PUBLIC_TURNSTILE_SITEKEY as string | undefined) ?? ''
).trim();

/** Script URL; Cloudflare asks to load it from exactly here, never a copy. */
export const SCRIPT_URL = 'https://challenges.cloudflare.com/turnstile/v0/api.js?render=explicit';

/** Options of `turnstile.render` that Taut uses. */
export interface RenderOptions {
  sitekey: string;
  action?: string;
  theme?: 'auto' | 'light' | 'dark';
  size?: 'normal' | 'flexible' | 'compact';
  language?: string;
  callback?: (token: string) => void;
  'expired-callback'?: () => void;
  'error-callback'?: () => void;
}

/** The part of `window.turnstile` that Taut uses. */
export interface TurnstileApi {
  render(container: HTMLElement, options: RenderOptions): string | undefined;
  reset(widgetId: string): void;
  remove(widgetId: string): void;
}

declare global {
  interface Window {
    turnstile?: TurnstileApi;
  }
}

/** Body field that carries `token` to taut-auth; nothing when there is no token. */
export function captchaBody(token: string): { captcha_token?: string } {
  return token ? { captcha_token: token } : {};
}

/** Message of a form sent before the widget produced its token, or `''` when it may go. */
export function captchaPending(token: string, sitekey = SITEKEY): string {
  return sitekey && !token ? 'Espera a que termine la verificación.' : '';
}

let loading: Promise<TurnstileApi> | null = null;

/** The Turnstile API, adding its script to the page the first time. */
export function loadTurnstile(): Promise<TurnstileApi> {
  if (window.turnstile) return Promise.resolve(window.turnstile);
  loading ??= new Promise<TurnstileApi>((resolve, reject) => {
    const script = document.createElement('script');
    script.src = SCRIPT_URL;
    script.async = true;
    script.addEventListener('load', () => {
      if (window.turnstile) resolve(window.turnstile);
      else reject(new Error('turnstile script loaded without its API'));
    });
    script.addEventListener('error', () => reject(new Error('turnstile script failed to load')));
    document.head.append(script);
  }).catch((error: unknown) => {
    // Let a later form try again (the network may be back).
    loading = null;
    throw error;
  });
  return loading;
}
