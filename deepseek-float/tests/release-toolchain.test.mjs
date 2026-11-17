import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

let toolchain;
try {
  toolchain = await import('../scripts/lib/release-toolchain.mjs');
} catch {
  toolchain = null;
}

function helpers() {
  assert.ok(toolchain, 'release toolchain helpers should be present');
  return toolchain;
}

test('Node release runtime satisfies the package engine range', () => {
  const { assertNodeEngineVersion } = helpers();
  assert.equal(assertNodeEngineVersion('v24.15.0', '^24.15.0'), true);
  assert.equal(assertNodeEngineVersion('v24.22.1', '^24.15.0'), true);
  assert.throws(() => assertNodeEngineVersion('v24.14.0', '^24.15.0'), /requires Node/);
  assert.throws(() => assertNodeEngineVersion('v25.0.0', '^24.15.0'), /requires Node/);
});

test('release uses the exact pnpm version declared by packageManager', () => {
  const { pinnedPnpmVersion } = helpers();
  assert.equal(pinnedPnpmVersion('pnpm@9.15.9'), '9.15.9');
  assert.throws(() => pinnedPnpmVersion('npm@10.8.2'), /must pin pnpm/);
});

test('pnpm child processes resolve Node from the release entry runtime first', () => {
  const { nodeBoundEnvironment } = helpers();
  const environment = nodeBoundEnvironment(
    { Path: 'C:\\Windows\\System32', KEEP: 'value' },
    'C:\\Tools\\node\\node.exe',
    'win32',
  );
  assert.equal(environment.Path, 'C:\\Tools\\node;C:\\Windows\\System32');
  assert.equal(environment.KEEP, 'value');
});

test('release rejects a pnpm process that uses a different Node runtime', () => {
  const { assertPnpmRuntime } = helpers();
  const base = {
    actualPnpmVersion: '9.15.9',
    expectedPnpmVersion: '9.15.9',
    actualNodeExecutable: 'C:\\Tools\\node\\node.exe',
    expectedNodeExecutable: 'c:\\tools\\node\\NODE.exe',
    actualNodeVersion: 'v24.22.1',
    expectedNodeVersion: 'v24.22.1',
    platform: 'win32',
  };
  assert.equal(assertPnpmRuntime(base), true);
  assert.throws(() => assertPnpmRuntime({ ...base, actualNodeVersion: 'v24.14.0' }), /different Node version/);
  assert.throws(() => assertPnpmRuntime({ ...base, actualNodeExecutable: 'C:\\Other\\node.exe' }), /different Node executable/);
  assert.throws(() => assertPnpmRuntime({ ...base, actualPnpmVersion: '12.4.1' }), /requires pnpm/);
});

// Exercise the real release entry without installing dependencies or compiling Rust.
// Only the external package-manager boundary is replaced. This tests propagation
// of lifecycle failures, not detection of a standalone pnpm EXE's embedded Node.
function releasePreflight(t, rejectLifecycle) {
  const project = fileURLToPath(new URL('../', import.meta.url));
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'float-toolchain-test-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.mkdirSync(path.join(root, 'scripts', 'lib'), { recursive: true });
  for (const file of [
    'package.json',
    'scripts/build-release.mjs',
    'scripts/lib/release-toolchain.mjs',
    'scripts/lib/release-package.mjs',
  ]) {
    fs.copyFileSync(path.join(project, file), path.join(root, file));
  }
  fs.writeFileSync(path.join(root, 'pnpm-fixture.cjs'), `
    const fs = require('node:fs');
    const args = process.argv.slice(2);
    fs.appendFileSync('pnpm-calls.jsonl', JSON.stringify(args) + '\\n');
    const command = args.join(' ');
    if (command === '--version') console.log('9.15.9');
    else if (command === 'exec node -p process.execPath') console.log(process.execPath);
    else if (command === 'exec node -p process.version') console.log(process.version);
    else if (command === 'run check:version') {
      if (${rejectLifecycle}) {
        console.error('ERR_PNPM_UNSUPPORTED_ENGINE: configured Node v20.11.1');
        process.exit(1);
      }
      console.log('version check passed');
    } else {
      console.error('Unexpected build or installation command: ' + command);
      process.exit(77);
    }
  `);
  fs.writeFileSync(path.join(root, 'pnpm.cmd'),
    `@echo off\r\n"${process.execPath}" "%~dp0pnpm-fixture.cjs" %*\r\n`);
  const result = spawnSync(process.execPath,
    ['scripts/build-release.mjs', '--check-toolchain-only'],
    { cwd: root, encoding: 'utf8', timeout: 30000 });
  assert.ifError(result.error);
  const callsFile = path.join(root, 'pnpm-calls.jsonl');
  const calls = fs.existsSync(callsFile)
    ? fs.readFileSync(callsFile, 'utf8').trim().split('\n').map((line) => JSON.parse(line))
    : [];
  return { ...result, calls };
}

test('release preflight stops on lifecycle engine rejection after pnpm exec checks pass',
  { skip: process.platform !== 'win32' }, (t) => {
    const result = releasePreflight(t, true);
    assert.notEqual(result.status, 0, 'pnpm lifecycle failure must stop the release preflight');
    assert.match(result.stderr, /ERR_PNPM_UNSUPPORTED_ENGINE/);
    assert.doesNotMatch(result.stdout, /Release toolchain check complete/);
  });

test('release preflight validates pnpm lifecycle scripts before reporting success without building',
  { skip: process.platform !== 'win32' }, (t) => {
    const result = releasePreflight(t, false);
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /Release toolchain check complete/);
    assert.ok(result.calls.some((args) => args.join(' ') === 'run check:version'));
    assert.ok(result.calls.every((args) => ['--version', 'exec', 'run'].includes(args[0])));
  });

test('project engine policy rejects an unsupported Node target through real pnpm',
  { skip: process.platform !== 'win32' }, (t) => {
    const project = fileURLToPath(new URL('../', import.meta.url));
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'float-engine-policy-test-'));
    t.after(() => fs.rmSync(root, { recursive: true, force: true }));
    fs.copyFileSync(path.join(project, '.npmrc'), path.join(root, '.npmrc'));
    const { packageManager, engines } = JSON.parse(fs.readFileSync(path.join(project, 'package.json'), 'utf8'));
    fs.writeFileSync(path.join(root, 'package.json'), JSON.stringify({
      name: 'engine-policy-fixture', private: true, packageManager, engines,
      scripts: { 'check:version': 'node -e "process.exit(0)"' },
    }));
    const env = { ...process.env };
    // Test the copied project policy, not engine-strict inherited from our caller.
    for (const key of Object.keys(env)) {
      if (key.toLowerCase() === 'npm_config_engine_strict') delete env[key];
    }
    // pnpm's node-version setting exercises the real engine check without
    // downloading an obsolete Node or depending on a standalone pnpm EXE.
    const result = spawnSync(process.env.ComSpec || 'cmd.exe',
      ['/d', '/s', '/c', 'pnpm --config.node-version=20.11.1 run check:version'],
      { cwd: root, encoding: 'utf8', env, timeout: 30000 });
    assert.ifError(result.error);
    assert.notEqual(result.status, 0, 'project configuration must reject an unsupported Node target');
    assert.match(result.stdout + result.stderr, /ERR_PNPM_UNSUPPORTED_ENGINE/);
  });
