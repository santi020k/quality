import assert from 'node:assert/strict';
import test from 'node:test';
import { getReleaseLine, getDependencyReleaseLine } from './changelog.mjs';

test('release notes retain reviewed migration paragraphs without network access', () => {
  assert.equal(getReleaseLine({ summary: 'Select affected checks.\n\nMigration: declare shared inputs.' }, 'major'),
    '- Select affected checks.\n\n  Migration: declare shared inputs.');
});
test('dependency notes are deterministic and deduplicated', () => {
  const dependencies = [{ name: 'z', newVersion: '2.0.0' }, { name: 'a', newVersion: '1.0.0' }];
  assert.equal(getDependencyReleaseLine([], [...dependencies, ...dependencies]),
    '- Updated dependency: a@1.0.0\n- Updated dependency: z@2.0.0');
  assert.equal(getDependencyReleaseLine([], dependencies.reverse()),
    getDependencyReleaseLine([], dependencies));
});
test('invalid release notes fail explicitly', () => {
  assert.throws(() => getReleaseLine({ summary: ' ' }, 'patch'));
  assert.throws(() => getReleaseLine({ summary: 'Change' }, 'unknown'));
  assert.throws(() => getDependencyReleaseLine([], [{ name: 'a' }]));
});
