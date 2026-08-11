param(
    [string]$Version = "dev",
    [switch]$SkipChecks
)

$ErrorActionPreference = "Stop"

if ($Version -notmatch "^[0-9A-Za-z._-]+$") {
    throw "Version contains invalid characters: $Version"
}

$projectRoot = Split-Path -Parent $PSScriptRoot
$distDirectory = Join-Path $projectRoot "dist"

$packageName =
    "com_port_reader-$Version-windows-x86_64"

$packageDirectory =
    Join-Path $distDirectory $packageName

$archivePath =
    Join-Path $distDirectory "$packageName.zip"

Set-Location $projectRoot

if (-not $SkipChecks) {
    & cargo test

    if ($LASTEXITCODE -ne 0) {
        throw "cargo test failed"
    }

    & cargo clippy --all-targets -- -D warnings

    if ($LASTEXITCODE -ne 0) {
        throw "cargo clippy failed"
    }
}

& cargo build `
    --release `
    --bin com_port_reader

if ($LASTEXITCODE -ne 0) {
    throw "cargo build failed"
}

$requiredPaths = @(
    "target\release\com_port_reader.exe",
    "startup.lua",
    "lua_scripts",
    "emulator_scripts",
    "lua_types"
)

foreach ($relativePath in $requiredPaths) {
    $sourcePath =
        Join-Path $projectRoot $relativePath

    if (-not (Test-Path $sourcePath)) {
        throw "Required release file is missing: $sourcePath"
    }
}

New-Item `
    -ItemType Directory `
    -Path $distDirectory `
    -Force |
    Out-Null

if (Test-Path $packageDirectory) {
    Remove-Item `
        -Path $packageDirectory `
        -Recurse `
        -Force
}

if (Test-Path $archivePath) {
    Remove-Item `
        -Path $archivePath `
        -Force
}

New-Item `
    -ItemType Directory `
    -Path $packageDirectory |
    Out-Null

Copy-Item `
    (Join-Path `
        $projectRoot `
        "target\release\com_port_reader.exe") `
    $packageDirectory

Copy-Item `
    (Join-Path $projectRoot "startup.lua") `
    $packageDirectory

Copy-Item `
    (Join-Path $projectRoot "lua_scripts") `
    $packageDirectory `
    -Recurse

Copy-Item `
    (Join-Path $projectRoot "emulator_scripts") `
    $packageDirectory `
    -Recurse

Copy-Item `
    (Join-Path $projectRoot "lua_types") `
    $packageDirectory `
    -Recurse

foreach ($optionalFile in @(
    "README.md",
    "LICENSE"
)) {
    $sourcePath =
        Join-Path $projectRoot $optionalFile

    if (Test-Path $sourcePath) {
        Copy-Item `
            $sourcePath `
            $packageDirectory
    }
}

Compress-Archive `
    -Path $packageDirectory `
    -DestinationPath $archivePath

Write-Host ""
Write-Host "Release package created:"
Write-Host $archivePath
