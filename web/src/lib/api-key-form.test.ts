import { describe, expect, it } from 'vitest';

import { curlExample, emptyApiKey, toApiKeyInput, validateApiKey } from './api-key-form';

describe('validateApiKey', () => {
  it('requires a name of at most 100 characters', () => {
    expect(validateApiKey(emptyApiKey()).name).toBeDefined();
    expect(validateApiKey({ name: '   ', scope: 'read' }).name).toBeDefined();
    expect(validateApiKey({ name: 'x'.repeat(101), scope: 'read' }).name).toMatch(/100/);
    // Counted in characters, like the API, not in UTF-16 units.
    expect(validateApiKey({ name: '🚀'.repeat(100), scope: 'write' })).toEqual({});
    expect(validateApiKey({ name: 'CI de GitHub', scope: 'write' })).toEqual({});
  });

  it('accepts only the read and write scopes', () => {
    const values = { name: 'CI', scope: 'admin' } as unknown as Parameters<
      typeof validateApiKey
    >[0];
    expect(validateApiKey(values).scope).toBeDefined();
  });
});

describe('toApiKeyInput', () => {
  it('trims the name and keeps the scope', () => {
    expect(toApiKeyInput({ name: '  Terraform ', scope: 'write' })).toEqual({
      name: 'Terraform',
      scope: 'write',
    });
  });
});

describe('curlExample', () => {
  it('sends the key as a bearer token', () => {
    expect(curlExample('http://localhost:8080', 'taut_live_abc')).toBe(
      'curl -H "Authorization: Bearer taut_live_abc" http://localhost:8080/v1/monitors',
    );
  });
});
