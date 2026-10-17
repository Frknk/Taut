import { describe, expect, it } from 'vitest';

import { badgeClass, bannerClass, buttonClass, navItemClass } from './ui';

describe('buttonClass', () => {
  it('defaults to a 36px secondary button', () => {
    const classes = buttonClass();
    expect(classes).toContain('bg-control');
    expect(classes).toContain('h-9');
  });

  it('uses the brand ink for primary and keeps the focus ring', () => {
    const classes = buttonClass('primary');
    expect(classes).toContain('bg-brand text-on-brand');
    expect(classes).toContain('focus-visible:outline-focus');
  });

  it('makes icon-only buttons square at every size', () => {
    expect(buttonClass('ghost', 'sm', true)).toContain('w-[26px]');
    expect(buttonClass('ghost', 'base', true)).toContain('w-9');
  });
});

describe('tones', () => {
  it('pairs each status text with its tint', () => {
    expect(badgeClass('danger')).toContain('bg-danger-tint text-danger');
    expect(bannerClass('success')).toContain('bg-success-tint text-success');
  });
});

describe('navItemClass', () => {
  it('marks the current page and keeps the focus ring', () => {
    expect(navItemClass(true)).toContain('bg-tint font-medium');
    expect(navItemClass(false)).toContain('text-subtle');
    expect(navItemClass(false)).toContain('focus-visible:outline-focus');
  });
});
