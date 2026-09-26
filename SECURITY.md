# Security and private data

Do not include account credentials, session cookies, browser profiles, logs, or
screenshots of signed-in accounts in public issues or pull requests. Use GitHub's
private vulnerability reporting when available under **Security → Report a
vulnerability**.

Builds need no NordVPN credentials. Each recipient supplies their own service
credentials in the installed app. Credentials are stored locally as plain text;
browser cookies and downloads live in local Docker volumes. Protect access to
your Windows account and Docker Desktop. Do not share your app data directory.

CI checks tracked files and reachable Git history before building. It rejects
private runtime files and personal machine paths, scans for secrets with a
checksum-verified Gitleaks release, and uploads only the Windows installer and
its checksum. These automated checks cannot prove that every possible secret or
piece of personal information is absent; review changes before committing.

Use your GitHub **no-reply** email for commit author and committer metadata. No
real service credentials should ever be added as GitHub Actions secrets for
ordinary builds. Live VPN tests run locally with disposable browser profiles.

The browser image is pinned and must be refreshed for future security updates.
See [verification and current limits](docs/testing.md) before relying on the app
for account sessions. The Windows installer is currently unsigned.
