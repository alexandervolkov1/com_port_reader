param(
    [string]$Version,
    [switch]$SkipChecks
)

$ErrorActionPreference = "Stop"

$projectRoot = Split-Path -Parent $PSScriptRoot
$distDirectory = Join-Path $projectRoot "dist"

Set-Location $projectRoot

if ([string]::IsNullOrWhiteSpace($Version)) {
    $metadata = & cargo metadata --no-deps --format-version 1 | ConvertFrom-Json

    if ($LASTEXITCODE -ne 0) {
        throw "cargo metadata failed"
    }

    $Version = $metadata.packages |
        Select-Object -ExpandProperty version

    if ([string]::IsNullOrWhiteSpace($Version)) {
        throw "Could not determine the package version from Cargo.toml"
    }
}

if ($Version -notmatch "^[0-9A-Za-z._-]+$") {
    throw "Version contains invalid characters: $Version"
}

$packageName =
    "com_port_reader-$Version-windows-x86_64"

$packageDirectory =
    Join-Path $distDirectory $packageName

$archivePath =
    Join-Path $distDirectory "$packageName.zip"

if (-not $SkipChecks) {
    & cargo fmt --check

    if ($LASTEXITCODE -ne 0) {
        throw "cargo fmt --check failed"
    }

    & cargo test

    if ($LASTEXITCODE -ne 0) {
        throw "cargo test failed"
    }

    & cargo test --doc

    if ($LASTEXITCODE -ne 0) {
        throw "cargo test --doc failed"
    }

    & cargo clippy --all-targets -- -D warnings

    if ($LASTEXITCODE -ne 0) {
        throw "cargo clippy failed"
    }
}

& cargo build `
    --release `
    --bins

if ($LASTEXITCODE -ne 0) {
    throw "cargo build failed"
}

$requiredPaths = @(
    "target\release\com_port_reader.exe",
    "target\release\device_emulator.exe",
    "startup.lua",
    "lua_scripts",
    "emulator_scripts",
    "lua_types",
    "profiles",
    "docs"
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
    (Join-Path `
        $projectRoot `
        "target\release\device_emulator.exe") `
    $packageDirectory

Copy-Item `
    (Join-Path $projectRoot "startup.lua") `
    $packageDirectory

Copy-Item `
    (Join-Path $projectRoot "lua_scripts") `
    $packageDirectory `
    -Recurse

Copy-Item `
    (Join-Path $projectRoot "docs") `
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

Copy-Item `
    (Join-Path $projectRoot "profiles") `
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
