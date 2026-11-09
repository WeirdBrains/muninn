# Build the Windows installers (muninn-<ver>-<arch>-windows-setup.exe) with Inno Setup,
# from the release zips that scripts/cross-windows.sh makes.
# Runs on the GitHub Actions windows-2025 runner.
#   scripts\windows-installer-build.ps1
# Environment: DIST (default dist) holds muninn-<ver>-<arch>-pc-windows-gnullvm.zip and
# receives the setup exes; VER (default parsed from Cargo.toml); ISCC (default Inno
# Setup 6 ISCC.exe found on PATH).
param()

$ver = if ($env:VER) {
    $env:VER
} else {
    $m = Select-String -Path Cargo.toml -Pattern '^version = "(.+)"'
    if ($m) { $m.Matches.Groups[1].Value } else { "0.0.0" }
}
$dist = if ($env:DIST) { $env:DIST } else { "dist" }
$work = "muninn-setup-$ver"
$tmpDir = Join-Path $env:TEMP "muninn-installer-$ver"

function Cleanup() {
    if (Test-Path $work) { Remove-Item -Recurse -Force $work }
    if (Test-Path $tmpDir) { Remove-Item -Recurse -Force $tmpDir }
}
trap { Cleanup; throw }

New-Item -ItemType Directory -Force -Path "$work/out" | Out-Null
New-Item -ItemType Directory -Force -Path $tmpDir | Out-Null

# Find Inno Setup ISCC
$iscc = $env:ISCC
if (-not $iscc -or -not (Test-Path $iscc)) {
    $paths = Get-ChildItem -Path "$env:LOCALAPPDATA\Programs\Inno Setup*" -Directory -ErrorAction SilentlyContinue | Sort-Object Name -Descending
    if ($paths.Count -gt 0) {
        $iscc = Join-Path $paths[0].FullName "ISCC.exe"
    }
}
if (-not $iscc -or -not (Test-Path $iscc)) {
    Write-Error "Inno Setup ISCC.exe not found; install Inno Setup 6+ or set ISCC"
    exit 1
}
Write-Host "ISCC: $iscc"
Write-Host "Version: $ver"

# Unzip release folders per architecture
$zipFiles = Get-ChildItem -Path $dist -Filter "*-pc-windows-gnullvm.zip" -ErrorAction SilentlyContinue
if ($zipFiles.Count -eq 0) {
    Write-Error "No *-pc-windows-gnullvm.zip found in dist/"
    exit 1
}

foreach ($zip in $zipFiles) {
    $base = [System.IO.Path]::GetFileNameWithoutExtension($zip.Name)
    # base is like "muninn-2.22.3-x86_64-pc-windows-gnullvm"
    $arch = ($base -split '-pc-windows-gnullvm')[0] -replace '^muninn-\d+\.\d+\.\d+-',''
    Write-Host "Unzipping $($zip.Name) -> $work\$arch"
    New-Item -ItemType Directory -Force -Path "$work\$arch" | Out-Null
    Expand-Archive -Path $zip.FullName -DestinationPath "$work\$arch" -Force
}

# Copy Inno Setup project files
Copy-Item packaging\windows\muninn.iss -Destination "$work\" -Force
Copy-Item packaging\windows\muninn-launch.cmd -Destination "$work\" -Force

# Generate .ico from SVG using ImageMagick (installed on the runner).
$icoPath = Join-Path $work "muninn.ico"
$svg = "assets/icons/muninn.svg"
$sizes = @(16, 24, 32, 48, 64, 128, 256)
$pngFiles = @()
foreach ($s in $sizes) {
    $pngFiles += Join-Path $tmpDir "$s.png"
}
for ($i = 0; $i -lt $sizes.Count; $i++) {
    magick.exe -background none -density 384 $svg -resize "${sizes[$i]}x${sizes[$i]}" $pngFiles[$i] 2>$null
}
magick.exe $pngFiles -define icon:auto-generate=true $icoPath 2>$null
if (-not (Test-Path $icoPath)) {
    Write-Warning "Icon generation failed (will build without custom icon)"
}

# Build installers for each architecture
foreach ($arch in @("x86_64", "aarch64")) {
    $srcDir = Join-Path $work $arch
    if (-not (Test-Path $srcDir)) {
        Write-Host "Skipping $arch: no zip found"
        continue
    }
    $exeSrc = Join-Path $srcDir "mnnn.exe"
    if (-not (Test-Path $exeSrc)) {
        Write-Error "mnnn.exe not found in $srcDir"
        exit 1
    }

    Write-Host "Building installer for $arch..."
    $params = @(
        "/Q",
        "/DAppVersion=$ver",
        "/DArch=$arch",
        "/DSrcDir=$srcDir",
        "/DOutDir=$work\out",
        "$work\muninn.iss"
    )
    if ($icoPath -and (Test-Path $icoPath)) {
        $params += "/DIconFile=$icoPath"
    }

    & $iscc @params
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Inno Setup failed for $arch"
        exit 1
    }

    $outFile = Join-Path $work "out\muninn-$ver-$arch-windows-setup.exe"
    if (Test-Path $outFile) {
        Copy-Item $outFile (Join-Path $dist (Split-Path $outFile -Leaf)) -Force
        Write-Host "  -> $(Split-Path $outFile -Leaf)"
    } else {
        Write-Error "Installer not found at $outFile"
        exit 1
    }
}

Cleanup
Write-Host "Done. Windows setup installers built in $dist/"
