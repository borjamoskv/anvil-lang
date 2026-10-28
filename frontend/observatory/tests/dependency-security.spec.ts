import { createRequire } from 'node:module';
import { expect, test } from '@playwright/test';

const require = createRequire(import.meta.url);
const astroPackage = require.resolve('astro/package.json');
const astroRequire = createRequire(astroPackage);

function versionAtLeast(actual: string, minimum: [number, number, number]): boolean {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(actual);
  if (!match) return false;
  const parts = match.slice(1).map(Number);
  for (let i = 0; i < minimum.length; i++) {
    if (parts[i] !== minimum[i]) return parts[i] > minimum[i];
  }
  return true;
}

test('installed Astro and Sharp include the AVIF security fixes', () => {
  const astro = require(astroPackage) as { version: string };
  const sharp = astroRequire('sharp') as { versions: { sharp: string } };
  expect(versionAtLeast(astro.version, [7, 2, 8])).toBe(true);
  expect(versionAtLeast(sharp.versions.sharp, [0, 35, 4])).toBe(true);
});
