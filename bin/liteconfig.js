#!/usr/bin/env node
'use strict';

const childProcess = require('node:child_process');
const crypto = require('node:crypto');
const fs = require('node:fs');
const https = require('node:https');
const os = require('node:os');
const path = require('node:path');

const packageJson = require('../package.json');

const DEFAULT_REPO = 'liteconfig/liteconfig';
const BIN_NAME = process.platform === 'win32' ? 'liteconfig.exe' : 'liteconfig';

function targetTriple(platform = process.platform, arch = process.arch, musl = isMuslLinux()) {
  let osPart;
  if (platform === 'darwin') {
    osPart = 'apple-darwin';
  } else if (platform === 'linux') {
    if (musl) {
      throw new Error('Prebuilt musl Linux binaries are not published yet. Use cargo install liteconfig-tui.');
    }
    osPart = 'unknown-linux-gnu';
  } else {
    throw new Error(`Unsupported OS: ${platform}. Use cargo install liteconfig-tui.`);
  }

  let archPart;
  if (arch === 'x64') {
    archPart = 'x86_64';
  } else if (arch === 'arm64') {
    archPart = 'aarch64';
  } else {
    throw new Error(`Unsupported architecture: ${arch}. Use cargo install liteconfig-tui.`);
  }

  return `${archPart}-${osPart}`;
}

function isMuslLinux() {
  if (process.platform !== 'linux') return false;
  const report = typeof process.report?.getReport === 'function' ? process.report.getReport() : null;
  return !report?.header?.glibcVersionRuntime;
}

function cacheRoot() {
  if (process.env.LITECONFIG_CACHE_DIR) return process.env.LITECONFIG_CACHE_DIR;
  if (process.env.XDG_CACHE_HOME) return path.join(process.env.XDG_CACHE_HOME, 'liteconfig');
  if (process.platform === 'darwin') return path.join(os.homedir(), 'Library', 'Caches', 'liteconfig');
  return path.join(os.homedir(), '.cache', 'liteconfig');
}

function releaseVersion() {
  const explicit = process.env.LITECONFIG_VERSION;
  if (explicit) return explicit.startsWith('v') ? explicit : `v${explicit}`;
  return `v${packageJson.version}`;
}

function releaseBaseUrl(repo, version) {
  return `https://github.com/${repo}/releases/download/${version}`;
}

function parseChecksumFile(text, assetName) {
  for (const line of text.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    const [hash, file] = trimmed.split(/\s+/);
    if (file === assetName && /^[a-fA-F0-9]{64}$/.test(hash)) {
      return hash.toLowerCase();
    }
  }
  throw new Error(`No SHA256 checksum found for ${assetName}`);
}

function sha256File(file) {
  const hash = crypto.createHash('sha256');
  const data = fs.readFileSync(file);
  hash.update(data);
  return hash.digest('hex');
}

function download(url, dest, redirects = 0) {
  if (redirects > 5) throw new Error(`Too many redirects while downloading ${url}`);
  return new Promise((resolve, reject) => {
    const request = https.get(
      url,
      {
        headers: {
          'User-Agent': `liteconfig-npm/${packageJson.version}`,
        },
      },
      (response) => {
        const status = response.statusCode || 0;
        if ([301, 302, 303, 307, 308].includes(status)) {
          response.resume();
          const location = response.headers.location;
          if (!location) {
            reject(new Error(`Redirect without location from ${url}`));
            return;
          }
          download(new URL(location, url).toString(), dest, redirects + 1).then(resolve, reject);
          return;
        }
        if (status !== 200) {
          response.resume();
          reject(new Error(`Download failed (${status}) from ${url}`));
          return;
        }
        const out = fs.createWriteStream(dest, { mode: 0o600 });
        response.pipe(out);
        out.on('finish', () => out.close(resolve));
        out.on('error', reject);
      },
    );
    request.on('error', reject);
  });
}

