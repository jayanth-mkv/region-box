// Download a pinned upstream Gitleaks binary and verify it before execution.
// Scan only reachable publication history; never print discovered secret values.
import { createHash } from 'node:crypto';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const version = '8.30.1';
const windows = process.platform === 'win32';
if (!windows && process.platform !== 'linux') throw new Error('Secret scan supports Windows and Linux.');
if (process.arch !== 'x64') throw new Error('Secret scan requires x64.');
const asset = windows ? `gitleaks_${version}_windows_x64.zip` : `gitleaks_${version}_linux_x64.tar.gz`;
const expected = windows ? 'd29144deff3a68aa93ced33dddf84b7fdc26070add4aa0f4513094c8332afc4e' : '551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb';
const directory = await mkdtemp(join(tmpdir(), 'regionbox-secret-scan-'));
const archive = join(directory, asset);
const run = (command, args, env = process.env) => {
  const result = spawnSync(command, args, { stdio: 'inherit', windowsHide: true, env });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} failed with exit code ${result.status}`);
};
try {
  const response = await fetch(`https://github.com/gitleaks/gitleaks/releases/download/v${version}/${asset}`, { signal: AbortSignal.timeout(120000) });
  if (!response.ok) throw new Error(`Scanner download failed: ${response.status}`);
  const bytes = Buffer.from(await response.arrayBuffer());
  if (createHash('sha256').update(bytes).digest('hex') !== expected) throw new Error('Scanner checksum mismatch.');
  await writeFile(archive, bytes);
  if (windows) run('powershell.exe', ['-NoProfile', '-Command', 'Add-Type -AssemblyName System.IO.Compression.FileSystem; [IO.Compression.ZipFile]::ExtractToDirectory($env:REGIONBOX_ARCHIVE, $env:REGIONBOX_SCANNER_DIR)'], { ...process.env, REGIONBOX_ARCHIVE: archive, REGIONBOX_SCANNER_DIR: directory });
  else run('tar', ['-xzf', archive, '-C', directory]);
  run(join(directory, windows ? 'gitleaks.exe' : 'gitleaks'), ['git', '--redact', '--no-banner', '--log-opts=HEAD', '.']);
} finally {
  // Only delete the exact temporary directory created above.
  if (resolve(directory).startsWith(resolve(tmpdir()) + (windows ? '\\' : '/') + 'regionbox-secret-scan-')) await rm(directory, { recursive: true, force: true });
}
