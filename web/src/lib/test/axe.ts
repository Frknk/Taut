// Accessibility assertion for the component tests that run in the browser.
import axe from 'axe-core';
import { expect } from 'vitest';

/** WCAG 2.2 A and AA. Best-practice rules such as `region` assume a whole page, not a component. */
const TAGS = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'];

/** Fails with one line per axe violation found under `root` (the whole document by default). */
export async function expectAccessible(root: Element = document.body): Promise<void> {
  const { violations } = await axe.run(root, {
    runOnly: { type: 'tag', values: TAGS },
    resultTypes: ['violations'],
  });
  const lines = violations.map(
    (v) => `${v.id}: ${v.help} (${v.nodes.map((n) => n.target.join(' ')).join(', ')})`,
  );
  expect(lines).toEqual([]);
}
