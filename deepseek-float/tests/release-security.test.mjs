import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

// Exercise only the signing function with fake credentials and a fake signer.
// Never invoke the top-level release/build entry or load the ambient environment.
const source = readFileSync(new URL('../scripts/build-release.mjs', import.meta.url), 'utf8');
const signingFunction = source.slice(source.indexOf('function signArtifacts()'));
const marker = 'dummy-password-for-log-regression';
function signing({ fail = false, configured = true, sha1 = false } = {}) {
  const logs = [];
  const calls = [];
  vm.runInNewContext(signingFunction + '\nsignArtifacts();', {
    releaseBinary: () => 'dummy-output.exe',
    fs: { existsSync: () => true },
    os: { platform: () => 'win32' },
    process: { env: configured ? {
      DSFLOAT_SIGN_CERT: 'dummy.pfx', DSFLOAT_SIGN_PWD: marker,
      ...(sha1 ? { DSFLOAT_SIGN_SHA1: '0'.repeat(40) } : {}),
    } : {} },
    console: { log: (...args) => logs.push(args.join(' ')), warn: (...args) => logs.push(args.join(' ')) },
    execFileSync: (tool, args, options) => {
      calls.push({ tool, args: Array.from(args), options });
      if (fail) throw new Error('fake signer error containing ' + marker);
    },
  }, { timeout: 1000 });
  return { logs: logs.join('\n'), calls };
}

test('PFX signer receives its password but release logs never receive it', () => {
  const result = signing();
  assert.equal(result.calls.length, 1);
  const { args, options } = result.calls[0];
  assert.equal(args[args.indexOf('/p') + 1], marker);
  assert.equal(options.stdio, 'pipe');
  assert.ok(!result.logs.includes(marker));
});

test('signer errors cannot leak their argument string through exception messages', () => {
  const result = signing({ fail: true });
  assert.match(result.logs, /签名失败/);
  assert.ok(!result.logs.includes(marker));
});

test('missing signing configuration does not invoke a signer', () => {
  assert.equal(signing({ configured: false }).calls.length, 0);
});

test('certificate-store signing never sends a PFX password', () => {
  const result = signing({ sha1: true });
  assert.ok(result.calls[0].args.includes('/sha1'));
  assert.ok(!result.calls[0].args.includes('/p'));
  assert.ok(!result.logs.includes(marker));
});
