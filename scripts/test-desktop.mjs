import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

const root = resolve('.local', `desktop-test-${Date.now()}`);
const executable = resolve(process.env.REGIONBOX_TEST_EXE ?? 'src-tauri/target/debug/regionbox.exe');
await mkdir(root, { recursive: true });
await writeFile(resolve(root, '.env'), 'NORDVPN_SERVICE_USER=\nNORDVPN_SERVICE_PASSWORD=\n');
await mkdir('test-results', { recursive: true });

async function launch() {
  const app = spawn(executable, [], {
    windowsHide: true,
    env: { ...process.env, REGIONBOX_DATA_DIR: root, REGIONBOX_ENV_FILE: resolve(root, '.env'),
      WEBVIEW2_USER_DATA_FOLDER: resolve(root, 'webview'),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=9339' },
    stdio: 'ignore',
  });
  let launchError;
  app.on('error', error => { launchError = error; });
  let browser;
  try {
  for (let attempt = 0; attempt < 90; attempt++) {
    if (launchError) throw launchError;
    if (app.exitCode !== null) throw new Error(`RegionBox exited with code ${app.exitCode}`);
    try { browser = await chromium.connectOverCDP('http://127.0.0.1:9339', { timeout: 1000 }); break; }
    catch { await delay(1000); }
  }
  if (!browser) throw new Error('Could not attach to the RegionBox WebView2 window.');
  let page;
  for (let attempt = 0; attempt < 30; attempt++) {
    page = browser.contexts().flatMap(c => c.pages()).find(p => !p.url().startsWith('devtools:'));
    if (page) break;
    await delay(500);
  }
  if (!page) throw new Error('RegionBox did not open a window.');
  const errors = [];
  page.on('pageerror', error => errors.push(String(error)));
  await expect(page.getByRole('heading', { name: 'Workspaces', exact: true }).or(page.getByRole('heading', { name: 'Set up your regional browsers' }))).toBeVisible({ timeout: 60000 });
  if (await page.getByRole('heading', { name: 'Set up your regional browsers' }).isVisible()) {
    await expect(page.getByRole('navigation', { name: 'Setup progress' })).toBeVisible();
    await expect(page.getByText('Connect your NordVPN account', { exact: true })).toBeVisible();
    await page.screenshot({ path: 'test-results/onboarding-native.png' });
    // This machine already has prerequisites. Confirm the real setup commands reuse them.
    const checks = await page.evaluate(async () => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      const before = await invoke('setup_status');
      if (!before.windowsReady || !before.dockerReady || !before.vpnImageReady || !before.browserImageReady) throw new Error('Native test requires the existing Docker/image installation.');
      await invoke('run_setup', { action: 'prepare' });
      await invoke('run_setup', { action: 'images' });
      return invoke('setup_status');
    });
    expect(checks.busy).toBe(false);
    expect(checks.error).toBeNull();
    await page.getByLabel('Service username').fill('account@example.com');
    await page.getByLabel('Service password').fill('test-only-password');
    await page.getByRole('button', { name: 'Save and continue' }).click();
    await expect(page.getByText("Use NordVPN's service username, rather than your email address.")).toBeVisible();
    await page.getByRole('button', { name: 'Set up later' }).click();
  }
  await expect(page.getByRole('heading', { name: 'Workspaces', exact: true })).toBeVisible();
  return { app, browser, page, errors };
  } catch (error) {
    if (browser) await browser.close().catch(() => {});
    if (app.exitCode === null) app.kill();
    throw error;
  }
}

async function close(session) {
  try { await session.page.evaluate(() => window.__TAURI_INTERNALS__.invoke('quit_app', { stop: false })); } catch { /* Window exits during the IPC request. */ }
  for (let i = 0; i < 20 && session.app.exitCode === null; i++) await delay(250);
  if (session.app.exitCode === null) session.app.kill();
  await session.browser.close().catch(() => {});
}

