/** Base URL of taut-api, taken from `PUBLIC_API_URL` at build time. */
export const API_URL: string = (
  (import.meta.env.PUBLIC_API_URL as string | undefined) ?? 'http://localhost:8080'
).replace(/\/$/, '');

/** Base URL of taut-status-bff (public status pages), from `PUBLIC_STATUS_URL`. */
export const STATUS_URL: string = (
  (import.meta.env.PUBLIC_STATUS_URL as string | undefined) ?? 'http://localhost:8085'
).replace(/\/$/, '');

/** Base URL of taut-auth (registration, login, sessions), from `PUBLIC_AUTH_URL`. */
export const AUTH_URL: string = (
  (import.meta.env.PUBLIC_AUTH_URL as string | undefined) ?? 'http://localhost:8086'
).replace(/\/$/, '');

/** Failure of an API call: HTTP status (0 when the API was unreachable) and error code. */
export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  /** Seconds to wait, from `Retry-After`, on `too_many_requests`. */
  readonly retryAfter: number | null;

  constructor(status: number, code: string, message: string, retryAfter: number | null = null) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
    this.retryAfter = retryAfter;
  }
}

type Method = 'GET' | 'POST' | 'PUT' | 'DELETE';

interface RequestOptions {
  /** Tenant id sent in `X-Tenant-Id`. */
  tenant?: string;
  /** JSON body. */
  body?: unknown;
}

/**
 * How the API client gets the session: the session store registers itself here, so this
 * module stays free of Svelte and of `$app` imports (and testable in node).
 */
export interface AuthHandlers {
  /** Current access token, if signed in. */
  getToken(): string | null;
  /** Tries to get a new access token with the refresh cookie; `false` when signed out. */
  refresh(): Promise<boolean>;
  /** The session is over (refresh failed after a 401). */
  onSignedOut(): void;
  /** The selected tenant no longer has the user as a member. */
  onNotMember(): void;
  /**
   * The action needs a recent authentication (`403 reauth_required`): asks the user for
   * their password (and code) and resolves `true` once they confirmed, `false` if they gave
   * up. The call is then retried once.
   */
  reauth(): Promise<boolean>;
}

let auth: AuthHandlers | null = null;

/** Registers the session handlers; called once by the session store. */
export function configureAuth(handlers: AuthHandlers | null): void {
  auth = handlers;
}

async function call(
  base: string,
  method: Method,
  path: string,
  { tenant, body }: RequestOptions,
  token: string | null,
  credentials: RequestCredentials,
): Promise<Response> {
  const headers: Record<string, string> = {};
  if (tenant) headers['x-tenant-id'] = tenant;
  if (token) headers['authorization'] = `Bearer ${token}`;
  if (body !== undefined) headers['content-type'] = 'application/json';

  const init: RequestInit = { method, headers, credentials };
  if (body !== undefined) init.body = JSON.stringify(body);

  try {
    return await fetch(`${base}${path}`, init);
  } catch {
    throw new ApiError(0, 'network', 'API unreachable');
  }
}

async function parse<T>(response: Response): Promise<T> {
  if (response.status === 204) return undefined as T;
  const json: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const error = (json as { error?: { code?: string; message?: string } } | null)?.error;
    const retryAfter = Number(response.headers.get('retry-after'));
    throw new ApiError(
      response.status,
      error?.code ?? 'unknown',
      error?.message ?? `HTTP ${response.status}`,
      Number.isFinite(retryAfter) && retryAfter > 0 ? retryAfter : null,
    );
  }
  return json as T;
}

/** Whether `error` is the step-up refusal and the user then confirmed their password. */
async function reauthenticated(error: unknown): Promise<boolean> {
  return (
    error instanceof ApiError &&
    error.code === 'reauth_required' &&
    auth !== null &&
    (await auth.reauth())
  );
}

/**
 * Calls taut-api as the signed-in user and returns the parsed JSON body (`undefined` for
 * `204 No Content`). A `401` triggers one refresh and one retry; if the refresh fails the
 * session handlers take over (back to the login page). A `403 reauth_required` asks the
 * user to confirm their password and retries once with the renewed token.
 */
export async function request<T>(
  method: Method,
  path: string,
  options: RequestOptions = {},
): Promise<T> {
  let response = await call(API_URL, method, path, options, auth?.getToken() ?? null, 'omit');
  if (response.status === 401 && auth && (await auth.refresh())) {
    response = await call(API_URL, method, path, options, auth.getToken(), 'omit');
  }
  if (response.status === 401) auth?.onSignedOut();
  try {
    return await parse<T>(response);
  } catch (error) {
    if (error instanceof ApiError && error.code === 'not_a_member') auth?.onNotMember();
    if (await reauthenticated(error)) {
      return parse<T>(await call(API_URL, method, path, options, auth?.getToken() ?? null, 'omit'));
    }
    throw error;
  }
}

/**
 * Calls taut-auth. Sends the refresh cookie (`credentials: 'include'`) and the access token
 * when there is one; never refreshes, since the refresh itself goes through here. Like
 * `request`, a `403 reauth_required` asks for the password and retries once.
 */
export async function authRequest<T>(
  method: Method,
  path: string,
  options: RequestOptions = {},
): Promise<T> {
  const response = await call(AUTH_URL, method, path, options, auth?.getToken() ?? null, 'include');
  try {
    return await parse<T>(response);
  } catch (error) {
    if (await reauthenticated(error)) {
      return parse<T>(
        await call(AUTH_URL, method, path, options, auth?.getToken() ?? null, 'include'),
      );
    }
    throw error;
  }
}

/** User-facing description of a failed call, in Spanish. API messages stay in English. */
export function describeError(error: unknown): string {
  if (!(error instanceof ApiError)) return 'Ocurrió un error inesperado.';
  switch (error.code) {
    case 'network':
      return 'No se pudo conectar con la API. ¿Está corriendo?';
    case 'unauthorized':
      return 'Tu sesión terminó; vuelve a entrar.';
    case 'not_a_member':
      return 'Ya no perteneces a esta organización.';
    case 'email_unverified':
      return 'Confirma tu correo antes de entrar.';
    case 'reauth_required':
      return 'Confirma tu contraseña para hacer esto.';
    case 'too_many_requests':
      return error.retryAfter
        ? `Demasiados intentos; espera ${describeWait(error.retryAfter)} y vuelve a probar.`
        : 'Demasiados intentos; espera unos minutos y vuelve a probar.';
    case 'not_found':
      return 'No existe o fue eliminado.';
    case 'forbidden':
      return `No tienes permiso para esto (${error.message}).`;
    case 'conflict':
      return `Ya existe (${error.message}).`;
    case 'bad_request':
      return `La API rechazó los datos: ${error.message}.`;
    default:
      return `Error de la API (${error.status}): ${error.message}.`;
  }
}

/** `45 s`, `3 min`, `15 min`. */
export function describeWait(seconds: number): string {
  if (seconds < 60) return `${seconds} s`;
  return `${Math.ceil(seconds / 60)} min`;
}
