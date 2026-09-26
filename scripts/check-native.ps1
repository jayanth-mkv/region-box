$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$logDirectory = Join-Path $projectRoot '.local'
New-Item -ItemType Directory -Force -Path $logDirectory | Out-Null
$buildProcess = Start-Process -FilePath 'cargo.exe' -ArgumentList @('check', '--manifest-path', 'src-tauri/Cargo.toml', '-j', '1') -WorkingDirectory $projectRoot -WindowStyle Hidden -RedirectStandardOutput (Join-Path $logDirectory 'cargo-check.stdout.log') -RedirectStandardError (Join-Path $logDirectory 'cargo-check.stderr.log') -PassThru
$buildProcess.Id
