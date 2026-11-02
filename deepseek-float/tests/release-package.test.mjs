import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import test from 'node:test';

const { assembleRelease } = await import('../scripts/lib/release-package.mjs');
const sha = (data) => createHash('sha256').update(data).digest('hex');

function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'float-package-test-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const materials = path.join(root, 'materials');
  fs.mkdirSync(materials);
  fs.writeFileSync(path.join(materials, 'LICENSE'), 'fixture license\r\n');
  fs.writeFileSync(path.join(materials, 'CHECKSUMS.csv'),
    `path,sha256,bytes\nLICENSE,${sha('fixture license\r\n')},17\n`);
  fs.writeFileSync(path.join(materials, 'secret.token'), 'must never ship');
  const binary = path.join(root, 'app.exe');
  fs.writeFileSync(binary, 'fixture executable');
  const output = path.join(root, 'release');
  fs.mkdirSync(output);
  fs.writeFileSync(path.join(output, 'old.pdb'), 'must never ship');
  return { materials, binary, output };
}

test('package preserves indexed license bytes and hashes the final EXE and ZIP; unlisted files stay out', (t) => {
  const options = fixture(t);
  const result = assembleRelease(options);
  const zip = fs.readFileSync(result.zip);
  assert.equal(fs.readFileSync(path.join(options.output, 'HASH.txt'), 'utf8').trim(), sha('fixture executable').toUpperCase());
  assert.equal(fs.readFileSync(result.zip + '.sha256', 'utf8').trim(), `${sha(zip).toUpperCase()}  ${path.basename(result.zip)}`);
  const unpacked = path.join(options.output, 'unpacked');
  execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command',
    'Expand-Archive -LiteralPath $env:FLOAT_TEST_ZIP -DestinationPath $env:FLOAT_TEST_UNPACK'],
  { env: { ...process.env, FLOAT_TEST_ZIP: result.zip, FLOAT_TEST_UNPACK: unpacked } });
  assert.equal(fs.readFileSync(path.join(unpacked, 'LICENSE'), 'utf8'), 'fixture license\r\n');
  assert.equal(fs.readFileSync(path.join(unpacked, 'deepseek-float.exe'), 'utf8'), 'fixture executable');
  assert.ok(!fs.existsSync(path.join(unpacked, 'secret.token')));
  assert.ok(!fs.existsSync(path.join(unpacked, 'old.pdb')));
  assert.match(fs.readFileSync(path.join(unpacked, 'CHECKSUMS.csv'), 'utf8'), /deepseek-float\.exe,/);
});

test('tampered license material fails before producing a distributable ZIP', (t) => {
  const options = fixture(t);
  fs.appendFileSync(path.join(options.materials, 'LICENSE'), 'tampered');
  assert.throws(() => assembleRelease(options), /material checksum/i);
  assert.equal(fs.readdirSync(options.output).filter((name) => name.endsWith('.zip')).length, 0);
});

test('material index cannot copy a file outside its directory', (t) => {
  const options = fixture(t);
  fs.writeFileSync(path.join(options.materials, 'CHECKSUMS.csv'), `path,sha256,bytes\n../app.exe,${sha('fixture executable')},18\n`);
  assert.throws(() => assembleRelease(options), /material path/i);
});
