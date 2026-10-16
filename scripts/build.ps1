<#
.SYNOPSIS
  Local Windows build of Tessera.

.EXAMPLE
  ./scripts/build.ps1            # release installers (NSIS .exe + .msi)
  ./scripts/build.ps1 -Dev       # run in development mode with hot reload
  ./scripts/build.ps1 -Portable  # release build, copy the bare .exe to dist/
#>
param(
  [switch]$Dev,
  [switch]$Portable
)
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')

foreach ($tool in 'pnpm', 'cargo') {
  if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
    throw "$tool is not installed. See README > Building locally."
  }
}

pnpm install --frozen-lockfile
if ($LASTEXITCODE) { exit $LASTEXITCODE }

if ($Dev) {
  pnpm tauri dev
  exit $LASTEXITCODE
}

# Updater artifacts must be signed. Use the key in ~/.tauri if present; otherwise build without them.
$key = Join-Path $HOME '.tauri/tessera.key'
if (-not $env:TAURI_SIGNING_PRIVATE_KEY -and (Test-Path $key)) {
  $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content $key -Raw
  $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ''
}
if ($env:TAURI_SIGNING_PRIVATE_KEY) {
  pnpm tauri build
} else {
  Write-Warning 'No signing key found: building without auto-update artifacts.'
  pnpm tauri build --config '{\"bundle\":{\"createUpdaterArtifacts\":false}}'
}
if ($LASTEXITCODE) { exit $LASTEXITCODE }

$bundle = 'src-tauri/target/release/bundle'
Write-Host "`nInstallers:" -ForegroundColor Green
Get-ChildItem "$bundle/nsis/*.exe", "$bundle/msi/*.msi" -ErrorAction SilentlyContinue | ForEach-Object { Write-Host "  $($_.FullName)" }

if ($Portable) {
  New-Item -ItemType Directory -Force dist | Out-Null
  Copy-Item src-tauri/target/release/tessera.exe dist/Tessera-portable.exe -Force
  Write-Host "  $(Resolve-Path dist/Tessera-portable.exe)"
}
