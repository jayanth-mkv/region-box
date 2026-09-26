// Opt-in native UI check. Reads existing credentials without modifying them.
import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

if (!process.env.REGIONBOX_ENV_FILE) throw new Error('Set REGIONBOX_ENV_FILE to the existing service credentials file.');
const root = resolve('.local', `credential-ui-${Date.now()}`);
await mkdir(root, { recursive: true });
await mkdir('test-results', { recursive: true });
const app = spawn(resolve(process.env.REGIONBOX_TEST_EXE ?? 'src-tauri/target/release/regionbox.exe'), [], {
  windowsHide: true, stdio: 'ignore', env: { ...process.env, REGIONBOX_DATA_DIR: root,
    WEBVIEW2_USER_DATA_FOLDER: resolve(root, 'webview'), WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=9341' },
});
let browser, page, launchError;
app.on('error', error => { launchError = error; });
try {
  for (let i = 0; i < 60; i++) {
    if (launchError) throw launchError;
    if (app.exitCode !== null) throw new Error('The test app closed before its window was ready.');
    try { browser = await chromium.connectOverCDP('http://127.0.0.1:9341', { timeout: 1000 }); break; }
    catch { await delay(1000); }
  }
  if (!browser) throw new Error('Could not attach to the credential test window.');
  page = browser.contexts().flatMap(context => context.pages()).find(page => !page.url().startsWith('devtools:'));
  if (!page) throw new Error('The test window was not found.');
  await expect(page.getByRole('button', { name: 'Check saved credentials', exact: true })).toBeVisible({ timeout: 60000 });
  await expect(page.getByText('Open your first browser', { exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'Check saved credentials', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Checking credentials…', exact: true })).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Set up later' })).toBeDisabled();
  console.log('Native app is checking the saved credentials against NordVPN.');
  await expect(page.getByText('NordVPN credentials verified.', { exact: true })).toBeVisible({ timeout: 360000 });
  await expect(page.getByLabel('First workspace').locator('option')).toHaveCount(10);
  await page.screenshot({ path: 'test-results/credentials-verified-native.png' });
  console.log('PASS: Native credentials step verified the saved login before showing all ten country choices.');
} finally {
  if (page) await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('quit_app', { stop: false })).catch(() => {});
  for (let i = 0; i < 20 && app.exitCode === null; i++) await delay(250);
  if (app.exitCode === null) app.kill();
  if (browser) await browser.close().catch(() => {});
}
