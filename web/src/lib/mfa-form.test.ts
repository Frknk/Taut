import { describe, expect, it } from 'vitest';

import {
  backupCodesText,
  classifyCode,
  codeError,
  mfaBody,
  validateEnrollCode,
  validateReauth,
} from './mfa-form';

describe('classifyCode', () => {
  it('tells app codes from backup codes and ignores spacing and case', () => {
    expect(classifyCode('123456')).toEqual({ kind: 'totp', code: '123456' });
    expect(classifyCode(' 123 456 ')).toEqual({ kind: 'totp', code: '123456' });
    expect(classifyCode('abcd-efgh')).toEqual({ kind: 'backup', code: 'abcd-efgh' });
    expect(classifyCode('ABCD EFGH')).toEqual({ kind: 'backup', code: 'abcd-efgh' });
    expect(classifyCode('abcdefgh')).toEqual({ kind: 'backup', code: 'abcd-efgh' });
  });

  it('rejects anything else', () => {
    for (const bad of ['', '12345', '1234567', 'abcd-efg', 'abcd-ef0l', 'abcd-efgh-ijkl']) {
      expect(classifyCode(bad), bad).toBeNull();
    }
  });
});

describe('codeError and mfaBody', () => {
  it('explains an empty or malformed code', () => {
    expect(codeError('')).toBe('Escribe el código.');
    expect(codeError('12')).toContain('6 dígitos');
    expect(codeError('123456')).toBeUndefined();
    expect(codeError('abcd-efgh')).toBeUndefined();
  });

  it('picks the field of the request body', () => {
    expect(mfaBody('123 456')).toEqual({ code: '123456' });
    expect(mfaBody('ABCD EFGH')).toEqual({ backup_code: 'abcd-efgh' });
    expect(mfaBody('nope')).toEqual({ code: 'nope' });
  });
});

describe('validateReauth', () => {
  it('needs the password, and the code only with 2FA on', () => {
    expect(validateReauth({ password: 'secret', code: '' }, false)).toEqual({});
    expect(validateReauth({ password: '', code: '' }, false)).toEqual({
      password: 'Escribe tu contraseña.',
    });
    expect(validateReauth({ password: 'secret', code: '' }, true)).toEqual({
      code: 'Escribe el código.',
    });
    expect(validateReauth({ password: 'secret', code: '123456' }, true)).toEqual({});
  });
});

describe('validateEnrollCode', () => {
  it('only takes the six digits of the app', () => {
    expect(validateEnrollCode('')).toBe('Escribe el código de la app.');
    expect(validateEnrollCode('abcd-efgh')).toContain('6 dígitos');
    expect(validateEnrollCode('123456')).toBeUndefined();
  });
});

describe('backupCodesText', () => {
  it('lists one code per line', () => {
    expect(backupCodesText(['abcd-efgh', 'jkmn-pqrs'])).toBe('abcd-efgh\njkmn-pqrs\n');
  });
});
