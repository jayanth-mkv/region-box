# Chromium safeguards

The launcher keeps Chromium's own sandbox enabled and reuses the existing
`/config/.config/chromium` profile. It does not suppress outdated-browser warnings.
The browser container retains a seccomp filter and has no SYS_ADMIN capability.

`seccomp.json` is the Chromium-compatible Docker profile from Microsoft's
[Playwright project](https://github.com/microsoft/playwright/blob/main/utils/docker/seccomp_profile.json),
retrieved September 27, 2026. It permits user namespace creation for Chromium's
sandbox. See `LICENSE.playwright` for its Apache 2.0 license and
[the official explanation](https://playwright.dev/docs/docker#crawling-and-scraping).

Managed policies disable password saving because this Linux container has no
OS-backed password vault. Persistent login cookies remain in each workspace's
Docker volume. WebRTC uses the default public interface; the VPN firewall still
controls all outgoing traffic. Camera and microphone forwarding remain disabled.
