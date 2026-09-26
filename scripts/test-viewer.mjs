// Test only the real Chromium image's local viewer, without using a VPN account.
// No RegionBox workspace or personal browser data is used by this check.
import { chromium, expect } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { createServer } from 'node:http';
import { mkdir } from 'node:fs/promises';
import { setTimeout as delay } from 'node:timers/promises';
import { randomBytes } from 'node:crypto';
import { resolve } from 'node:path';

const image = 'lscr.io/linuxserver/chromium@sha256:cf6200ccdcb224feaf5d3bde4ce45b3783c926a7496c98059cae1e0db78e5b2f';
const container = `regionbox-viewer-test-${Date.now()}`;
const profile = `${container}-profile`;
const viewerToken = randomBytes(32).toString('hex');
const docker = (...args) => execFileSync('docker', ['--context', 'desktop-linux', ...args], { encoding: 'utf8', windowsHide: true, timeout: 90000 });
let browser;
let server;
let page;
let created = false;
let profileCreated = false;
const startContainer = (volume = profile) => {
  docker('run', '--detach', '--name', container, '--shm-size', '1g', '--memory', '1500m',
    '--security-opt', `seccomp=${resolve('src-tauri/browser/seccomp.json')}`,
    '--volume', `${resolve('src-tauri/browser/chromium-browser')}:/usr/bin/wrapped-chromium:ro`,
    '--volume', `${resolve('src-tauri/browser/policies.json')}:/etc/chromium/policies/managed/regionbox.json:ro`,
    '--volume', `${resolve('scripts/browser-probe.py')}:/tmp/browser-probe.py:ro`,
    '--volume', volume ? `${volume}:/config` : '/config',
    '--publish', '127.0.0.1:32990:3000', '--env', 'CHROME_CLI=--no-first-run --remote-debugging-port=9222 --remote-allow-origins=http://localhost --user-data-dir=/config/viewer-test-profile about:blank',
    '--env', 'SELKIES_AUDIO_ENABLED=true', '--env', 'SELKIES_COMMAND_ENABLED=false|locked', '--env', 'SELKIES_FRAMERATE=10',
    '--env', `SELKIES_MASTER_TOKEN=${viewerToken}`,
    '--env', 'SELKIES_ENABLE_SHARING=false|locked', '--env', 'SELKIES_ALLOWED_ORIGINS=http://127.0.0.1:32990', image);
  created = true;
};
const probe = action => docker('exec', container, 'python3', '/tmp/browser-probe.py', action).trim();
const waitForBrowser = async () => {
  await expect.poll(() => {
    try { return JSON.parse(docker('exec', container, 'curl', '-fsS', 'http://127.0.0.1:9222/json/list')).some(t => t.type === 'page'); }
    catch { return false; }
  }, { timeout: 60000 }).toBe(true);
};
try {
  docker('volume', 'create', profile); profileCreated = true;
  startContainer();
  let ready = false;
  for (let i = 0; i < 90; i++) {
    try { const response = await fetch('http://127.0.0.1:32990/api/health', { signal: AbortSignal.timeout(1000) }); if (response.ok) { ready = true; break; } } catch { /* Starting. */ }
    await delay(1000);
  }
  if (!ready) throw new Error('Chromium viewer did not become ready.');
  const permissions = JSON.stringify({ [viewerToken]: { role: 'controller', mk_control: true } });
  const authorize = () => fetch('http://127.0.0.1:32990/api/tokens', {
    method: 'POST', headers: { Authorization: `Bearer ${viewerToken}`, 'Content-Type': 'application/json' },
    body: permissions, signal: AbortSignal.timeout(2000),
  });
  await expect.poll(async () => {
    try {
      const response = await authorize();
      return response.status;
    } catch { return 0; }
  }, { timeout: 30000 }).toBe(200);
  server = createServer((request, response) => {
    response.writeHead(200, { 'Content-Type': 'text/html' });
    response.end(`<!doctype html><title>RegionBox viewer test</title><style>html,body{margin:0;height:100%}iframe{border:0;width:100%;height:100%}</style><iframe title="Chromium" src="http://127.0.0.1:32990/?token=${viewerToken}" allow="clipboard-read;clipboard-write;fullscreen"></iframe>`);
  });
  await new Promise(resolve => server.listen(32991, '127.0.0.1', resolve));
  browser = await chromium.launch({ channel: 'msedge', headless: true });
  page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  let receivedFrames = 0;
  const websocketErrors = [];
  page.on('websocket', socket => {
    socket.on('framereceived', () => { receivedFrames++; });
    socket.on('socketerror', error => websocketErrors.push({ url: socket.url(), error, receivedFramesAtError: receivedFrames }));
  });
  await page.goto('http://127.0.0.1:32991/');
  await expect.poll(() => receivedFrames, { timeout: 60000 }).toBeGreaterThan(3);
  // RegionBox refreshes this table after container/app restarts. Existing clients must stay usable.
  expect((await authorize()).status).toBe(200);
  const frame = page.frames().find(frame => frame.url().startsWith('http://127.0.0.1:32990/'));
  expect(frame).toBeTruthy();
  expect(await frame.evaluate(() => window.isSecureContext)).toBe(true);
  expect(await frame.evaluate(() => typeof VideoDecoder)).toBe('function');
  await delay(5000);
  await page.mouse.click(300, 65);
  await page.keyboard.press('Control+a');
  await page.keyboard.type('data:text/html,<title>RegionBox viewer checked</title><h1>RegionBox viewer checked</h1>', { delay: 20 });
  await page.keyboard.press('Enter');
  await expect.poll(() => {
    const targets = JSON.parse(docker('exec', container, 'curl', '-fsS', 'http://127.0.0.1:9222/json/list'));
    return targets.some(target => target.title === 'RegionBox viewer checked');
  }, { timeout: 20000 }).toBe(true);
  await mkdir('test-results', { recursive: true });
  await page.screenshot({ path: 'test-results/chromium-viewer.png' });
  // The image may briefly serve its static page before the streaming route is
  // ready. Its client retries. Require recovery and reject errors after streaming.
  expect(websocketErrors.filter(error => error.receivedFramesAtError > 0)).toEqual([]);
  const unauthenticated = await page.evaluate(() => new Promise(resolve => {
    const socket = new WebSocket('ws://127.0.0.1:32990/api/websockets');
    const timer = setTimeout(() => { socket.close(); resolve('timeout'); }, 5000);
    socket.onopen = () => { clearTimeout(timer); socket.close(); resolve('accepted'); };
    socket.onerror = () => { clearTimeout(timer); resolve('rejected'); };
  }));
  expect(unauthenticated).toBe('rejected');
  console.log('PASS: real Chromium viewer embeds in a secure loopback page, streams video, and accepts mouse/keyboard navigation.');
  console.log('PASS: the viewer rejects a WebSocket connection without its private token.');
  const sandbox = probe('sandbox');
  expect(sandbox).toMatch(/Layer 1 Sandbox\s+Namespace/);
  expect(sandbox).toMatch(/PID namespaces\s+Yes/);
  expect(sandbox).toMatch(/Network namespaces\s+Yes/);
  expect(sandbox).toMatch(/Seccomp-BPF sandbox\s+Yes/);
  console.log('PASS: Chromium reports namespace and seccomp-BPF sandboxes enabled.');
  probe('seed');
  await delay(2000);
  docker('rm', '--force', container); created = false;
  startContainer();
  await waitForBrowser();
  expect(JSON.parse(probe('cookies'))).toEqual([{ name: 'regionbox_test', value: 'persisted' }]);
  console.log('PASS: a persistent login cookie survives browser container recreation.');
  docker('rm', '--force', container); created = false;
  // An anonymous, fresh profile must never see the first workspace's cookie.
  startContainer(null);
  await waitForBrowser();
  expect(JSON.parse(probe('cookies'))).toEqual([]);
  console.log('PASS: a fresh browser profile cannot see another profile\'s cookie.');
  if (websocketErrors.length) console.log(`Viewer recovered from ${websocketErrors.length} initial connection retry.`);
  console.log('This viewer-only test does not verify NordVPN routing.');
} catch (error) {
  if (page) {
    await mkdir('test-results', { recursive: true });
    await page.screenshot({ path: 'test-results/chromium-viewer-failure.png' });
    for (const frame of page.frames()) console.error((await frame.locator('body').innerText()).slice(0, 2000));
  }
  if (created) console.error(docker('logs', '--tail', '35', container));
  throw error;
} finally {
  if (browser) await browser.close();
  if (server) await new Promise(resolve => server.close(resolve));
  if (created) docker('rm', '--force', '--volumes', container);
  if (profileCreated) docker('volume', 'rm', profile);
}
