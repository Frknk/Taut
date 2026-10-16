// Spanish labels and formatting shared by the panel and the status page.

import type { ChannelKind, MonitorKind, MonitorStatus, Role } from './types.js';
import type { Tone } from './ui.js';

export const KIND_LABELS: Record<MonitorKind, string> = {
  http: 'HTTP',
  keyword: 'Palabra clave',
  tcp: 'Puerto TCP',
  heartbeat: 'Heartbeat',
};

export const CHANNEL_LABELS: Record<ChannelKind, string> = {
  email: 'Correo',
  discord: 'Discord',
  slack: 'Slack',
  webhook: 'Webhook',
};

export const STATUS_LABELS: Record<MonitorStatus, string> = {
  pending: 'Pendiente',
  up: 'Operativo',
  down: 'Caído',
  paused: 'Pausado',
};

/**
 * Badge tone of each status. Paused is the user's choice, not a problem, so it stays neutral;
 * only `up` and `down` carry a status color.
 */
export const STATUS_TONES: Record<MonitorStatus, Tone> = {
  pending: 'outline',
  up: 'success',
  down: 'danger',
  paused: 'neutral',
};

/** A different shape per status, so color never tells them apart alone. */
export const STATUS_GLYPHS: Record<MonitorStatus, 'dot' | 'pause' | 'ring'> = {
  pending: 'ring',
  up: 'dot',
  down: 'dot',
  paused: 'pause',
};

/** `30 s`, `1 min`, `1 min 30 s`, `2 h`. */
export function formatSeconds(seconds: number): string {
  if (seconds < 60) return `${seconds} s`;
  if (seconds < 3600) {
    const minutes = Math.floor(seconds / 60);
    const rest = seconds % 60;
    return rest === 0 ? `${minutes} min` : `${minutes} min ${rest} s`;
  }
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  return minutes === 0 ? `${hours} h` : `${hours} h ${minutes} min`;
}

/** Date and time in the browser's locale, for example `15/10/2026, 21:03`. */
export function formatDateTime(iso: string): string {
  return new Date(iso).toLocaleString('es', { dateStyle: 'short', timeStyle: 'short' });
}

/** `hace 5 s`, `hace 3 min`, `hace 2 h`, `hace 4 d`. */
export function timeAgo(iso: string, now: Date = new Date()): string {
  const seconds = Math.max(0, Math.round((now.getTime() - new Date(iso).getTime()) / 1000));
  if (seconds < 60) return `hace ${seconds} s`;
  if (seconds < 3600) return `hace ${Math.floor(seconds / 60)} min`;
  if (seconds < 86_400) return `hace ${Math.floor(seconds / 3600)} h`;
  return `hace ${Math.floor(seconds / 86_400)} d`;
}

export const ROLE_LABELS: Record<Role, string> = {
  owner: 'Propietario',
  member: 'Miembro',
};
