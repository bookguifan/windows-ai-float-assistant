import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';

const sha256 = (data) => createHash('sha256').update(data).digest('hex');

// Copy only indexed, byte-verified legal materials. Never collect a working
// directory or a previous release directory into the ZIP.
export function assembleRelease({ materials, binary, output, releaseNotes }) {
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'float-release-'));
  const payload = path.join(temporary, 'payload');
  fs.mkdirSync(payload);
  const files = [];
  try {
    const rows = fs.readFileSync(path.join(materials, 'CHECKSUMS.csv'), 'utf8').trim().split(/\r?\n/);
    if (rows.shift() !== 'path,sha256,bytes' || rows.length === 0) throw new Error('Invalid material checksum index');
    for (const row of rows) {
      const [name, expectedHash, expectedBytes] = row.split(',');
      if (!name || name.includes('\\') || name.includes(':') || name.startsWith('/')
          || name.split('/').some((part) => !part || part === '.' || part === '..')) {
        throw new Error(`Invalid material path: ${name}`);
      }
      const source = path.join(materials, name);
      // Reject links in every path component, including directory junctions.
      let current = materials;
      for (const part of name.split('/')) {
        current = path.join(current, part);
        if (fs.lstatSync(current).isSymbolicLink()) throw new Error(`Invalid material path: ${name}`);
      }
      const bytes = fs.readFileSync(source);
      if (sha256(bytes) !== expectedHash || bytes.length !== Number(expectedBytes)) {
        throw new Error(`Material checksum mismatch: ${name}`);
      }
      // Keep internal provenance/compile-input evidence in the source repository.
      // License texts, attribution, source archives and source correspondence stay.
      if (['.gitattributes', 'files.csv', 'mpl-correspondence/COMPILER-INPUTS.csv'].includes(name)) continue;
      const destination = path.join(payload, name);
      fs.mkdirSync(path.dirname(destination), { recursive: true });
      fs.writeFileSync(destination, bytes);
      files.push(name);
    }
    fs.copyFileSync(binary, path.join(payload, 'deepseek-float.exe'));
    fs.writeFileSync(path.join(payload, 'HASH.txt'), sha256(fs.readFileSync(binary)).toUpperCase() + os.EOL);
    files.push('deepseek-float.exe', 'HASH.txt');
    if (releaseNotes) {
      fs.copyFileSync(releaseNotes, path.join(payload, 'RELEASE_NOTES.md'));
      files.push('RELEASE_NOTES.md');
    }
    const checksums = files.sort().map((name) => {
      const bytes = fs.readFileSync(path.join(payload, name));
      return `${name},${sha256(bytes)},${bytes.length}`;
    });
    fs.writeFileSync(path.join(payload, 'CHECKSUMS.csv'), 'path,sha256,bytes\n' + checksums.join('\n') + '\n');
    const zipName = 'windows-ai-float-assistant-v0.1.0-beta.2-windows-x64.zip';
    const temporaryZip = path.join(temporary, zipName);
    execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command',
      "$ErrorActionPreference = 'Stop'; $items = @(Get-ChildItem -LiteralPath $env:FLOAT_RELEASE_PAYLOAD | ForEach-Object FullName); Compress-Archive -LiteralPath $items -DestinationPath $env:FLOAT_RELEASE_ZIP -CompressionLevel Optimal"],
    { stdio: 'pipe', env: { ...process.env, FLOAT_RELEASE_PAYLOAD: payload, FLOAT_RELEASE_ZIP: temporaryZip } });
    const zipHash = sha256(fs.readFileSync(temporaryZip)).toUpperCase();
    fs.mkdirSync(output, { recursive: true });
    // Update only generated filenames; never remove unrelated local files.
    fs.cpSync(payload, output, { recursive: true });
    const zip = path.join(output, zipName);
    fs.copyFileSync(temporaryZip, zip);
    fs.writeFileSync(zip + '.sha256', `${zipHash}  ${zipName}${os.EOL}`);
    return { zip, zipHash, exeHash: sha256(fs.readFileSync(binary)).toUpperCase(), files: files.length + 1 };
  } finally {
    // mkdtemp produced this absolute directory under the system temp directory.
    if (path.dirname(temporary) !== path.resolve(os.tmpdir())) throw new Error('Invalid temporary package directory');
    fs.rmSync(temporary, { recursive: true, force: true });
  }
}
