# Package the Flutter Windows app (zip + Inno setup.exe + MSIX).
# Thin wrapper around scripts/flutter-dist.sh for PowerShell / cmd.
#
#   $env:DIST = 'C:\Users\...\muninn-flutter-windows-dist'
#   $env:CARGO_TARGET_DIR = 'C:\Users\...\.cache\muninn-flutter-windows-cargo'
#   .\scripts\flutter-dist.ps1
#   # or:  SKIP_BUILD=1 via $env:SKIP_BUILD = '1'
#
# Needs Git Bash (bash.exe) on PATH, Flutter Windows desktop, and optionally
# Inno Setup 6.3+ (ISCC) plus the msix pub package.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if (-not $root) { $root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path }
$bash = $null
foreach ($cand in @(
  'C:\Program Files\Git\bin\bash.exe',
  'C:\Program Files\Git\usr\bin\bash.exe',
  (Get-Command bash.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source)
)) {
  if ($cand -and (Test-Path $cand)) { $bash = $cand; break }
}
if (-not $bash) {
  throw 'Git Bash bash.exe not found. Install Git for Windows or run scripts/flutter-dist.sh from Git Bash.'
}
$script = Join-Path $root 'scripts\flutter-dist.sh'
# Convert Windows path to a Git Bash path (/c/...)
function ConvertTo-BashPath([string]$p) {
  $full = (Resolve-Path $p -ErrorAction SilentlyContinue)
  if ($full) { $p = $full.Path }
  $p = $p -replace '\\', '/'
  if ($p -match '^([A-Za-z]):/(.*)$') {
    return '/' + $Matches[1].ToLower() + '/' + $Matches[2]
  }
  return $p
}
$bashScript = ConvertTo-BashPath $script
$envArgs = @()
if ($env:DIST) { $envArgs += "DIST=$(ConvertTo-BashPath $env:DIST)" }
if ($env:CARGO_TARGET_DIR) { $envArgs += "CARGO_TARGET_DIR=$(ConvertTo-BashPath $env:CARGO_TARGET_DIR)" }
if ($env:SKIP_BUILD) { $envArgs += "SKIP_BUILD=$($env:SKIP_BUILD)" }
if ($env:ISCC) { $envArgs += "ISCC=$(ConvertTo-BashPath $env:ISCC)" }
$cmd = if ($envArgs.Count -gt 0) { ($envArgs -join ' ') + ' ' + "'" + $bashScript + "'" } else { "'" + $bashScript + "'" }
Write-Host "Running: bash -lc $cmd"
& $bash -lc $cmd
exit $LASTEXITCODE
