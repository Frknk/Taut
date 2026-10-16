import { describe, expect, it } from 'vitest';

import { emptyChannel, toChannelInput, validateChannel } from './channel-form';

describe('validateChannel', () => {
  it('requires a name and an email for email channels', () => {
    const errors = validateChannel({ ...emptyChannel(), destination: 'ops' });
    expect(errors.name).toBeDefined();
    expect(errors.destination).toMatch(/correo válido/);
    expect(
      validateChannel({ ...emptyChannel(), name: 'Guardia', destination: 'ops@acme.com' }),
    ).toEqual({});
  });

  it('requires a web URL for webhook channels', () => {
    const values = { ...emptyChannel(), name: 'Discord', type: 'discord' as const };
    expect(validateChannel({ ...values, destination: 'discord.com' }).destination).toBeDefined();
    expect(
      validateChannel({ ...values, destination: 'https://discord.com/api/webhooks/1/x' }),
    ).toEqual({});
  });
});

describe('toChannelInput', () => {
  it('puts the destination under the key of its type', () => {
    expect(
      toChannelInput({ ...emptyChannel(), name: ' Guardia ', destination: ' a@b.co ' }),
    ).toEqual({
      name: 'Guardia',
      type: 'email',
      config: { to: 'a@b.co' },
      active: true,
    });
    expect(
      toChannelInput({
        name: 'Hook',
        type: 'webhook',
        destination: 'https://x.test',
        active: false,
      }).config,
    ).toEqual({ url: 'https://x.test' });
  });
});
