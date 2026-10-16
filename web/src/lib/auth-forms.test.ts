import { describe, expect, it } from 'vitest';

import {
  looksLikeEmail,
  validateLogin,
  validateNewPassword,
  validatePasswordChange,
  validateRegister,
} from './auth-forms';

describe('looksLikeEmail', () => {
  it('accepts plain addresses and rejects the rest', () => {
    expect(looksLikeEmail(' Ana@Example.com ')).toBe(true);
    for (const bad of ['', 'ana', 'ana@', '@x.co', 'ana@x', 'a na@x.co', 'a@b@c.co']) {
      expect(looksLikeEmail(bad), bad).toBe(false);
    }
  });
});

describe('validateLogin', () => {
  it('needs an email and a password', () => {
    expect(validateLogin({ email: 'ana@x.co', password: 'secret' })).toEqual({});
    expect(validateLogin({ email: 'nope', password: '' })).toEqual({
      email: 'Escribe un correo válido.',
      password: 'Escribe tu contraseña.',
    });
  });
});

describe('validateRegister', () => {
  const ok = {
    name: 'Ana',
    email: 'ana@x.co',
    password: 'correct horse',
    confirm: 'correct horse',
  };

  it('passes a complete form', () => {
    expect(validateRegister(ok)).toEqual({});
  });

  it('checks every field', () => {
    expect(validateRegister({ ...ok, name: '  ' }).name).toBe('Escribe tu nombre.');
    expect(validateRegister({ ...ok, name: 'x'.repeat(101) }).name).toContain('100');
    expect(validateRegister({ ...ok, password: 'short', confirm: 'short' }).password).toContain(
      '8',
    );
    expect(
      validateRegister({ ...ok, password: 'x'.repeat(129), confirm: 'x'.repeat(129) }).password,
    ).toContain('128');
    expect(validateRegister({ ...ok, password: 'Ana@x.co', confirm: 'Ana@x.co' }).password).toBe(
      'No puede ser tu correo.',
    );
    expect(validateRegister({ ...ok, confirm: 'other' }).confirm).toBe(
      'Las contraseñas no coinciden.',
    );
  });
});

describe('validateNewPassword and validatePasswordChange', () => {
  it('reuses the password rules', () => {
    expect(validateNewPassword({ password: 'correct horse', confirm: 'correct horse' })).toEqual(
      {},
    );
    expect(validateNewPassword({ password: 'short', confirm: 'short' }).password).toContain('8');
    expect(
      validatePasswordChange({ current: '', password: 'correct horse', confirm: 'correct horse' })
        .current,
    ).toBe('Escribe tu contraseña actual.');
    expect(
      validatePasswordChange({ current: 'same pass', password: 'same pass', confirm: 'same pass' })
        .password,
    ).toBe('La nueva contraseña es igual a la actual.');
  });
});
