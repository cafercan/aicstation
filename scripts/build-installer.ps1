# Builds the release exe and the Inno Setup installer, then writes SHA256SUMS.txt.
# Output: dist\AICStation-v<version>-windows-x64-setup.exe and dist\SHA256SUMS.txt
# Requires Inno Setup 6 (ISCC.exe). butaglyph (scripts/butaglyph.json) is
# downloaded and installed next to the app as buta.exe.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$version = (Get-Content src-tauri\tauri.conf.json -Raw | ConvertFrom-Json).version
$iscc = @(
  "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
  "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
  "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) { throw "Inno Setup 6 (ISCC.exe) not found" }

npx tauri build --no-bundle
if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }
# The exe is in cargo's target dir, which a cargo config may move (e.g. a
# shared D:\cargo-target\windows); the installer takes it from there.
$target = (cargo metadata --format-version 1 --no-deps | ConvertFrom-Json).target_directory
if ($LASTEXITCODE -ne 0 -or -not $target) { throw "cargo metadata failed" }

node scripts/fetch-butaglyph.mjs x86_64-pc-windows-msvc
if ($LASTEXITCODE -ne 0) { throw "fetching butaglyph failed" }

if (Test-Path dist) { Remove-Item -Recurse -Force dist }
& $iscc "/DAppVersion=$version" "/DTargetDir=$target" installer\AICStation.iss
if ($LASTEXITCODE -ne 0) { throw "ISCC failed" }

$setup = "AICStation-v$version-windows-x64-setup.exe"
$hash = (Get-FileHash "dist\$setup" -Algorithm SHA256).Hash.ToLower()
[IO.File]::WriteAllText("$root\dist\SHA256SUMS.txt", "$hash  $setup`n")
Get-Content dist\SHA256SUMS.txt
