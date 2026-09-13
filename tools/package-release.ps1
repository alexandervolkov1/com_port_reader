param(
    [string]$Version,
    [switch]$SkipChecks
)

$ErrorActionPreference = "Stop"
$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$distDirectory = Join-Path $projectRoot "dist"
Set-Location $projectRoot

# This bundle contains native MSVC x64 executables, not cross-platform binaries.
$compiler = & rustc -vV
if ($LASTEXITCODE -ne 0 -or $compiler -notcontains "host: x86_64-pc-windows-msvc") {
    throw "Packaging requires the x86_64-pc-windows-msvc Rust toolchain."
}

$metadataJson = & cargo metadata --locked --no-deps --format-version 1
if ($LASTEXITCODE -ne 0) { throw "cargo metadata failed" }
$metadata = $metadataJson | ConvertFrom-Json
$package = @($metadata.packages | Where-Object name -eq "com_port_reader")
if ($package.Count -ne 1) { throw "Could not identify the com_port_reader package." }
if ([string]::IsNullOrWhiteSpace($Version)) { $Version = $package[0].version }
if ($Version -ne $package[0].version) {
    throw "Version must match Cargo.toml ($($package[0].version)); update Cargo.toml and Cargo.lock first."
}
if ($Version -notmatch "^[0-9A-Za-z._-]+$") { throw "Invalid package version: $Version" }

if (-not $SkipChecks) {
    & cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw "cargo fmt failed" }
    & cargo test --locked
    if ($LASTEXITCODE -ne 0) { throw "cargo test failed" }
    & cargo clippy --locked --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw "cargo clippy failed" }
}

& cargo build --locked --release --bins
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$requiredPaths = @(
    "target\release\com_port_reader.exe",
    "target\release\device_emulator.exe",
    "startup.lua", "README.md", "CHANGELOG.md",
    "lua_scripts", "emulator_scripts", "lua_types", "profiles", "docs"
)
foreach ($relativePath in $requiredPaths) {
    $sourcePath = Join-Path $projectRoot $relativePath
    if (-not (Test-Path -LiteralPath $sourcePath)) {
        throw "Required release file is missing: $sourcePath"
    }
}

New-Item -ItemType Directory -Path $distDirectory -Force | Out-Null
$distDirectory = (Resolve-Path -LiteralPath $distDirectory).Path
function Assert-DistChild([string]$Path) {
    $absolute = [IO.Path]::GetFullPath($Path)
    if (-not [string]::Equals([IO.Path]::GetDirectoryName($absolute), $distDirectory,
            [StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to modify a path outside the distribution directory: $absolute"
    }
}

$packageName = "com_port_reader-$Version-windows-x86_64"
$packageDirectory = Join-Path $distDirectory $packageName
$archivePath = Join-Path $distDirectory "$packageName.zip"
$checksumPath = "$archivePath.sha256"
$stagingDirectory = Join-Path $distDirectory (".staging-" + [guid]::NewGuid().ToString("N"))
Assert-DistChild $stagingDirectory
New-Item -ItemType Directory -Path $stagingDirectory | Out-Null
$stagedPackage = Join-Path $stagingDirectory $packageName
New-Item -ItemType Directory -Path $stagedPackage | Out-Null

# Copy only shipped resources, never previous recordings, logs or remembered profiles.
foreach ($relativePath in $requiredPaths) {
    Copy-Item -LiteralPath (Join-Path $projectRoot $relativePath) -Destination $stagedPackage -Recurse
}
if (Test-Path -LiteralPath (Join-Path $projectRoot "LICENSE")) {
    Copy-Item -LiteralPath (Join-Path $projectRoot "LICENSE") -Destination $stagedPackage
}

$stagedArchive = Join-Path $stagingDirectory "$packageName.zip"
Compress-Archive -LiteralPath $stagedPackage -DestinationPath $stagedArchive
$checksum = (Get-FileHash -LiteralPath $stagedArchive -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText("$stagedArchive.sha256", "$checksum  $packageName.zip" + [Environment]::NewLine,
    [Text.Encoding]::ASCII)

# Preserve the old package, including any user measurements inside it.
$previousOutputs = @($packageDirectory, $archivePath, $checksumPath)
foreach ($output in $previousOutputs) { Assert-DistChild $output }
$existingOutputs = @($previousOutputs | Where-Object { Test-Path -LiteralPath $_ })
if ($existingOutputs.Count -gt 0) {
    $backupDirectory = Join-Path $distDirectory (".previous-" + [guid]::NewGuid().ToString("N"))
    Assert-DistChild $backupDirectory
    New-Item -ItemType Directory -Path $backupDirectory | Out-Null
    foreach ($output in $existingOutputs) {
        Move-Item -LiteralPath $output -Destination $backupDirectory
    }
    Write-Host "Previous release files preserved in: $backupDirectory"
}
Move-Item -LiteralPath $stagedPackage -Destination $packageDirectory
Move-Item -LiteralPath $stagedArchive -Destination $archivePath
Move-Item -LiteralPath "$stagedArchive.sha256" -Destination $checksumPath
# This is the freshly created, now empty staging directory; no recursive deletion.
Assert-DistChild $stagingDirectory
Remove-Item -LiteralPath $stagingDirectory

Write-Host "Release package: $archivePath"
Write-Host "SHA-256: $checksum"
Write-Host "Upload the ZIP and its .sha256 file. Packaging does not publish or create a Git tag."
