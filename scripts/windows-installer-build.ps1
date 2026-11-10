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
# Absolute: ISCC resolves relative /D paths against the .iss file's folder.
$work = Join-Path (Get-Location) "muninn-setup-$ver"
$tmpDir = Join-Path $env:TEMP "muninn-installer-$ver"

function Cleanup() {
    if (Test-Path $work) { Remove-Item -Recurse -Force $work }
    if (Test-Path $tmpDir) { Remove-Item -Recurse -Force $tmpDir }
}
# Report the real error (a bare `throw` here only says "ScriptHalted"), then fail.
trap {
    Write-Host "::error::$($_.Exception.GetType().FullName): $($_.Exception.Message)"
    Write-Host $_.InvocationInfo.PositionMessage
    if ($_.ScriptStackTrace) { Write-Host $_.ScriptStackTrace }
    Cleanup
    exit 1
}

# Run a native tool; fail with its output when it exits non-zero.
function Invoke-Tool([string]$exe, [string[]]$toolArgs, [string]$what) {
    $out = & $exe @toolArgs 2>&1
    $code = $LASTEXITCODE
    if ($code -ne 0) {
        Write-Host ($out | Out-String)
        throw "$what failed (exit $code): $exe $($toolArgs -join ' ')"
    }
    $out
}

New-Item -ItemType Directory -Force -Path "$work/out" | Out-Null
New-Item -ItemType Directory -Force -Path $tmpDir | Out-Null

# Find Inno Setup ISCC
$iscc = $env:ISCC
if (-not $iscc -or -not (Test-Path $iscc)) {
    $cmd = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($cmd) {
        $iscc = $cmd.Source
    } else {
        # winget installs per-user (LOCALAPPDATA) or per-machine (Program Files) depending on elevation.
        $roots = @("$env:LOCALAPPDATA\Programs", ${env:ProgramFiles(x86)}, $env:ProgramFiles) | Where-Object { $_ }
        $paths = @(foreach ($r in $roots) { Get-ChildItem -Path (Join-Path $r "Inno Setup*") -Directory -ErrorAction SilentlyContinue }) |
            Where-Object { Test-Path (Join-Path $_.FullName "ISCC.exe") } | Sort-Object Name -Descending
        if ($paths.Count -gt 0) {
            $iscc = Join-Path $paths[0].FullName "ISCC.exe"
        }
    }
}
if (-not $iscc -or -not (Test-Path $iscc)) {
    throw "Inno Setup ISCC.exe not found; install Inno Setup 6+ or set ISCC"
}
Write-Host "ISCC: $iscc"
Write-Host "Version: $ver"

# Unzip release folders per architecture
$zipFiles = Get-ChildItem -Path $dist -Filter "*-pc-windows-gnullvm.zip" -ErrorAction SilentlyContinue
if ($zipFiles.Count -eq 0) {
    throw "No *-pc-windows-gnullvm.zip found in $dist"
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

# Find ImageMagick: PATH, else its Program Files install dir (`winget install
# ImageMagick.ImageMagick` installs it silently without adding it to PATH).
$magick = $env:MAGICK
if (-not $magick -or -not (Test-Path $magick)) {
    $cmd = Get-Command magick.exe -ErrorAction SilentlyContinue
    if ($cmd) {
        $magick = $cmd.Source
    } else {
        $roots = @($env:ProgramFiles, ${env:ProgramFiles(x86)}, "$env:LOCALAPPDATA\Programs") | Where-Object { $_ }
        $found = @(foreach ($r in $roots) { Get-ChildItem -Path (Join-Path $r "ImageMagick*") -Directory -ErrorAction SilentlyContinue }) |
            Where-Object { Test-Path (Join-Path $_.FullName "magick.exe") } | Sort-Object Name -Descending
        if ($found.Count -gt 0) { $magick = Join-Path $found[0].FullName "magick.exe" }
    }
}
if (-not $magick -or -not (Test-Path $magick)) {
    throw "ImageMagick magick.exe not found on PATH or under Program Files; install ImageMagick 7 or set MAGICK"
}
Write-Host "magick: $magick"

# Generate .ico from SVG (muninn.iss needs it).
$icoPath = Join-Path $work "muninn.ico"
$svg = "assets/icons/muninn.svg"
$sizes = @(16, 24, 32, 48, 64, 128, 256)
$pngFiles = @()
foreach ($s in $sizes) {
    $pngFiles += Join-Path $tmpDir "$s.png"
}
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $sz = "$($sizes[$i])x$($sizes[$i])"
    Invoke-Tool $magick @("-background", "none", "-density", "384", $svg, "-resize", $sz, $pngFiles[$i]) "magick ($sz png)" | Out-Null
    if (-not (Test-Path $pngFiles[$i])) { throw "magick made no $($pngFiles[$i])" }
}
Invoke-Tool $magick (@($pngFiles) + @("-define", "icon:auto-generate=true", $icoPath)) "magick (ico)" | Out-Null
if (-not (Test-Path $icoPath)) { throw "magick made no $icoPath" }
Write-Host "Icon: $icoPath"

# Build installers for each architecture
foreach ($arch in @("x86_64", "aarch64")) {
    $srcDir = Join-Path $work $arch
    if (-not (Test-Path $srcDir)) {
        Write-Host "Skipping ${arch}: no zip found"
        continue
    }
    # The zip holds one muninn-<ver>-<arch>-pc-windows-gnullvm folder.
    if (-not (Test-Path (Join-Path $srcDir "mnnn.exe"))) {
        $nested = @(Get-ChildItem -Path $srcDir -Directory | Where-Object { Test-Path (Join-Path $_.FullName "mnnn.exe") })
        if ($nested.Count -eq 1) { $srcDir = $nested[0].FullName }
    }
    if (-not (Test-Path (Join-Path $srcDir "mnnn.exe"))) {
        throw "mnnn.exe not found in $srcDir"
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
        throw "Inno Setup failed for $arch (exit $LASTEXITCODE)"
    }

    $outFile = Join-Path $work "out\muninn-$ver-$arch-windows-setup.exe"
    if (Test-Path $outFile) {
        Copy-Item $outFile (Join-Path $dist (Split-Path $outFile -Leaf)) -Force
        Write-Host "  -> $(Split-Path $outFile -Leaf)"
    } else {
        throw "Installer not found at $outFile"
    }
}

Cleanup
Write-Host "Done. Windows setup installers built in $dist/"
