import { describe, expect, it } from 'vitest';

import { ApiError, describeError, describeWait } from './api';

describe('describeError', () => {
  it('explains the session and membership codes', () => {
    expect(describeError(new ApiError(401, 'unauthorized', 'x'))).toContain('sesión');
    expect(describeError(new ApiError(403, 'not_a_member', 'x'))).toContain('organización');
    expect(describeError(new ApiError(403, 'email_unverified', 'x'))).toContain('Confirma');
    expect(describeError(new ApiError(403, 'reauth_required', 'x'))).toContain('contraseña');
    expect(describeError(new ApiError(403, 'forbidden', 'owner role required'))).toContain(
      'owner role required',
    );
  });

  it('tells how long to wait after too many attempts', () => {
    expect(describeError(new ApiError(429, 'too_many_requests', 'x', 840))).toContain('14 min');
    expect(describeError(new ApiError(429, 'too_many_requests', 'x'))).toContain('unos minutos');
  });

  it('falls back for unknown failures', () => {
    expect(describeError(new Error('boom'))).toBe('Ocurrió un error inesperado.');
    expect(describeError(new ApiError(0, 'network', 'x'))).toContain('conectar');
  });
});

describe('describeWait', () => {
  it('uses seconds under a minute and rounds minutes up', () => {
    expect(describeWait(45)).toBe('45 s');
    expect(describeWait(60)).toBe('1 min');
    expect(describeWait(61)).toBe('2 min');
  });
});
