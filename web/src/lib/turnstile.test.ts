import { describe, expect, it } from 'vitest';

import { captchaBody, captchaPending } from './turnstile';

describe('captchaBody', () => {
  it('sends the token only when there is one', () => {
    expect(captchaBody('abc')).toEqual({ captcha_token: 'abc' });
    expect(captchaBody('')).toEqual({});
  });
});

describe('captchaPending', () => {
  it('holds the form until the widget produced a token', () => {
    expect(captchaPending('', 'site-key')).toContain('verificación');
    expect(captchaPending('abc', 'site-key')).toBe('');
  });

  it('never holds it with the captcha off', () => {
    expect(captchaPending('', '')).toBe('');
  });
});
