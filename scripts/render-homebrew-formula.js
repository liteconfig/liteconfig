#!/usr/bin/env node
'use strict';

const fs = require('node:fs');
const path = require('node:path');

const TARGET_PLACEHOLDERS = new Map([
  ['aarch64-apple-darwin', '__SHA256_AARCH64_APPLE_DARWIN__'],
  ['x86_64-apple-darwin', '__SHA256_X86_64_APPLE_DARWIN__'],
  ['aarch64-unknown-linux-gnu', '__SHA256_AARCH64_LINUX_GNU__'],
  ['x86_64-unknown-linux-gnu', '__SHA256_X86_64_LINUX_GNU__'],
]);

function normalizeVersion(versionOrTag) {
  if (!versionOrTag) {
    throw new Error('Release version or tag is required');
  }
  return versionOrTag.startsWith('v') ? versionOrTag.slice(1) : versionOrTag;
}

function parseChecksums(text) {
  const checksums = new Map();
  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!line) continue;
    const match = line.match(/^([a-fA-F0-9]{64})\s+(.+)$/);
    if (!match) {
      throw new Error(`Invalid SHA256SUMS line: ${rawLine}`);
    }
    checksums.set(match[2], match[1].toLowerCase());
  }
  return checksums;
}

function assetName(version, target) {
  return `liteconfig-v${version}-${target}.tar.gz`;
}

function renderFormula(template, versionOrTag, checksums) {
  const version = normalizeVersion(versionOrTag);
  let rendered = template.replace(/__VERSION__/g, version);

  for (const [target, placeholder] of TARGET_PLACEHOLDERS) {
    const checksum = checksums.get(assetName(version, target));
    if (!checksum) {
      throw new Error(`Missing checksum for ${assetName(version, target)}`);
    }
    rendered = rendered.replace(placeholder, checksum);
  }

  const leftover = rendered.match(/__VERSION__|__SHA256_[A-Z0-9_]+__/);
  if (leftover) {
    throw new Error(`Unreplaced placeholder: ${leftover[0]}`);
  }
  return rendered;
}

function main(argv = process.argv.slice(2)) {
  if (argv.length !== 4) {
    throw new Error(
      'usage: render-homebrew-formula.js <version-or-tag> <template> <SHA256SUMS> <output>'
    );
  }

  const [versionOrTag, templatePath, sumsPath, outputPath] = argv;
  const template = fs.readFileSync(templatePath, 'utf8');
  const checksums = parseChecksums(fs.readFileSync(sumsPath, 'utf8'));
  const rendered = renderFormula(template, versionOrTag, checksums);

  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, rendered);
}

if (require.main === module) {
  try {
    main();
  } catch (error) {
    console.error(`render-homebrew-formula: ${error.message}`);
    process.exit(1);
  }
}

module.exports = {
  TARGET_PLACEHOLDERS,
  assetName,
  normalizeVersion,
  parseChecksums,
  renderFormula,
};
