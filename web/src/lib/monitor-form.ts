// Form model of a monitor: defaults, Spanish validation and conversion to the API body.
// The rules mirror services/api/src/validate.rs; the API stays the source of truth.

import type { Monitor, MonitorInput, MonitorKind } from './types.js';

export const INTERVALS = [30, 60, 180, 300] as const;
export const MAX_TIMEOUT_S = 60;
export const MAX_PERIOD_S = 3600;

export interface MonitorFormValues {
  name: string;
  type: MonitorKind;
  target: string;
  interval_s: number;
  timeout_s: number;
  /** Empty string means any 2xx. */
  expected_status: string;
  keyword: string;
  confirmation_period_s: number;
  recovery_period_s: number;
  grace_period_s: number;
}

export type FormErrors = Partial<Record<keyof MonitorFormValues, string>>;

export function emptyForm(): MonitorFormValues {
  return {
    name: '',
    type: 'http',
    target: '',
    interval_s: 60,
    timeout_s: 10,
    expected_status: '',
    keyword: '',
    confirmation_period_s: 60,
    recovery_period_s: 120,
    grace_period_s: 60,
  };
}

export function formFromMonitor(monitor: Monitor): MonitorFormValues {
  return {
    name: monitor.name,
    type: monitor.type,
    target: monitor.target ?? '',
    interval_s: monitor.interval_s,
    timeout_s: monitor.timeout_s,
    expected_status: monitor.expected_status === null ? '' : String(monitor.expected_status),
    keyword: monitor.keyword ?? '',
    confirmation_period_s: monitor.confirmation_period_s,
    recovery_period_s: monitor.recovery_period_s,
    grace_period_s: monitor.grace_period_s,
  };
}

const isHttp = (type: MonitorKind) => type === 'http' || type === 'keyword';

function isWebUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return (url.protocol === 'http:' || url.protocol === 'https:') && url.hostname !== '';
  } catch {
    return false;
  }
}

function isHostPort(value: string): boolean {
  const match = /^(\[[0-9a-fA-F:.]+\]|[a-zA-Z0-9.-]+):(\d{1,5})$/.exec(value);
  if (!match) return false;
  const port = Number(match[2]);
  return port >= 1 && port <= 65_535;
}

const inRange = (value: number, min: number, max: number) =>
  Number.isInteger(value) && value >= min && value <= max;

/** Field errors in Spanish; an empty object means the form can be sent. */
export function validateForm(values: MonitorFormValues): FormErrors {
  const errors: FormErrors = {};
  const name = values.name.trim();
  if (name === '') errors.name = 'El nombre es obligatorio.';
  else if (name.length > 100) errors.name = 'Máximo 100 caracteres.';

  const target = values.target.trim();
  if (isHttp(values.type)) {
    if (!isWebUrl(target)) errors.target = 'Una URL completa, por ejemplo https://ejemplo.com.';
  } else if (values.type === 'tcp') {
    if (!isHostPort(target)) errors.target = 'Host y puerto, por ejemplo db.ejemplo.com:5432.';
  }

  if (!(INTERVALS as readonly number[]).includes(values.interval_s)) {
    errors.interval_s = 'Elige 30 s, 1, 3 o 5 min.';
  }
  if (values.type !== 'heartbeat') {
    if (!inRange(values.timeout_s, 1, MAX_TIMEOUT_S)) {
      errors.timeout_s = `Entre 1 y ${MAX_TIMEOUT_S} segundos.`;
    } else if (values.timeout_s >= values.interval_s) {
      errors.timeout_s = 'Debe ser menor que la frecuencia.';
    }
  }

  if (isHttp(values.type) && values.expected_status.trim() !== '') {
    const status = Number(values.expected_status);
    if (!inRange(status, 100, 599)) errors.expected_status = 'Un código HTTP entre 100 y 599.';
  }
  if (values.type === 'keyword') {
    if (values.keyword.trim() === '') errors.keyword = 'Indica el texto que debe aparecer.';
    else if (values.keyword.length > 200) errors.keyword = 'Máximo 200 caracteres.';
  }

  for (const field of ['confirmation_period_s', 'recovery_period_s', 'grace_period_s'] as const) {
    if (!inRange(values[field], 0, MAX_PERIOD_S)) {
      errors[field] = `Entre 0 y ${MAX_PERIOD_S} segundos.`;
    }
  }
  return errors;
}

/** API body with only the fields that apply to the monitor type. */
export function toInput(values: MonitorFormValues): MonitorInput {
  const input: MonitorInput = {
    name: values.name.trim(),
    type: values.type,
    interval_s: values.interval_s,
    timeout_s: values.type === 'heartbeat' ? Math.min(10, values.interval_s - 1) : values.timeout_s,
    confirmation_period_s: values.confirmation_period_s,
    recovery_period_s: values.recovery_period_s,
    grace_period_s: values.grace_period_s,
  };
  if (values.type !== 'heartbeat') input.target = values.target.trim();
  if (isHttp(values.type) && values.expected_status.trim() !== '') {
    input.expected_status = Number(values.expected_status);
  }
  if (values.type === 'keyword') input.keyword = values.keyword;
  return input;
}
