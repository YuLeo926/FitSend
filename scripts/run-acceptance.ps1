param(
    [string]$CargoTargetDirectory = $env:CARGO_TARGET_DIR
)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot
if (-not $CargoTargetDirectory) {
    $CargoTargetDirectory = Join-Path $env:LOCALAPPDATA "FitSendAcceptanceBuild\target"
}
$env:CARGO_TARGET_DIR = $CargoTargetDirectory

$bundledFfmpeg = Join-Path $projectRoot "src-tauri\resources\ffmpeg\ffmpeg.exe"
$bundledFfprobe = Join-Path $projectRoot "src-tauri\resources\ffmpeg\ffprobe.exe"
if ((Test-Path -LiteralPath $bundledFfmpeg) -and (Test-Path -LiteralPath $bundledFfprobe)) {
    $env:FITSEND_FFMPEG_PATH = $bundledFfmpeg
    $env:FITSEND_FFPROBE_PATH = $bundledFfprobe
}

$reportDirectory = Join-Path $projectRoot "output\acceptance"
Push-Location (Join-Path $projectRoot "src-tauri")
try {
    & cargo run --release -p fitsend-core --example acceptance_matrix -- $reportDirectory
    if ($LASTEXITCODE -ne 0) { throw "The FitSend acceptance matrix reported failures." }
} finally {
    Pop-Location
}
