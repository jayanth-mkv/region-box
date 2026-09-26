# Verification

## Onboarding and installer results — 27 September 2026

- Passed: TypeScript/Vite build, eight Rust checks, and onboarding UI fixtures.
- Passed: native desktop onboarding and existing-prerequisite reuse.
- Passed: the NSIS installer installed RegionBox for the current Windows user and created its Start menu shortcut.
- Passed: the installed release executable completed the native desktop test with no credentials-path override. It used its own app data directory rather than the development project's `.env`.
- Built: `src-tauri/target/release/bundle/nsis/RegionBox_0.1.0_x64-setup.exe` (about 3 MB, unsigned).
- Pending: fresh-PC WSL/Docker installation and the live NordVPN checks below.

The initial build with link-time optimization exhausted available RAM and caused a Docker readiness timeout. The native check passed after the build pressure was removed. Release packaging now disables link-time optimization; the installer was produced from a release executable with that optimization disabled and tested after installation.

## Initial setup results — 27 September 2026

- Passed: TypeScript/Vite production build and standalone Windows prototype build.
- Passed: all six Rust checks, including Docker Compose validation for all three countries.
- Passed: real desktop setup, credential validation, workspace creation, unique viewer tokens, and settings persistence across restart.
- Passed: real Chromium video, mouse/keyboard navigation, access refresh, and refusal of connections without a viewer token.
- Pending: live NordVPN routing, concurrent US/DE/UK connections, browser profile persistence, DNS/IPv6/WebRTC egress, and VPN-loss behavior. These require the user's service credentials.

The developer prototype is `src-tauri/target/debug/regionbox.exe`. Build the shareable Windows installer with `npm run desktop:build`; its output is under `src-tauri/target/release/bundle/nsis/`.

## Onboarding checks

```powershell
rtk npm run test:onboarding
rtk npm run test:desktop
```

The onboarding UI fixtures cover a missing installation, measured download progress, administrator guidance, a restart pause, automatic continuation from Docker into image downloads, an interrupted download and retry, credential entry, country selection, successful completion, and a narrow window. They never execute Windows installers or connect to a VPN.

The native desktop check uses the actual command bridge on this PC. It checks prerequisite detection, reusing the installed WSL/Docker/images, invalid credentials, deferring and reopening setup, persistence after restart, and absence of credential-file instructions in Settings. It refuses to run setup actions unless all existing prerequisites are already ready. To test a release executable, set `REGIONBOX_TEST_EXE` to its full path and `REGIONBOX_TEST_PORTABLE=1` before running the desktop test. Portable mode omits the credentials-path override and checks that the installed app selects its own data directory.

### Fresh Windows verification still needed

Use a separate supported Windows PC or VM without Docker/WSL. Verify the RegionBox installer and WebView2 bootstrap, WSL approval and restart/resume, the official Docker download and signature, Docker's terms screen, image download/retry, and a real NordVPN connection. Also check canceled administrator approval and disabled virtualization. This development PC already has WSL and Docker, so the UI fixtures and reuse checks do not establish that the fresh installation path has passed.

## Build and core behavior

```powershell
rtk npm run build
rtk npm run test:core
```

The core checks cover workspace persistence, unique viewer ports, credential input validation, IP response parsing, and the generated Compose isolation rules.

## Native desktop setup

```powershell
rtk npm run desktop:prototype
rtk npm run test:desktop
```

This launches the actual Windows executable, attaches Playwright to WebView2, and tests the real Rust command bridge. It uses a separate app data directory in `.local/desktop-test-*` and test-only credentials. It does not start a VPN connection or read the real `.env`.

Checks: the three initial countries, disabled start before setup, rejection of account email credentials, saving credentials, clearing the password input, creating an additional German workspace, logs, Stop all, and settings persistence across an app restart. The initial setup screenshot is written to `test-results/desktop-setup.png`.

The test enables a local WebView2 debugging port only for the test process. Normal app launches do not enable it.

## Chromium viewer without VPN credentials

```powershell
rtk npm run test:viewer
```

This starts an isolated, temporary browser container on port 32990 and embeds its real viewer in a local test page on port 32991. It checks authenticated streaming, the secure browser context, mouse/keyboard navigation after refreshing access, and rejection of a WebSocket connection without a token. Microsoft Edge is used as the test client. The test removes its own container afterward and leaves a screenshot at `test-results/chromium-viewer.png`.

This test container uses the normal Docker network, contains no personal data, and does not represent a VPN-connected workspace. The initial streaming request can retry while the upstream image starts its WebSocket service; the test requires successful recovery and working navigation.

## Live NordVPN checks — requires service credentials

Once the user supplies valid credentials locally:

1. Start US and Germany, then UK. Keep all three running concurrently.
2. In each browser visit Cloudflare's `/cdn-cgi/trace` endpoint and compare `ip` and `loc` with **Check IP**. Check each country independently and note any geolocation disagreement.
3. Verify DNS uses Gluetun's resolver and test DNS/IPv6/WebRTC egress for direct host-IP leakage.
4. Set different cookies/local-storage values on the same test origin in each browser. Confirm they remain separate.
5. Stop and restart one workspace, then reopen RegionBox. Confirm its saved browser data persists and the other workspaces keep running.
6. Interrupt only one workspace's VPN tunnel while its browser continues requesting an external endpoint. Confirm requests fail during the interruption and never use the host's normal public IP. Confirm the other workspaces stay online.
7. Restore the tunnel, verify recovery, and recheck the reported country and IP.
8. Check the embedded browser's mouse, keyboard, navigation, clipboard, and viewer reconnection.

These are separate from a successful compilation or UI setup test. Do not mark the VPN behavior verified until these checks have actually run.
