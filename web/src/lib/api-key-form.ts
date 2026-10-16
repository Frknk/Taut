// Form model of a tenant API key: Spanish validation, the API body and the labels of each
// scope. The rules mirror `api_keys.rs` in services/api; the API stays the source of truth.

import type { ApiKeyScope } from './types.js';

export interface ApiKeyFormValues {
  name: string;
  scope: ApiKeyScope;
}

export type ApiKeyErrors = Partial<Record<keyof ApiKeyFormValues, string>>;

/** Same limit as the API: 1 to 100 characters, counted as the API counts them. */
const MAX_NAME_CHARS = 100;

export const SCOPE_LABELS: Record<ApiKeyScope, string> = {
  read: 'Lectura',
  write: 'Escritura',
};

export const SCOPE_HINTS: Record<ApiKeyScope, string> = {
  read: 'Solo consulta monitores, chequeos, métricas e incidentes.',
  write: 'Además crea, edita, pausa y borra monitores.',
};

export function emptyApiKey(): ApiKeyFormValues {
  return { name: '', scope: 'read' };
}

export function validateApiKey(values: ApiKeyFormValues): ApiKeyErrors {
  const errors: ApiKeyErrors = {};
  const name = values.name.trim();
  if (name === '') errors.name = 'Ponle un nombre que diga dónde se usa.';
  else if ([...name].length > MAX_NAME_CHARS) errors.name = 'Máximo 100 caracteres.';
  if (!(values.scope in SCOPE_LABELS)) errors.scope = 'Elige lectura o escritura.';
  return errors;
}

export function toApiKeyInput(values: ApiKeyFormValues): ApiKeyFormValues {
  return { name: values.name.trim(), scope: values.scope };
}

/** A first request to try the key with: list the monitors of its tenant. */
export function curlExample(apiUrl: string, key: string): string {
  return `curl -H "Authorization: Bearer ${key}" ${apiUrl}/v1/monitors`;
}
