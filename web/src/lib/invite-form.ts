// Forms of the members and organization settings, in Spanish. The rules mirror services/api
// (invitations.rs, members.rs): the api stays the source of truth, these only save a round
// trip and keep the destructive buttons honest.

import { looksLikeEmail } from './auth-forms.js';
import type { Role } from './types.js';

export const TENANT_NAME_MAX = 100;

export interface InviteValues {
  email: string;
  role: Role;
}

/** Body of `POST /v1/invitations`. */
export interface InviteInput {
  email: string;
  role: Role;
}

export type InviteErrors = Partial<Record<keyof InviteValues, string>>;

export function emptyInvite(): InviteValues {
  return { email: '', role: 'member' };
}

/** `members` are the addresses already in the tenant: inviting them again is a 409. */
export function validateInvite(values: InviteValues, members: string[] = []): InviteErrors {
  const errors: InviteErrors = {};
  const email = values.email.trim().toLowerCase();
  if (!looksLikeEmail(email)) errors.email = 'Escribe un correo válido.';
  else if (members.some((member) => member.toLowerCase() === email)) {
    errors.email = 'Esa persona ya es miembro.';
  }
  if (values.role !== 'owner' && values.role !== 'member') errors.role = 'Elige un rol.';
  return errors;
}

export function toInviteInput(values: InviteValues): InviteInput {
  return { email: values.email.trim().toLowerCase(), role: values.role };
}

/** Error of a new organization name, or `undefined` when it is fine. */
export function tenantNameError(name: string): string | undefined {
  const trimmed = name.trim();
  if (!trimmed) return 'Escribe un nombre.';
  if ([...trimmed].length > TENANT_NAME_MAX) return `Máximo ${TENANT_NAME_MAX} caracteres.`;
  return undefined;
}

/** Deleting asks to type the slug: only an exact match (spaces aside) enables the button. */
export function confirmsDeletion(typed: string, slug: string): boolean {
  return typed.trim() === slug;
}

/** Whether an invitation's link stopped working. */
export function isExpired(expiresAt: string, now: Date = new Date()): boolean {
  return new Date(expiresAt).getTime() <= now.getTime();
}
