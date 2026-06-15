'use strict';

const assert = require('node:assert/strict');
const childProcess = require('node:child_process');
const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');

const repoRoot = path.resolve(__dirname, '..');
const installScript = path.join(repoRoot, 'install.sh');
const version = 'v1.2.3';
const target = 'x86_64-unknown-linux-gnu';
const asset = `liteconfig-${version}-${target}.tar.gz`;

function writeExecutable(filePath, body) {
  fs.writeFileSync(filePath, body, { mode: 0o755 });
}

function createFixture(options = {}) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'liteconfig-install-test-'));
  const fixtureDir = path.join(root, 'fixtures');
  const payloadDir = path.join(root, 'payload');
  const fakeBin = path.join(root, 'fake-bin');
  const homeDir = path.join(root, 'home');
  const installDir = path.join(root, 'bin');

  fs.mkdirSync(fixtureDir);
  fs.mkdirSync(payloadDir);
  fs.mkdirSync(fakeBin);
  fs.mkdirSync(homeDir);

  const payloadBin = path.join(payloadDir, 'liteconfig');
  writeExecutable(payloadBin, '#!/bin/sh\necho "liteconfig fixture"\n');

  const assetPath = path.join(fixtureDir, asset);
  childProcess.execFileSync('tar', ['-C', payloadDir, '-czf', assetPath, 'liteconfig']);

  const hash = crypto.createHash('sha256').update(fs.readFileSync(assetPath)).digest('hex');
  const sumsText =
    options.sumsText ??
    `${hash}  ${asset}\n`;
  fs.writeFileSync(path.join(fixtureDir, 'SHA256SUMS'), sumsText);

  writeExecutable(
    path.join(fakeBin, 'curl'),
    `#!/bin/sh
set -eu
out=""
url=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o)
      out="$2"
      shift 2
      ;;
    --retry|--retry-delay)
      shift 2
      ;;
    -*)
      shift
      ;;
    *)
      url="$1"
      shift
      ;;
  esac
done
case "$url" in
  */SHA256SUMS)
    if [ "\${FAKE_CURL_MODE:-ok}" = "missing-sums" ]; then
      exit 22
    fi
    src="\${FAKE_FIXTURE_DIR}/SHA256SUMS"
    ;;
  */*.tar.gz)
    src="\${FAKE_FIXTURE_DIR}/\${FAKE_ASSET_NAME}"
    ;;
  *)
    echo "unexpected curl url: $url" >&2
    exit 64
    ;;
esac
if [ -n "$out" ]; then
  cp "$src" "$out"
else
  cat "$src"
fi
`
  );

  writeExecutable(
    path.join(fakeBin, 'uname'),
    `#!/bin/sh
case "$1" in
  -m) printf '%s\\n' "\${FAKE_UNAME_M:-x86_64}" ;;
  *) printf '%s\\n' "\${FAKE_UNAME_S:-Linux}" ;;
esac
`
  );

  writeExecutable(
    path.join(fakeBin, 'ldd'),
    `#!/bin/sh
if [ "\${FAKE_MUSL:-0}" = "1" ]; then
  printf '%s\\n' 'musl libc'
else
  printf '%s\\n' 'ldd (GNU libc)'
fi
`
  );

  writeExecutable(
    path.join(fakeBin, 'shasum'),
    `#!/bin/sh
set -eu
[ "$1" = "-a" ] || exit 64
[ "$2" = "256" ] || exit 64
node -e 'const crypto = require("node:crypto"); const fs = require("node:fs"); const file = process.argv[1]; const hash = crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex"); process.stdout.write(hash + "  " + file + "\\n");' "$3"
`
  );

  return { fixtureDir, homeDir, installDir, fakeBin, hash };
}

function runInstall(options = {}) {
  const fixture = createFixture(options);
  const result = childProcess.spawnSync('/bin/sh', [installScript], {
    cwd: repoRoot,
    encoding: 'utf8',
    env: {
      ...process.env,
      HOME: fixture.homeDir,
      LITECONFIG_BIN_DIR: fixture.installDir,
      LITECONFIG_REPO: 'liteconfig/liteconfig',
      LITECONFIG_VERSION: version,
      FAKE_ASSET_NAME: asset,
      FAKE_FIXTURE_DIR: fixture.fixtureDir,
      FAKE_UNAME_S: 'Linux',
      FAKE_UNAME_M: 'x86_64',
      PATH: `${fixture.fakeBin}:${process.env.PATH}`,
      ...options.env,
    },
  });

  return {
    ...fixture,
    ...result,
    output: `${result.stdout}${result.stderr}`,
    installedBinary: path.join(fixture.installDir, 'liteconfig'),
  };
}

test('install.sh installs when checksum matches', () => {
  const run = runInstall();
  assert.equal(run.status, 0, run.output);
  assert.match(run.output, /Checksum verified/);
  assert.ok(fs.existsSync(run.installedBinary));
});

test('install.sh fails closed when SHA256SUMS cannot be downloaded', () => {
  const run = runInstall({ env: { FAKE_CURL_MODE: 'missing-sums' } });
  assert.notEqual(run.status, 0, run.output);
  assert.match(run.output, /Could not download SHA256SUMS/);
  assert.ok(!fs.existsSync(run.installedBinary));
});

test('install.sh fails when SHA256SUMS does not contain the asset', () => {
  const run = runInstall({
    sumsText: `${'a'.repeat(64)}  some-other-file.tar.gz\n`,
  });
  assert.notEqual(run.status, 0, run.output);
  assert.match(run.output, /No checksum for/);
  assert.ok(!fs.existsSync(run.installedBinary));
});

test('install.sh fails when checksum does not match', () => {
  const run = runInstall({
    sumsText: `${'b'.repeat(64)}  ${asset}\n`,
  });
  assert.notEqual(run.status, 0, run.output);
  assert.match(run.output, /Checksum mismatch!/);
  assert.ok(!fs.existsSync(run.installedBinary));
});

test('install.sh rejects musl Linux and points to cargo', () => {
  const run = runInstall({ env: { FAKE_MUSL: '1' } });
  assert.notEqual(run.status, 0, run.output);
  assert.match(run.output, /musl Linux binaries are not published/);
  assert.match(run.output, /cargo install liteconfig-tui/);
  assert.ok(!fs.existsSync(run.installedBinary));
});
