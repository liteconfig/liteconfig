'use strict';

const assert = require('node:assert/strict');
const test = require('node:test');

const formula = require('../scripts/render-homebrew-formula.js');

test('normalizeVersion strips a leading v', () => {
  assert.equal(formula.normalizeVersion('v1.2.3'), '1.2.3');
  assert.equal(formula.normalizeVersion('1.2.3'), '1.2.3');
});

test('parseChecksums and renderFormula fill all placeholders', () => {
  const template = [
    'version "__VERSION__"',
    '__SHA256_AARCH64_APPLE_DARWIN__',
    '__SHA256_X86_64_APPLE_DARWIN__',
    '__SHA256_AARCH64_LINUX_GNU__',
    '__SHA256_X86_64_LINUX_GNU__',
  ].join('\n');
  const checksums = formula.parseChecksums(
    [
      `${'a'.repeat(64)}  ${formula.assetName('1.2.3', 'aarch64-apple-darwin')}`,
      `${'b'.repeat(64)}  ${formula.assetName('1.2.3', 'x86_64-apple-darwin')}`,
      `${'c'.repeat(64)}  ${formula.assetName('1.2.3', 'aarch64-unknown-linux-gnu')}`,
      `${'d'.repeat(64)}  ${formula.assetName('1.2.3', 'x86_64-unknown-linux-gnu')}`,
    ].join('\n')
  );

  const rendered = formula.renderFormula(template, 'v1.2.3', checksums);
  assert.match(rendered, /version "1.2.3"/);
  assert.doesNotMatch(rendered, /__[A-Z0-9_]+__/);
  assert.match(rendered, new RegExp(`^${'a'.repeat(64)}`, 'm'));
  assert.match(rendered, new RegExp(`^${'d'.repeat(64)}`, 'm'));
});

test('renderFormula fails when a target checksum is missing', () => {
  const checksums = formula.parseChecksums(
    `${'a'.repeat(64)}  ${formula.assetName('1.2.3', 'aarch64-apple-darwin')}\n`
  );
  assert.throws(
    () => formula.renderFormula('__SHA256_X86_64_APPLE_DARWIN__', '1.2.3', checksums),
    /Missing checksum/
  );
});