async function downloadText(url, dest) {
  await download(url, dest);
  return fs.readFileSync(dest, 'utf8');
}

function extractBinary(archive, destDir) {
  fs.mkdirSync(destDir, { recursive: true });
  validateArchivePaths(listArchivePaths(archive));
  childProcess.execFileSync('tar', ['-xzf', archive, '-C', destDir], { stdio: 'ignore' });
  const direct = path.join(destDir, BIN_NAME);
  if (fs.existsSync(direct) && fs.statSync(direct).isFile()) return direct;

  const found = findFile(destDir, BIN_NAME);
  if (!found) throw new Error(`Binary ${BIN_NAME} not found inside release archive`);
  return found;
}

function findFile(dir, name) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const candidate = path.join(dir, entry.name);
    if (entry.isFile() && entry.name === name) return candidate;
    if (entry.isDirectory()) {
      const nested = findFile(candidate, name);
      if (nested) return nested;
    }
  }
  return null;
}

function listArchivePaths(archive) {
  return childProcess
    .execFileSync('tar', ['-tzf', archive], { encoding: 'utf8' })
    .split(/\r?\n/)
    .filter(Boolean);
}

function validateArchivePaths(entries) {
  if (!entries.some((entry) => path.basename(entry) === BIN_NAME)) {
    throw new Error(`Release archive does not contain ${BIN_NAME}`);
  }
  for (const entry of entries) {
    const normalized = path.posix.normalize(entry.replace(/\\/g, '/'));
    if (
      normalized.startsWith('../') ||
      normalized === '..' ||
      path.posix.isAbsolute(normalized)
    ) {
      throw new Error(`Unsafe archive path: ${entry}`);
    }
  }
}

async function ensureBinary() {
  const repo = process.env.LITECONFIG_REPO || DEFAULT_REPO;
  const version = releaseVersion();
  const target = targetTriple();
  const asset = `liteconfig-${version}-${target}.tar.gz`;
  const baseUrl = releaseBaseUrl(repo, version);
  const cacheDir = path.join(cacheRoot(), version, target);
  const cachedBin = path.join(cacheDir, BIN_NAME);

  if (fs.existsSync(cachedBin)) return cachedBin;

  fs.mkdirSync(cacheDir, { recursive: true, mode: 0o755 });
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'liteconfig-'));
  try {
    const checksumsPath = path.join(tmp, 'SHA256SUMS');
    const archivePath = path.join(tmp, asset);
    const sums = await downloadText(`${baseUrl}/SHA256SUMS`, checksumsPath);
    await download(`${baseUrl}/${asset}`, archivePath);

    const expected = parseChecksumFile(sums, asset);
    const actual = sha256File(archivePath);
    if (expected !== actual) {
      throw new Error(`Checksum mismatch for ${asset}`);
    }

    const extracted = extractBinary(archivePath, path.join(tmp, 'extract'));
    const staged = path.join(cacheDir, `${BIN_NAME}.${process.pid}.tmp`);
    fs.copyFileSync(extracted, staged);
    fs.chmodSync(staged, 0o755);
    fs.renameSync(staged, cachedBin);
    return cachedBin;
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
}

async function main(argv = process.argv.slice(2)) {
  const bin = await ensureBinary();
  const child = childProcess.spawn(bin, argv, {
    stdio: 'inherit',
    shell: false,
  });
  child.on('exit', (code, signal) => {
    if (signal) {
      process.kill(process.pid, signal);
      return;
    }
    process.exit(code ?? 1);
  });
  child.on('error', (error) => {
    console.error(`liteconfig: failed to launch: ${error.message}`);
    process.exit(1);
  });
}

if (require.main === module) {
  main().catch((error) => {
    console.error(`liteconfig: ${error.message}`);
    process.exit(1);
  });
}

module.exports = {
  cacheRoot,
  parseChecksumFile,
  validateArchivePaths,
  releaseBaseUrl,
  releaseVersion,
  targetTriple,
};
