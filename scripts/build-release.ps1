$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$logDirectory = Join-Path $projectRoot '.local'
New-Item -ItemType Directory -Force -Path $logDirectory | Out-Null
$env:CARGO_BUILD_JOBS = '1'
$buildProcess = Start-Process -FilePath 'node.exe' -ArgumentList @('node_modules/@tauri-apps/cli/tauri.js', 'build', '--bundles', 'nsis') -WorkingDirectory $projectRoot -WindowStyle Hidden -RedirectStandardOutput (Join-Path $logDirectory 'release.stdout.log') -RedirectStandardError (Join-Path $logDirectory 'release.stderr.log') -PassThru
$buildProcess.Id
