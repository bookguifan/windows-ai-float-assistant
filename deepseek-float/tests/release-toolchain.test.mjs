import assert from 'node:assert/strict';
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