let session;
try {
  session = await launch();
  const { page } = session;
  const navigation = page.getByRole('navigation', { name: 'Workspaces' });
  for (const country of ['United States', 'Germany', 'United Kingdom']) await expect(navigation.getByRole('button', { name: new RegExp(country) })).toBeVisible({ timeout: 30000 });
  await expect(page.getByRole('button', { name: 'Start workspace', exact: true })).toBeDisabled();
  await expect(page.getByRole('heading', { name: 'Connect NordVPN to get started' })).toBeVisible();
  await page.screenshot({ path: 'test-results/desktop-setup.png' });

  await page.getByRole('button', { name: 'Set up NordVPN' }).click();
  await expect(page.getByLabel('Service password')).toHaveAttribute('type', 'password');
  await page.getByLabel('Service username').fill('account@example.com');
  await page.getByLabel('Service password').fill('test-only-password');
  await page.getByRole('button', { name: 'Save credentials' }).click();
  await expect(page.getByText("Use NordVPN's service username, rather than your email address.")).toBeVisible();
  await page.getByRole('button', { name: 'Dismiss', exact: true }).click();
  await page.getByLabel('Service username').fill('test-only-service-user');
  await page.getByRole('button', { name: 'Save credentials' }).click();
  await expect(page.getByText('Saved. You can now start a workspace.')).toBeVisible();
  await expect(page.getByLabel('Service password')).toHaveValue('');
  await expect(page.getByText(/You can also edit the local credentials file/)).toHaveCount(0);
  await expect(page.getByText(/\.env/)).toHaveCount(0);
  await page.getByRole('button', { name: 'Open setup', exact: true }).click();
  await expect(page.getByText('Open your first browser', { exact: true })).toBeVisible({ timeout: 30000 });
  await page.getByLabel('First workspace').selectOption('gb');
  await page.getByRole('button', { name: 'Set up later' }).click();

  await page.getByRole('button', { name: 'Create workspace', exact: true }).click();
  await page.getByLabel('Name', { exact: true }).fill('Second Germany');
  await page.getByLabel('Country', { exact: true }).selectOption('DE');
  await page.getByRole('button', { name: 'Create workspace', exact: true }).last().click();
  await expect(page.getByRole('heading', { name: 'Second Germany', exact: true })).toBeVisible();
  await expect(navigation.getByRole('button', { name: /Second Germany/ })).toBeVisible();
  await page.getByRole('button', { name: 'View logs' }).click();
  await expect(page.getByText('This workspace has not been started yet.')).toBeVisible();
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth > innerWidth);
  expect(overflow).toBe(false);
  expect(session.errors).toEqual([]);
  await close(session); session = null;

  session = await launch();
  await expect(session.page.getByRole('heading', { name: 'Set up your regional browsers' })).toHaveCount(0);
  await expect(session.page.getByRole('navigation', { name: 'Workspaces' }).getByRole('button', { name: /Second Germany/ })).toBeVisible({ timeout: 30000 });
  const config = JSON.parse(await readFile(resolve(root, 'workspaces.json'), 'utf8'));
  expect(config.workspaces).toHaveLength(4);
  expect(new Set(config.workspaces.map(w => w.port)).size).toBe(4);
  expect(new Set(config.workspaces.map(w => w.viewerToken)).size).toBe(4);
  for (const workspace of config.workspaces) expect(workspace.viewerToken).toMatch(/^[a-f0-9]{64}$/);
  expect(config.workspaces.find(w => w.name === 'Second Germany').country).toBe('DE');
  await session.page.getByRole('button', { name: 'Stop all', exact: true }).click();
  await expect(session.page.getByRole('button', { name: 'Stop all', exact: true })).toBeEnabled({ timeout: 30000 });
  expect(session.errors).toEqual([]);
  console.log('PASS: native desktop setup, credential validation, workspace creation, settings persistence, logs, and Stop all.');
  console.log('Live NordVPN country routing and tunnel-loss checks were not run.');
} finally {
  if (session) await close(session);
}
