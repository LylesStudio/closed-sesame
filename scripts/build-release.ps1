# One-click local release build.
# Matches .gitea/workflows/release.yml for the host platform:
#   sesame         --features cli,mount
#   sesame-recover --features recover
#   sesame-gui     --features gui-mount
# Each binary is built separately so feature sets stay the same as CI.
#
# Usage (from the repo root or anywhere):
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-release.ps1
#   scripts\build-release.bat

param(
    [string]$Version = ""
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

function Invoke-ReleaseBuild {
    param(
        [Parameter(Mandatory = $true)][string]$Bin,
        [Parameter(Mandatory = $true)][string]$Features
    )
    Write-Host ""
    Write-Host ">> cargo build --release --locked --no-default-features --features $Features --bin $Bin" -ForegroundColor Cyan
    & cargo build --release --locked --no-default-features --features $Features --bin $Bin
    if ($LASTEXITCODE -ne 0) {
        throw "cargo failed with exit code $LASTEXITCODE"
    }
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "cargo was not found on PATH. Install the Rust toolchain from rust-toolchain.toml first."
}
if (-not (Get-Command rustc -ErrorAction SilentlyContinue)) {
    throw "rustc was not found on PATH. Install the Rust toolchain from rust-toolchain.toml first."
}

if ([string]::IsNullOrWhiteSpace($Version)) {
    $cargoToml = Get-Content -LiteralPath (Join-Path $repoRoot "Cargo.toml") -Raw
    if ($cargoToml -notmatch '(?m)^version\s*=\s*"([^"]+)"') {
        throw "Could not read package version from Cargo.toml. Pass -Version explicitly."
    }
    $Version = $Matches[1]
}

$hostLine = & rustc -vV | Where-Object { $_ -like "host: *" }
if (-not $hostLine) {
    throw "Could not read the host target from 'rustc -vV'."
}
$target = ($hostLine -replace '^host:\s*', '').Trim()

$isWindows = $env:OS -eq "Windows_NT"
$exeSuffix = if ($isWindows) { ".exe" } else { "" }

$distDir = Join-Path $repoRoot "dist"
$stagingName = "closed-sesame-$Version-$target"
$stagingDir = Join-Path $distDir $stagingName
$archiveName = if ($isWindows) { "$stagingName.zip" } else { "$stagingName.tar.gz" }
$archivePath = Join-Path $distDir $archiveName
$checksumPath = Join-Path $distDir "SHA256SUMS.txt"

Write-Host "ClosedSesame release" -ForegroundColor Green
Write-Host "  version : $Version"
Write-Host "  target  : $target"
Write-Host "  output  : $archivePath"

$builds = @(
    @{ Bin = "sesame";         Features = "cli,mount" },
    @{ Bin = "sesame-recover"; Features = "recover" },
    @{ Bin = "sesame-gui";     Features = "gui-mount" }
)

foreach ($build in $builds) {
    Invoke-ReleaseBuild -Bin $build.Bin -Features $build.Features
}

if (Test-Path -LiteralPath $stagingDir) {
    Remove-Item -LiteralPath $stagingDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $stagingDir | Out-Null

$releaseDir = Join-Path $repoRoot "target\release"
foreach ($build in $builds) {
    $fileName = "$($build.Bin)$exeSuffix"
    $source = Join-Path $releaseDir $fileName
    if (-not (Test-Path -LiteralPath $source)) {
        throw "Expected binary was not produced: $source"
    }
    Copy-Item -LiteralPath $source -Destination (Join-Path $stagingDir $fileName)
}

$docs = @(
    @{ Source = "README.md"; Destination = "README.md" },
    @{ Source = "THIRD_PARTY.md"; Destination = "THIRD_PARTY.md" },
    @{ Source = "LICENSE"; Destination = "LICENSE" },
    @{ Source = "vendor\winfsp\README.md"; Destination = "WINFSP-INSTALL.md" }
)
foreach ($doc in $docs) {
    $source = Join-Path $repoRoot $doc.Source
    if (-not (Test-Path -LiteralPath $source)) {
        throw "Missing release document: $($doc.Source)"
    }
    Copy-Item -LiteralPath $source -Destination (Join-Path $stagingDir $doc.Destination)
}

if (Test-Path -LiteralPath $archivePath) {
    Remove-Item -LiteralPath $archivePath -Force
}

if ($isWindows) {
    Compress-Archive -Path (Join-Path $stagingDir "*") -DestinationPath $archivePath -CompressionLevel Optimal
} else {
    & tar -czf $archivePath -C $distDir $stagingName
    if ($LASTEXITCODE -ne 0) {
        throw "tar failed with exit code $LASTEXITCODE"
    }
}

$hash = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
$checksumLine = "$hash  $archiveName"
Set-Content -LiteralPath $checksumPath -Value $checksumLine -Encoding ascii

Write-Host ""
Write-Host "Release package ready" -ForegroundColor Green
Write-Host "  archive : $archivePath"
Write-Host "  sha256  : $hash"
Write-Host "  sums    : $checksumPath"
Get-ChildItem -LiteralPath $stagingDir | ForEach-Object {
    Write-Host ("  {0,12:N0}  {1}" -f $_.Length, $_.Name)
}
