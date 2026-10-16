// Second-factor inputs, in Spanish: a six-digit code from the authenticator app or a backup
// code (`xxxx-xxxx`). The rules mirror services/auth (`mfa.rs`); the service stays the source
// of truth, these only save a round trip and pick the right field of the request body.

import type { AuthErrors } from './auth-forms.js';

/** Letters and digits of a backup code: lowercase, without `0/o` and `1/l/i`. */
const BACKUP_ALPHABET = /^[abcdefghjkmnpqrstuvwxyz23456789]{8}$/;

/** What the user typed, classified. */
export type MfaCode = { kind: 'totp'; code: string } | { kind: 'backup'; code: string };

/** The code without spaces (and dashes, for a backup code), or `null` when it is neither. */
export function classifyCode(value: string): MfaCode | null {
  const compact = value.replace(/\s+/g, '');
  if (/^\d{6}$/.test(compact)) return { kind: 'totp', code: compact };
  const backup = compact.replace(/-/g, '').toLowerCase();
  if (BACKUP_ALPHABET.test(backup)) {
    return { kind: 'backup', code: `${backup.slice(0, 4)}-${backup.slice(4)}` };
  }
  return null;
}

/** Error of a code field, or `undefined` when it looks right. */
export function codeError(value: string): string | undefined {
  if (!value.trim()) return 'Escribe el código.';
  if (!classifyCode(value)) return 'Son 6 dígitos, o un código de respaldo como abcd-efgh.';
  return undefined;
}

/** The body field the service expects for `value`. Call after `codeError` said it is fine. */
export function mfaBody(value: string): { code: string } | { backup_code: string } {
  const parsed = classifyCode(value);
  if (!parsed) return { code: value.trim() };
  return parsed.kind === 'totp' ? { code: parsed.code } : { backup_code: parsed.code };
}

/** Password plus code: what re-authenticating and disabling 2FA ask for. */
export interface ReauthValues {
  password: string;
  code: string;
}

/** With `mfaEnabled` the code is required; without it, ignored. */
export function validateReauth(
  values: ReauthValues,
  mfaEnabled: boolean,
): AuthErrors<ReauthValues> {
  const errors: AuthErrors<ReauthValues> = {};
  if (!values.password) errors.password = 'Escribe tu contraseña.';
  if (mfaEnabled) {
    const code = codeError(values.code);
    if (code) errors.code = code;
  }
  return errors;
}

/** The six-digit code the enrollment confirmation needs (backup codes do not exist yet). */
export function validateEnrollCode(value: string): string | undefined {
  if (!value.trim()) return 'Escribe el código de la app.';
  if (classifyCode(value)?.kind !== 'totp') return 'Son los 6 dígitos que muestra la app.';
  return undefined;
}

/** Backup codes as a text file: one per line, to save somewhere safe. */
export function backupCodesText(codes: string[]): string {
  return `${codes.join('\n')}\n`;
}
