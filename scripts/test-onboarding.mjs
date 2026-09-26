// UI fixtures exercise installer/restart/error states without changing Windows.
// The real native setup commands are checked separately by test-desktop.mjs.
import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { mkdir } from 'node:fs/promises';
import { setTimeout as delay } from 'node:timers/promises';

const server = spawn(process.execPath, ['node_modules/vite/bin/vite.js', 'preview', '--host', '127.0.0.1', '--port', '1422', '--strictPort'], { windowsHide: true, stdio: 'ignore' });
let browser;
try {
  for (let attempt = 0; attempt < 30; attempt++) {
    try { if ((await fetch('http://127.0.0.1:1422/')).ok) break; } catch { /* Starting. */ }
    await delay(500);
  }
  browser = await chromium.launch({ channel: 'msedge', headless: true });
  const context = await browser.newContext({ viewport: { width: 1280, height: 820 } });
  await context.addInitScript(() => {
    window.isTauri = true;
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
    const state = {
      setup: { checked: true, supported: true, virtualization: true, windowsReady: false, dockerInstalled: false, dockerReady: false, vpnImageReady: false, browserImageReady: false, dismissed: false, restartRequired: false, busy: false, phase: '', detail: '', error: null, downloaded: 0, total: null },
      data: { dockerReady: false, dockerMessage: 'Start Docker Desktop', credentialsReady: false, credentialsVerified: false, credentialsPath: 'TEST-ONLY/.env', dataPath: 'TEST-ONLY', workspaces: [
        { id: 'us', name: 'United States', country: 'US', port: 32101, state: 'stopped', detail: 'Browser data is saved', browserUrl: null, network: null },
        { id: 'de', name: 'Germany', country: 'DE', port: 32102, state: 'stopped', detail: 'Browser data is saved', browserUrl: null, network: null },
        { id: 'gb', name: 'United Kingdom', country: 'GB', port: 32103, state: 'stopped', detail: 'Browser data is saved', browserUrl: null, network: null },
      ] },
      calls: [], resolve: null,
    };
    window.__setupFixture = state;
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
      transformCallback: () => 1,
      unregisterCallback: () => {},
      invoke: async (command, args) => {
        state.calls.push(command);
        if (command === 'snapshot') return structuredClone(state.data);
        if (command === 'setup_status') return structuredClone(state.setup);
        if (command.startsWith('plugin:event|')) return 1;
        if (command === 'run_setup') {
          state.setup.busy = true;
          state.setup.phase = args.action === 'images' ? 'downloading_browser' : 'installing_wsl';
          state.setup.detail = args.action === 'images' ? 'Downloading Chromium files (2 of 2).' : 'Preparing Windows support. Approve the Windows administrator prompt if it appears.';
          await new Promise((resolve, reject) => { state.resolve = (error) => { state.setup.busy = false; if (error) { state.setup.error = error; reject(error); } else resolve(); }; });
          return;
        }
        if (command === 'save_credentials' || command === 'check_saved_credentials') {
          state.data.credentialsVerified = false;
          await new Promise((resolve, reject) => { state.resolve = error => {
            if (error) reject(error);
            else { state.data.credentialsReady = true; state.data.credentialsVerified = true; resolve(); }
          }; });
          return;
        }
        if (command === 'dismiss_setup') { state.setup.dismissed = true; return; }
        if (command === 'create_workspace') {
          const workspace = { id: 'new-country', name: args.name, country: args.country, port: 32104, state: 'stopped', detail: '', browserUrl: null, network: null };
          state.data.workspaces.push(workspace);
          return structuredClone(workspace);
        }
        if (command === 'start_workspace') {
          const workspace = state.data.workspaces.find(workspace => workspace.id === args.id);
          workspace.state = 'starting'; workspace.detail = 'Connecting NordVPN and starting Chromium';
          await new Promise(resolve => { state.resolve = () => { workspace.state = 'running'; workspace.network = { country: workspace.country, ip: '203.0.113.10', checkedAt: 1 }; resolve(); }; });
          return;
        }
        throw new Error(`Unexpected fixture command: ${command}`);
      },
    };
  });
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(String(error)));
  await page.goto('http://127.0.0.1:1422/');
  await expect(page.getByText('Prepare Windows support', { exact: true })).toBeVisible();
  await mkdir('test-results', { recursive: true });
  await page.screenshot({ path: 'test-results/onboarding-welcome.png' });
  await page.getByRole('button', { name: 'Set up this PC' }).click();
  await expect(page.getByRole('button', { name: 'Set up later' })).toBeDisabled();
  await expect(page.getByText(/Preparing Windows support\. Approve/)).toBeVisible({ timeout: 10000 });
  await page.evaluate(() => { const fixture = window.__setupFixture; fixture.setup.restartRequired = true; fixture.resolve(); });
  await expect(page.getByText('Restart Windows to continue', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Set up later' })).toBeEnabled();
  await page.screenshot({ path: 'test-results/onboarding-restart.png' });
  await page.evaluate(() => { Object.assign(window.__setupFixture.setup, { restartRequired: false, windowsReady: true }); });
  await page.getByRole('button', { name: 'Recheck setup' }).click();
  await expect(page.getByText('Get Docker ready', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Set up this PC' }).click();
  await page.evaluate(() => { Object.assign(window.__setupFixture.setup, { phase: 'downloading_docker', detail: 'Downloading Docker Desktop from Docker', total: 100, downloaded: 40 }); });
  await expect(page.getByText('40% downloaded')).toBeVisible({ timeout: 10000 });
  await expect(page.getByRole('progressbar', { name: 'Docker download' })).toHaveAttribute('aria-valuenow', '40');
  await page.evaluate(() => { const fixture = window.__setupFixture; Object.assign(fixture.setup, { windowsReady: true, dockerReady: true, dockerInstalled: true }); fixture.data.dockerReady = true; fixture.resolve(); });
  await expect(page.getByText('Downloading Chromium files (2 of 2).', { exact: true })).toBeVisible({ timeout: 10000 });
  await page.evaluate(() => { window.__setupFixture.resolve('The download was interrupted. Retry setup.'); });
  await expect(page.getByText('The download was interrupted. Retry setup.')).toBeVisible();
  await page.getByRole('button', { name: 'Download browser files', exact: true }).click();
  await page.evaluate(() => { const fixture = window.__setupFixture; Object.assign(fixture.setup, { error: null, vpnImageReady: true, browserImageReady: true }); fixture.resolve(); });
  await expect(page.getByText('Connect your NordVPN account', { exact: true })).toBeVisible();
  await page.getByLabel('Service username').fill('test-only-service-user');
  await page.getByLabel('Service password').fill('test-only-password');
  await page.getByRole('button', { name: 'Check and continue' }).click();
  await expect(page.getByRole('button', { name: 'Checking credentials…' })).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Set up later' })).toBeDisabled();
  await expect(page.getByText('Open your first browser', { exact: true })).toHaveCount(0);
  await page.evaluate(() => window.__setupFixture.resolve('NordVPN rejected this login. Check both service credentials and try again.'));
  await expect(page.getByText(/NordVPN rejected this login/)).toBeVisible();
  await expect(page.getByLabel('Service username')).toHaveValue('test-only-service-user');
  await expect(page.getByText('Connect your NordVPN account', { exact: true })).toBeVisible();
  await page.screenshot({ path: 'test-results/onboarding-credential-error.png' });
  await page.getByRole('button', { name: 'Check and continue' }).click();
  await page.evaluate(() => window.__setupFixture.resolve());
  await expect(page.getByText('NordVPN credentials verified.', { exact: true })).toBeVisible();
  // On reopening the app, saved values must be checked before proceeding.
  await page.evaluate(() => { window.__setupFixture.data.credentialsVerified = false; });
  await page.getByRole('button', { name: 'Recheck setup' }).click();
  await expect(page.getByRole('button', { name: 'Check saved credentials' })).toBeVisible();
  await page.getByRole('button', { name: 'Check saved credentials' }).click();
  await page.evaluate(() => window.__setupFixture.resolve('Could not connect to a NordVPN server. Your credentials could not be verified.'));
  await expect(page.getByText(/Your credentials could not be verified/)).toBeVisible();
  await expect(page.getByText('Open your first browser', { exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'Check saved credentials' }).click();
  await page.evaluate(() => window.__setupFixture.resolve());
  await expect(page.getByText('Open your first browser', { exact: true })).toBeVisible();
  await expect(page.getByText(/\.env/)).toHaveCount(0);
  await expect(page.getByLabel('First workspace').locator('option')).toHaveCount(10);
  await page.getByLabel('First workspace').selectOption('new:IN');
  await page.getByRole('button', { name: 'Start first browser' }).click();
  await expect(page.getByRole('button', { name: 'Set up later' })).toBeDisabled();
  await page.evaluate(() => window.__setupFixture.resolve());
  await expect(page.getByText('Your first browser is ready', { exact: true })).toBeVisible();
  await page.screenshot({ path: 'test-results/onboarding-complete.png' });
  await page.setViewportSize({ width: 760, height: 560 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth)).toBe(false);
  await page.getByRole('button', { name: 'Open browser', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'India', exact: true })).toBeVisible();
  await page.evaluate(() => {
    window.__setupFixture.data.workspaces.find(w => w.id === 'new-country').browserUrl = 'about:blank';
  });
  await page.getByRole('button', { name: 'Refresh status' }).click();
  await expect(page.getByTitle('India browser')).toBeVisible();
  for (const size of [{ width: 760, height: 560 }, { width: 1280, height: 820 }, { width: 1920, height: 1080 }]) {
    await page.setViewportSize(size);
    const layout = await page.evaluate(() => {
      const frame = document.querySelector('iframe').getBoundingClientRect();
      const footer = document.querySelector('footer').getBoundingClientRect();
      return { overflow: document.documentElement.scrollWidth > innerWidth || document.documentElement.scrollHeight > innerHeight,
        frameFits: frame.bottom <= footer.top + 1 && frame.right <= innerWidth && frame.height > 100 };
    });
    expect(layout).toEqual({ overflow: false, frameFits: true });
  }
  await page.screenshot({ path: 'test-results/browser-layout.png' });
  expect(errors).toEqual([]);
  console.log('PASS: onboarding UI fixtures cover installation progress, restart pause, automatic next step, download retry, credentials, country selection, completion, and narrow layout.');
  console.log('These fixtures do not execute Windows/Docker installers or establish a VPN connection.');
} finally {
  if (browser) await browser.close();
  server.kill();
}
