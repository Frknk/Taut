import { describe, expect, it } from 'vitest';

import {
  confirmsDeletion,
  emptyInvite,
  isExpired,
  tenantNameError,
  toInviteInput,
  validateInvite,
} from './invite-form';

describe('validateInvite', () => {
  it('needs a valid email', () => {
    expect(validateInvite(emptyInvite()).email).toMatch(/correo válido/);
    expect(validateInvite({ email: 'bea@', role: 'member' }).email).toBeDefined();
    expect(validateInvite({ email: ' bea@acme.com ', role: 'owner' })).toEqual({});
  });

  it('rejects addresses that are already members, ignoring case', () => {
    const errors = validateInvite({ email: 'Ana@Acme.com', role: 'member' }, ['ana@acme.com']);
    expect(errors.email).toMatch(/ya es miembro/);
  });

  it('only accepts the two roles', () => {
    const values = { email: 'bea@acme.com', role: 'admin' } as unknown as ReturnType<
      typeof emptyInvite
    >;
    expect(validateInvite(values).role).toBeDefined();
  });
});

describe('toInviteInput', () => {
  it('trims and lowercases the address', () => {
    expect(toInviteInput({ email: '  Bea@Acme.COM ', role: 'owner' })).toEqual({
      email: 'bea@acme.com',
      role: 'owner',
    });
  });
});

describe('tenantNameError', () => {
  it('needs 1 to 100 characters after trimming', () => {
    expect(tenantNameError('   ')).toBeDefined();
    expect(tenantNameError('x'.repeat(101))).toMatch(/100/);
    expect(tenantNameError(' Acme ')).toBeUndefined();
  });
});

describe('confirmsDeletion', () => {
  it('needs the exact slug', () => {
    expect(confirmsDeletion('acme', 'acme')).toBe(true);
    expect(confirmsDeletion(' acme ', 'acme')).toBe(true);
    expect(confirmsDeletion('Acme', 'acme')).toBe(false);
    expect(confirmsDeletion('', 'acme')).toBe(false);
  });
});

describe('isExpired', () => {
  it('compares with now', () => {
    const now = new Date('2026-10-16T12:00:00Z');
    expect(isExpired('2026-10-16T11:59:59Z', now)).toBe(true);
    expect(isExpired('2026-10-23T12:00:00Z', now)).toBe(false);
  });
});
