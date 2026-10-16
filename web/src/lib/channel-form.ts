// Form model of an alert channel: Spanish validation and conversion to the API body.
// The rules mirror `validate::channel` in services/api; the API stays the source of truth.

import type { AlertChannel, ChannelInput, ChannelKind } from './types.js';

export interface ChannelFormValues {
  name: string;
  type: ChannelKind;
  /** Email address or webhook URL, depending on the type. */
  destination: string;
  active: boolean;
}

export type ChannelErrors = Partial<Record<keyof ChannelFormValues, string>>;

export function emptyChannel(): ChannelFormValues {
  return { name: '', type: 'email', destination: '', active: true };
}

export function channelFromApi(channel: AlertChannel): ChannelFormValues {
  return {
    name: channel.name,
    type: channel.type,
    destination: channel.config.to ?? channel.config.url ?? '',
    active: channel.active,
  };
}

const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@.]+$/;

function isWebUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return url.protocol === 'http:' || url.protocol === 'https:';
  } catch {
    return false;
  }
}

/** Placeholder of the destination field for each type. */
export const DESTINATION_HINTS: Record<ChannelKind, string> = {
  email: 'guardia@ejemplo.com',
  discord: 'https://discord.com/api/webhooks/…',
  slack: 'https://hooks.slack.com/services/…',
  webhook: 'https://ejemplo.com/hooks/taut',
};

export function validateChannel(values: ChannelFormValues): ChannelErrors {
  const errors: ChannelErrors = {};
  const name = values.name.trim();
  if (name === '') errors.name = 'El nombre es obligatorio.';
  else if (name.length > 100) errors.name = 'Máximo 100 caracteres.';
  const destination = values.destination.trim();
  if (values.type === 'email') {
    if (!EMAIL.test(destination))
      errors.destination = 'Un correo válido, por ejemplo guardia@ejemplo.com.';
  } else if (!isWebUrl(destination)) {
    errors.destination = 'La URL completa del webhook (https://…).';
  }
  return errors;
}

export function toChannelInput(values: ChannelFormValues): ChannelInput {
  const destination = values.destination.trim();
  return {
    name: values.name.trim(),
    type: values.type,
    config: values.type === 'email' ? { to: destination } : { url: destination },
    active: values.active,
  };
}
