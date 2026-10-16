import { describe, expect, it } from 'vitest';

import { emptyForm, toInput, validateForm, type MonitorFormValues } from './monitor-form';

function form(overrides: Partial<MonitorFormValues>): MonitorFormValues {
  return { ...emptyForm(), name: 'Sitio', target: 'https://ejemplo.com', ...overrides };
}

describe('validateForm', () => {
  it('accepts the defaults with a name and a URL', () => {
    expect(validateForm(form({}))).toEqual({});
  });

  it('requires a name', () => {
    expect(validateForm(form({ name: '  ' })).name).toBe('El nombre es obligatorio.');
  });

  it('checks the target by monitor type', () => {
    expect(validateForm(form({ target: 'ejemplo.com' })).target).toMatch(/URL completa/);
    expect(validateForm(form({ type: 'tcp', target: 'db.ejemplo.com' })).target).toMatch(
      /Host y puerto/,
    );
    expect(validateForm(form({ type: 'tcp', target: 'db.ejemplo.com:5432' }))).toEqual({});
    expect(validateForm(form({ type: 'tcp', target: '[::1]:22' }))).toEqual({});
    expect(validateForm(form({ type: 'heartbeat', target: '' }))).toEqual({});
  });

  it('keeps the timeout below the interval', () => {
    expect(validateForm(form({ interval_s: 30, timeout_s: 30 })).timeout_s).toBe(
      'Debe ser menor que la frecuencia.',
    );
    expect(validateForm(form({ interval_s: 45 })).interval_s).toBeDefined();
  });

  it('requires a keyword for keyword monitors', () => {
    expect(validateForm(form({ type: 'keyword' })).keyword).toBeDefined();
    expect(validateForm(form({ type: 'keyword', keyword: 'Bienvenido' }))).toEqual({});
  });

  it('validates the expected status code', () => {
    expect(validateForm(form({ expected_status: '700' })).expected_status).toBeDefined();
    expect(validateForm(form({ expected_status: '204' }))).toEqual({});
  });
});

describe('toInput', () => {
  it('sends only the fields of the monitor type', () => {
    const input = toInput(form({ type: 'heartbeat', target: 'ignored', keyword: 'ignored' }));
    expect(input.target).toBeUndefined();
    expect(input.keyword).toBeUndefined();
    expect(input.expected_status).toBeUndefined();
    expect(input.timeout_s).toBeLessThan(input.interval_s);
  });

  it('converts the expected status and trims text fields', () => {
    const input = toInput(
      form({ name: ' Sitio ', target: ' https://ejemplo.com ', expected_status: '204' }),
    );
    expect(input).toMatchObject({
      name: 'Sitio',
      target: 'https://ejemplo.com',
      expected_status: 204,
    });
  });
});
