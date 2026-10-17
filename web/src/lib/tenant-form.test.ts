import { describe, expect, it } from 'vitest';

import { slugify } from './tenant-form';

describe('slugify', () => {
  it('turns a name into a slug', () => {
    expect(slugify('Acme Corp')).toBe('acme-corp');
    expect(slugify('  Señal Ñandú!  ')).toBe('senal-nandu');
    expect(slugify('a--b__c')).toBe('a-b-c');
  });

  it('caps the length at 40', () => {
    expect(slugify('x'.repeat(60))).toHaveLength(40);
  });
});
