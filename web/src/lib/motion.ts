// Svelte transitions of the Taut UI motion system (ADR 0007, «Movimiento»). The CSS side lives in
// routes/layout.css; these cover what CSS cannot see: blocks that Svelte mounts and unmounts.
// Every transition drops its movement when the user prefers reduced motion and keeps the fade.

import type { TransitionConfig } from 'svelte/transition';

/** The `--ease-out` curve of layout.css (an exponential ease-out), for Svelte transitions. */
export function easeOut(t: number): number {
  return t === 1 ? 1 : 1 - 2 ** (-10 * t);
}

/** Whether the user asked the system for less motion. */
export function prefersReducedMotion(): boolean {
  return typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
}

/** Where a transition starts: how far it shifts (px), how much it shrinks and blurs. */
export interface MotionFrom {
  x?: number;
  y?: number;
  scale?: number;
  blur?: number;
}

/**
 * The inline style of a transition at progress `t` (0 = start, 1 = rest). With `reduced` only the
 * opacity changes.
 */
export function motionStyle(t: number, from: MotionFrom, reduced: boolean): string {
  const u = 1 - t;
  const parts = [`opacity: ${t}`];
  if (reduced) return parts.join('; ');
  const { x = 0, y = 0, scale = 1, blur = 0 } = from;
  if (x !== 0 || y !== 0) parts.push(`translate: ${x * u}px ${y * u}px`);
  if (scale !== 1) parts.push(`scale: ${1 - (1 - scale) * u}`);
  if (blur !== 0) parts.push(`filter: blur(${blur * u}px)`);
  return parts.join('; ');
}

function transition(from: MotionFrom, duration: number): TransitionConfig {
  const reduced = prefersReducedMotion();
  return { duration, easing: easeOut, css: (t) => motionStyle(t, from, reduced) };
}

/**
 * A new step of the same card (login → 2FA, form → «sent»): it comes in from the side it moves
 * towards, slightly blurred, so the swap reads as one change. Use it as `in:` only; the old step
 * leaves at once, so both never take space together. `back` reverses the direction.
 */
export function step(_node: Element, { back = false } = {}): TransitionConfig {
  return transition({ x: back ? -8 : 8, blur: 2 }, 200);
}

/** A small icon or badge that replaces another in place (copy → check, a new count). */
export function pop(_node: Element): TransitionConfig {
  return transition({ scale: 0.8, blur: 2 }, 150);
}

/** Content that appears inside a page already on screen (a section, a table, an org's page). */
export function appear(_node: Element, { duration = 150 } = {}): TransitionConfig {
  return transition({}, duration);
}

/** A row or item the user removed: it fades while the rest close the gap (`animate:flip`). */
export function leave(_node: Element): TransitionConfig {
  return transition({}, 150);
}

/** Duration of `animate:flip` for lists: rows close the gap left by a removed one. */
export const FLIP = { duration: 200, easing: easeOut };
