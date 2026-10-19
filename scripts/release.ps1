<#
.SYNOPSIS
  Builds the Windows installers locally, publishes them as a GitHub Release, then (only if that worked)
  starts the GitHub Actions workflow that adds the macOS and Linux builds to the same release.

.EXAMPLE
  ./scripts/release.ps1                    # release the version in src-tauri/tauri.conf.json
  ./scripts/release.ps1 -NotesFile notes.md
  ./scripts/release.ps1 -WindowsOnly       # publish Windows, don't start the macOS/Linux workflow
  ./scripts/release.ps1 -DryRun            # build and write latest.json, publish nothing

.NOTES
  Needs: gh (logged in with push rights), the updater signing key in ~/.tauri/tessera.key,
  a clean checkout of main that matches origin/main.
#>
param(
  [string]$NotesFile,
  [switch]$WindowsOnly,
  [switch]$DryRun
)
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
$repo = 'JackLuciano/tessera'

function Invoke-Checked([scriptblock]$cmd, [string]$what) {
  & $cmd
  if ($LASTEXITCODE) { throw "$what failed (exit $LASTEXITCODE)" }
}

# 1. Preconditions: released code must be exactly what is on origin/main
foreach ($tool in 'gh', 'git', 'pnpm', 'cargo') {
  if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw "$tool is not installed." }
}
if (-not (Test-Path (Join-Path $HOME '.tauri/tessera.key')) -and -not $env:TAURI_SIGNING_PRIVATE_KEY) {
  throw 'Signing key ~/.tauri/tessera.key not found: a release without it would break auto-update.'
}
$version = (Get-Content src-tauri/tauri.conf.json -Raw | ConvertFrom-Json).version
$tag = "v$version"
if (-not $DryRun) {
  if ((git branch --show-current) -ne 'main') { throw 'Releases are cut from main. Switch to main first.' }
  if (git status --porcelain) { throw 'Working tree is not clean.' }
  Invoke-Checked { git fetch origin main --tags --quiet } 'git fetch'
  if ((git rev-parse HEAD) -ne (git rev-parse origin/main)) { throw 'Local main differs from origin/main. Pull or push first.' }
  gh release view $tag --repo $repo *> $null
  if (-not $LASTEXITCODE) { throw "Release $tag already exists. Bump the version in package.json, Cargo.toml and tauri.conf.json." }
}
Write-Host "Releasing Tessera $tag" -ForegroundColor Cyan

# 2. Build (signs the updater artifacts with the local key)
Invoke-Checked { ./scripts/build.ps1 } 'Build'

$bundle = 'src-tauri/target/release/bundle'
$setup = Get-Item "$bundle/nsis/Tessera_${version}_x64-setup.exe"
$msi = Get-Item "$bundle/msi/Tessera_${version}_x64_en-US.msi"
foreach ($f in $setup, $msi) {
  if (-not (Test-Path "$($f.FullName).sig")) { throw "Missing signature $($f.Name).sig" }
}

# 3. latest.json for tauri-plugin-updater. The macOS/Linux workflow merges its platforms into this file.
$base = "https://github.com/$repo/releases/download/$tag"
$sig = (Get-Content "$($setup.FullName).sig" -Raw).Trim()
$msiSig = (Get-Content "$($msi.FullName).sig" -Raw).Trim()
$notes = if ($NotesFile) { Get-Content $NotesFile -Raw } else { "Tessera $tag" }
$latest = [ordered]@{
  version  = $version
  notes    = $notes
  pub_date = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
  platforms = [ordered]@{
    'windows-x86_64'      = @{ signature = $sig; url = "$base/$($setup.Name)" }
    'windows-x86_64-nsis' = @{ signature = $sig; url = "$base/$($setup.Name)" }
    'windows-x86_64-msi'  = @{ signature = $msiSig; url = "$base/$($msi.Name)" }
  }
}
$latestPath = "$bundle/latest.json"
$latest | ConvertTo-Json -Depth 5 | Set-Content $latestPath -Encoding utf8NoBOM
Write-Host "Wrote $latestPath"

if ($DryRun) { Write-Host 'Dry run: nothing published.' -ForegroundColor Yellow; exit 0 }

# 4. Tag and publish. Notes default to GitHub's generated changelog.
Invoke-Checked { git tag -a $tag -m "Tessera $tag" } 'git tag'
Invoke-Checked { git push origin $tag } 'git push tag'
$assets = @($setup.FullName, "$($setup.FullName).sig", $msi.FullName, "$($msi.FullName).sig", $latestPath)
$noteArgs = if ($NotesFile) { @('--notes-file', $NotesFile) } else { @('--generate-notes') }
Invoke-Checked { gh release create $tag @assets --repo $repo --title "Tessera $tag" --verify-tag @noteArgs } 'gh release create'
Write-Host "Published https://github.com/$repo/releases/tag/$tag" -ForegroundColor Green

# 5. Windows is out: now let Actions add macOS + Linux to the same release
if ($WindowsOnly) {
  Write-Host "Skipped macOS/Linux. Later: gh workflow run release.yml --repo $repo -f tag=$tag"
  exit 0
}
Invoke-Checked { gh workflow run release.yml --repo $repo --ref main -f tag=$tag -f platforms=all } 'Starting the macOS/Linux workflow'
Write-Host "macOS/Linux build started: https://github.com/$repo/actions/workflows/release.yml" -ForegroundColor Green
