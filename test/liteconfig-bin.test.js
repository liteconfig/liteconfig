'use strict';

const assert = require('node:assert/strict');
const test = require('node:test');

const launcher = require('../bin/liteconfig.js');

test('maps supported npm platforms to release targets', () => {
  assert.equal(launcher.targetTriple('darwin', 'arm64', false), 'aarch64-apple-darwin');
  assert.equal(launcher.targetTriple('darwin', 'x64', false), 'x86_64-apple-darwin');
  assert.equal(launcher.targetTriple('linux', 'arm64', false), 'aarch64-unknown-linux-gnu');
  assert.equal(launcher.targetTriple('linux', 'x64', false), 'x86_64-unknown-linux-gnu');
});

test('rejects unsupported platforms and musl linux', () => {
  assert.throws(() => launcher.targetTriple('win32', 'x64', false), /Unsupported OS/);
  assert.throws(() => launcher.targetTriple('linux', 'x64', true), /musl/);
  assert.throws(() => launcher.targetTriple('linux', 'ia32', false), /Unsupported architecture/);
});

test('parses checksums by exact asset name', () => {
  const sums = [
    'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  liteconfig-v0.1.0-x86_64-unknown-linux-gnu.tar.gz',
    'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb  other.tar.gz',
  ].join('\n');
  assert.equal(
    launcher.parseChecksumFile(sums, 'liteconfig-v0.1.0-x86_64-unknown-linux-gnu.tar.gz'),
    'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
  );
  assert.throws(() => launcher.parseChecksumFile(sums, 'missing.tar.gz'), /No SHA256/);
});

test('builds release URLs without shell interpolation', () => {
  assert.equal(
    launcher.releaseBaseUrl('liteconfig/liteconfig', 'v0.1.0'),
    'https://github.com/liteconfig/liteconfig/releases/download/v0.1.0',
  );
});

test('rejects unsafe archive paths before extraction', () => {
  assert.doesNotThrow(() => launcher.validateArchivePaths(['liteconfig']));
  assert.doesNotThrow(() => launcher.validateArchivePaths(['package/liteconfig']));
  assert.throws(() => launcher.validateArchivePaths(['../liteconfig']), /Unsafe archive path/);
  assert.throws(() => launcher.validateArchivePaths(['/tmp/liteconfig']), /Unsafe archive path/);
  assert.throws(() => launcher.validateArchivePaths(['package/readme.txt']), /does not contain/);
});
