// No secrets are needed to build RegionBox. Fail before publishing local data.
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';

const git = (...args) => execFileSync('git', args, { encoding: 'utf8', windowsHide: true });
const files = git('ls-files', '-z').split('\0').filter(Boolean);
const failures = [];
for (const path of files) {
  if (/(?:^|\/)(?:\.env(?:\..*)?|nord-user|nord-password|workspaces\.json|vpn\.ovpn|Cookies(?:-journal)?|Login Data(?:-journal)?|Local State)$/.test(path) && path !== '.env.example') failures.push(`${path}: private runtime file`);
  if (/^(?:\.local|test-results|playwright-report|node_modules|dist|src-tauri\/target)\//.test(path) || /\.(?:pfx|p12|pem|key|log)$/i.test(path)) failures.push(`${path}: local artifact or key file`);
  const text = readFileSync(path).toString('utf8');
  if (/[A-Z]:\\(?:Users|PROJECTS)\\|\/Users\/[^/]+\//i.test(text)) failures.push(`${path}: personal machine path`);
}
const emails = new Set(git('log', '--format=%ae%n%ce', 'HEAD').trim().split(/\r?\n/));
for (const email of emails) {
  if (!email.endsWith('@users.noreply.github.com')) failures.push('Git history includes a private author/committer email; use a GitHub no-reply address.');
}
const example = readFileSync('.env.example', 'utf8');
if (/^NORDVPN_SERVICE_(?:USER|PASSWORD)\s*=\s*\S+/m.test(example)) failures.push('.env.example: credential placeholders must be empty');
const pkg = JSON.parse(readFileSync('package.json', 'utf8'));
const tauri = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8'));
const cargo = readFileSync('src-tauri/Cargo.toml', 'utf8').match(/^version = "([^"]+)"/m)?.[1];
if (pkg.version !== tauri.version || pkg.version !== cargo) failures.push('Package, Tauri, and Cargo release versions differ.');
if (process.env.GITHUB_REF_TYPE === 'tag' && process.env.GITHUB_REF_NAME !== `v${pkg.version}`) failures.push('Release tag does not match the app version.');
if (tauri.bundle.resources?.length || tauri.bundle.externalBin?.length) failures.push('Review newly bundled files before publishing.');
if (failures.length) {
  console.error([...new Set(failures)].join('\n'));
  process.exitCode = 1;
} else console.log(`PASS: ${files.length} tracked files, public commit identities, empty credential template, and release versions checked.`);
