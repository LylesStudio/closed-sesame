param(
    [string]$ControlDrive = "Y:"
)

$ErrorActionPreference = "Continue"
$repoRoot = Split-Path -Parent $PSScriptRoot
$outputDirectory = Join-Path $repoRoot "target\winfsp-debug"
$outputPath = Join-Path $outputDirectory "baseline.txt"
$winFspBin = Join-Path ${env:ProgramFiles(x86)} "WinFsp\bin"
$winFspDll = Join-Path $winFspBin "winfsp-x64.dll"
$memFs = Join-Path $winFspBin "memfs-x64.exe"

New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null

& {
    "Captured: $(Get-Date -Format o)"
    "Repository: $repoRoot"
    ""
    "== WinFsp runtime =="
    if (Test-Path $winFspDll) {
        $version = (Get-Item $winFspDll).VersionInfo
        "DLL: $winFspDll"
        "FileVersion: $($version.FileVersion)"
        "ProductVersion: $($version.ProductVersion)"
    } else {
        "DLL missing: $winFspDll"
    }
    sc.exe query winfsp 2>&1

    ""
    "== Drive allocation =="
    foreach ($letter in [char[]](84..90)) {
        fsutil fsinfo drivetype "$letter`:" 2>&1
    }

    ""
    "== Official memfs control =="
    if (!(Test-Path $memFs)) {
        "memfs missing: $memFs"
    } elseif (Test-Path "$ControlDrive\") {
        "Control drive is already allocated: $ControlDrive"
    } else {
        $process = Start-Process -FilePath $memFs -ArgumentList "-m", $ControlDrive -PassThru
        try {
            Start-Sleep -Seconds 2
            try {
                [System.IO.Directory]::EnumerateFileSystemEntries("$ControlDrive\") |
                    Select-Object -First 5
                "Control access: OK"
            } catch {
                "Control access: FAILED ($($_.Exception.Message))"
            }
        } finally {
            Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
            $process.WaitForExit()
        }
    }

    ""
    "== ClosedSesame build =="
    Push-Location $repoRoot
    try {
        cargo build --features "cli,mount,winfsp-debug" --bin sesame 2>&1
        if ($LASTEXITCODE -eq 0) {
            Get-FileHash (Join-Path $repoRoot "target\debug\sesame.exe")
        }
    } finally {
        Pop-Location
    }
} | Tee-Object -FilePath $outputPath

Write-Host "Baseline written to $outputPath"
