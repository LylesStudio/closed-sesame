param(
    [string]$LocalesDir = "locales"
)

$ErrorActionPreference = "Stop"
$files = @("en.yml", "zh-CN.yml", "ja.yml")
$keysByLocale = @{}

function Get-LocaleKeys([string]$Path) {
    $keys = @()
    Get-Content $Path | ForEach-Object {
        $line = $_.Trim()
        if ($line -eq '' -or $line.StartsWith('_')) { return }
        if ($line -match '^([^:#\s][^:]*):') {
            $keys += $Matches[1].Trim()
        }
    }
    return , ($keys | Sort-Object -Unique)
}

foreach ($file in $files) {
    $path = Join-Path $LocalesDir $file
    if (-not (Test-Path $path)) {
        Write-Error "Missing locale file: $path"
    }
    $keysByLocale[$file] = Get-LocaleKeys $path
}

$baseline = $keysByLocale["en.yml"]
$failed = $false

foreach ($file in $files) {
    if ($file -eq "en.yml") { continue }
    $current = $keysByLocale[$file]
    $missing = Compare-Object -ReferenceObject $baseline -DifferenceObject $current -PassThru |
        Where-Object { $_.SideIndicator -eq "<=" }
    $extra = Compare-Object -ReferenceObject $baseline -DifferenceObject $current -PassThru |
        Where-Object { $_.SideIndicator -eq "=>" }
    if ($missing -or $extra) {
        $failed = $true
        Write-Host "Locale parity failed for $file"
        if ($missing) { Write-Host "  Missing keys:"; $missing | ForEach-Object { Write-Host "    $_" } }
        if ($extra) { Write-Host "  Extra keys:"; $extra | ForEach-Object { Write-Host "    $_" } }
    }
}

if ($failed) {
    exit 1
}

Write-Host "Locale key parity OK ($($baseline.Count) keys x $($files.Count) locales)"
