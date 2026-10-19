import { describe, expect, it } from 'vitest';

import { easeOut, motionStyle, prefersReducedMotion } from './motion';

describe('easeOut', () => {
  it('starts at 0, ends at 1 and front-loads the movement', () => {
    expect(easeOut(0)).toBe(0);
    expect(easeOut(1)).toBe(1);
    expect(easeOut(0.2)).toBeGreaterThan(0.7);
  });
});

describe('motionStyle', () => {
  it('moves, scales and blurs from the start towards rest', () => {
    const from = { x: 8, scale: 0.8, blur: 2 };
    expect(motionStyle(0, from, false)).toBe(
      'opacity: 0; translate: 8px 0px; scale: 0.8; filter: blur(2px)',
    );
    expect(motionStyle(1, from, false)).toBe(
      'opacity: 1; translate: 0px 0px; scale: 1; filter: blur(0px)',
    );
  });

  it('only fades when the user prefers reduced motion', () => {
    expect(motionStyle(0.5, { x: 8, scale: 0.8, blur: 2 }, true)).toBe('opacity: 0.5');
  });

  it('leaves out what does not change', () => {
    expect(motionStyle(0.5, {}, false)).toBe('opacity: 0.5');
  });
});

describe('prefersReducedMotion', () => {
  it('is false where matchMedia does not exist', () => {
    expect(prefersReducedMotion()).toBe(false);
  });
});
