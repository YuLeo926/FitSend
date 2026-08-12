param(
    [string]$CargoTargetDirectory = $env:CARGO_TARGET_DIR
)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot

& (Join-Path $PSScriptRoot "stage-ffmpeg.ps1")

if (-not $CargoTargetDirectory) {
    $CargoTargetDirectory = Join-Path $projectRoot "src-tauri\target"
}
$env:CARGO_TARGET_DIR = $CargoTargetDirectory

Push-Location $projectRoot
try {
    & npm.cmd run tauri build
    if ($LASTEXITCODE -ne 0) { throw "Tauri build failed with exit code $LASTEXITCODE." }

    $version = (Get-Content -Raw (Join-Path $projectRoot "package.json") | ConvertFrom-Json).version
    $releaseDirectory = Join-Path $projectRoot "release"
    $bundleDirectory = Join-Path $CargoTargetDirectory "release\bundle"
    $portableDirectory = Join-Path $releaseDirectory "FitSend_$version`_portable"
    New-Item -ItemType Directory -Force -Path $releaseDirectory,$portableDirectory | Out-Null

    $legacyPortable = Join-Path $releaseDirectory "FitSend_$version`_portable.exe"
    if (Test-Path -LiteralPath $legacyPortable) {
        Remove-Item -LiteralPath $legacyPortable -Force
    }

    Copy-Item -LiteralPath (Join-Path $bundleDirectory "msi\FitSend_${version}_x64_en-US.msi") -Destination $releaseDirectory -Force
    Copy-Item -LiteralPath (Join-Path $bundleDirectory "nsis\FitSend_${version}_x64-setup.exe") -Destination $releaseDirectory -Force
    Copy-Item -LiteralPath (Join-Path $CargoTargetDirectory "release\fitsend.exe") -Destination (Join-Path $portableDirectory "FitSend.exe") -Force
    Copy-Item -LiteralPath (Join-Path $projectRoot "src-tauri\resources\ffmpeg") -Destination $portableDirectory -Recurse -Force

    $portableZip = Join-Path $releaseDirectory "FitSend_${version}_portable.zip"
    if (Test-Path -LiteralPath $portableZip) { Remove-Item -LiteralPath $portableZip -Force }
    Compress-Archive -Path (Join-Path $portableDirectory "*") -DestinationPath $portableZip -CompressionLevel Optimal

    Write-Output "Bundled release files:"
    Get-ChildItem -LiteralPath $releaseDirectory -File | Select-Object Name,Length
} finally {
    Pop-Location
}
