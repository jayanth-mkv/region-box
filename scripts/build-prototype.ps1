$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$logDirectory = Join-Path $projectRoot '.local'
New-Item -ItemType Directory -Force -Path $logDirectory | Out-Null
$env:CARGO_BUILD_JOBS = '1'
$buildProcess = Start-Process -FilePath 'node.exe' -ArgumentList @('node_modules/@tauri-apps/cli/tauri.js', 'build', '--debug', '--no-bundle') -WorkingDirectory $projectRoot -WindowStyle Hidden -RedirectStandardOutput (Join-Path $logDirectory 'prototype.stdout.log') -RedirectStandardError (Join-Path $logDirectory 'prototype.stderr.log') -PassThru
$buildProcess.Id
