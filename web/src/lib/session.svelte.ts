// The signed-in user. The access token lives here, in memory only (docs/adr/0005); the refresh
// token is an HttpOnly cookie the browser sends to taut-auth on its own. On load the panel asks
// for a refresh once: with a valid cookie the session comes back, without one it is anonymous.

import { goto } from '$app/navigation';
import { resolve } from '$app/paths';

import { ApiError, authRequest, configureAuth } from './api.js';
import { expiresAtMs, refreshDelayMs } from './session-timing.js';
import { selectedTenant } from './tenant.svelte.js';
import type { SessionResponse, User } from './types.js';

export type SessionStatus = 'booting' | 'anonymous' | 'authenticated';

class Session {
  user = $state<User | null>(null);
  status = $state<SessionStatus>('booting');
  accessToken: string | null = null;
  private expiresAt = 0;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private refreshing: Promise<boolean> | null = null;

  /** Restores the session from the refresh cookie, once, when the app loads. */
  async boot(): Promise<void> {
    if (this.status !== 'booting') return;
    const ok = await this.refresh();
    this.status = ok ? 'authenticated' : 'anonymous';
  }

  /** Takes the tokens of a login, refresh or password change. */
  apply(response: SessionResponse): void {
    this.accessToken = response.access_token;
    this.expiresAt = expiresAtMs(response.expires_in);
    this.user = response.user;
    this.status = 'authenticated';
    this.arm();
  }

  /** Gets a new access token with the cookie. One call in flight at a time. */
  refresh(): Promise<boolean> {
    if (!this.refreshing) {
      this.refreshing = authRequest<SessionResponse>('POST', '/v1/auth/refresh')
        .then((response) => {
          this.apply(response);
          return true;
        })
        .catch((error: unknown) => {
          // Only a rejected session clears the state; a network blip keeps the token we have.
          if (error instanceof ApiError && error.status === 401) this.clear();
          return false;
        })
        .finally(() => {
          this.refreshing = null;
        });
    }
    return this.refreshing;
  }

  async login(email: string, password: string): Promise<void> {
    this.apply(
      await authRequest<SessionResponse>('POST', '/v1/auth/login', { body: { email, password } }),
    );
  }

  async logout(): Promise<void> {
    try {
      await authRequest<void>('POST', '/v1/auth/logout');
    } catch {
      // The cookie may be gone already; the local state goes anyway.
    }
    this.clear();
    selectedTenant.clear();
    void goto(resolve('/(auth)/login'));
  }

  /** Forgets the session locally. */
  clear(): void {
    this.accessToken = null;
    this.expiresAt = 0;
    this.user = null;
    this.status = 'anonymous';
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
  }

  private arm(): void {
    if (this.timer) clearTimeout(this.timer);
    this.timer = setTimeout(() => void this.refresh(), refreshDelayMs(this.expiresAt));
  }
}

export const session = new Session();

configureAuth({
  getToken: () => session.accessToken,
  refresh: () => session.refresh(),
  onSignedOut: () => {
    session.clear();
    selectedTenant.clear();
    void goto(resolve('/(auth)/login'));
  },
  onNotMember: () => {
    selectedTenant.clear();
    void goto(resolve('/'));
  },
});
