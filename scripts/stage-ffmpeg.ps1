param(
    [string]$SourceDirectory = $env:FITSEND_FFMPEG_SOURCE_DIR
)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot
$destination = Join-Path $projectRoot "src-tauri\resources\ffmpeg"

function Resolve-ToolPath {
    param([string]$Name)

    if ($SourceDirectory) {
        $direct = Join-Path $SourceDirectory "$Name.exe"
        $insideBin = Join-Path $SourceDirectory "bin\$Name.exe"
        if (Test-Path -LiteralPath $direct) { return (Resolve-Path -LiteralPath $direct).Path }
        if (Test-Path -LiteralPath $insideBin) { return (Resolve-Path -LiteralPath $insideBin).Path }
        throw "Could not find $Name.exe in '$SourceDirectory' or its bin folder."
    }

    $command = Get-Command $Name -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $command) {
        throw "Could not find $Name.exe. Install FFmpeg or set FITSEND_FFMPEG_SOURCE_DIR to an extracted FFmpeg folder."
    }
    return $command.Source
}

$ffmpeg = Resolve-ToolPath "ffmpeg"
$ffprobe = Resolve-ToolPath "ffprobe"
$binDirectory = Split-Path -Parent $ffmpeg
$packageRoot = @($binDirectory, (Split-Path -Parent $binDirectory)) |
    Where-Object { (Test-Path -LiteralPath (Join-Path $_ "LICENSE")) -and (Test-Path -LiteralPath (Join-Path $_ "README.txt")) } |
    Select-Object -First 1
if (-not $packageRoot) {
    throw "The FFmpeg distribution must include LICENSE and README.txt beside its executables or parent bin folder."
}
$versionOutput = & $ffmpeg -version 2>&1 | Out-String

if ($LASTEXITCODE -ne 0) {
    throw "The selected FFmpeg binary could not be executed."
}
if ($versionOutput -match "--enable-nonfree") {
    throw "This FFmpeg build enables nonfree components and cannot be staged for distribution."
}

New-Item -ItemType Directory -Force -Path $destination | Out-Null
Copy-Item -LiteralPath $ffmpeg -Destination (Join-Path $destination "ffmpeg.exe") -Force
Copy-Item -LiteralPath $ffprobe -Destination (Join-Path $destination "ffprobe.exe") -Force

$license = Join-Path $packageRoot "LICENSE"
$readme = Join-Path $packageRoot "README.txt"
Copy-Item -LiteralPath $license -Destination (Join-Path $destination "LICENSE.GPLv3.txt") -Force
Copy-Item -LiteralPath $readme -Destination (Join-Path $destination "BUILD_README.txt") -Force

# Release builds verify these files and their corresponding source bundle before
# staging. Keep the full third-party notices inside every installed/portable app.
foreach ($name in @('THIRD_PARTY_LICENSES.txt', 'SOURCES.lock', 'TOOLCHAIN.txt', 'SHA256SUMS.txt')) {
    $source = Join-Path $packageRoot $name
    if (Test-Path -LiteralPath $source) {
        $stagedName = if ($name -eq 'SHA256SUMS.txt') { 'BUILD_PACKAGE_SHA256SUMS.txt' } else { $name }
        Copy-Item -LiteralPath $source -Destination (Join-Path $destination $stagedName) -Force
    }
}

$ffmpegSize = [math]::Round((Get-Item -LiteralPath $ffmpeg).Length / 1MB, 1)
$ffprobeSize = [math]::Round((Get-Item -LiteralPath $ffprobe).Length / 1MB, 1)
$versionLine = ($versionOutput -split "`r?`n")[0]
Write-Output "Staged $versionLine"
Write-Output "ffmpeg.exe: $ffmpegSize MB; ffprobe.exe: $ffprobeSize MB"
Write-Output "Destination: $destination"
