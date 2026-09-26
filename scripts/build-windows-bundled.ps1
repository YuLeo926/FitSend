param(
    [string]$CargoTargetDirectory = $env:CARGO_TARGET_DIR,
    [string]$FfmpegSourceDirectory = $env:FITSEND_FFMPEG_SOURCE_DIR,
    [string]$FfmpegSourceArchive = $env:FITSEND_FFMPEG_SOURCE_ARCHIVE
)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot

function Get-ReleaseHash {
    param([string]$Path)

    try {
        return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    } catch [System.Management.Automation.CommandNotFoundException] {
        # Some Windows PowerShell hosts lose module command discovery after Tauri's build tools run.
        $stream = [System.IO.File]::OpenRead($Path)
        $sha256 = [System.Security.Cryptography.SHA256]::Create()
        try {
            return [System.BitConverter]::ToString($sha256.ComputeHash($stream)).Replace('-', '').ToLowerInvariant()
        } finally {
            $sha256.Dispose()
            $stream.Dispose()
        }
    }
}

if (-not $FfmpegSourceDirectory -or -not $FfmpegSourceArchive) {
    throw 'Release builds require FITSEND_FFMPEG_SOURCE_DIR and FITSEND_FFMPEG_SOURCE_ARCHIVE from the same source-pinned build.'
}
& (Join-Path $PSScriptRoot "verify-ffmpeg-release.ps1") -SourceDirectory $FfmpegSourceDirectory -SourceArchive $FfmpegSourceArchive
& (Join-Path $PSScriptRoot "stage-ffmpeg.ps1") -SourceDirectory (Join-Path $FfmpegSourceDirectory 'bin')

if (-not $CargoTargetDirectory) {
    $CargoTargetDirectory = Join-Path $projectRoot "src-tauri\target"
}
$env:CARGO_TARGET_DIR = $CargoTargetDirectory

Push-Location $projectRoot
try {
    & npm.cmd run tauri build
    if ($LASTEXITCODE -ne 0) { throw "Tauri build failed with exit code $LASTEXITCODE." }

    $version = (Get-Content -Raw (Join-Path $projectRoot "package.json") | ConvertFrom-Json).version
    $releaseDirectory = [System.IO.Path]::GetFullPath((Join-Path $projectRoot "release"))
    $bundleDirectory = Join-Path $CargoTargetDirectory "release\bundle"
    New-Item -ItemType Directory -Force -Path $releaseDirectory | Out-Null
    $resolvedReleaseDirectory = (Resolve-Path -LiteralPath $releaseDirectory).Path.TrimEnd('\')
    $portableName = "FitSend_$version`_portable"
    $portableDirectory = [System.IO.Path]::GetFullPath((Join-Path $resolvedReleaseDirectory $portableName))
    $portableParent = [System.IO.Directory]::GetParent($portableDirectory).FullName.TrimEnd('\')
    if (-not [string]::Equals($portableParent, $resolvedReleaseDirectory, [System.StringComparison]::OrdinalIgnoreCase) -or
        -not [string]::Equals((Split-Path -Leaf $portableDirectory), $portableName, [System.StringComparison]::Ordinal)) {
        throw "Refusing to recreate a portable directory outside the exact release directory."
    }

    if (Test-Path -LiteralPath $portableDirectory) {
        $portableItem = Get-Item -LiteralPath $portableDirectory -Force
        if (-not $portableItem.PSIsContainer -or
            ($portableItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0 -or
            -not [string]::Equals($portableItem.FullName.TrimEnd('\'), $portableDirectory.TrimEnd('\'), [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing to recreate an unexpected or redirected portable path."
        }
        Remove-Item -LiteralPath $portableDirectory -Recurse -Force
    }
    New-Item -ItemType Directory -Path $portableDirectory | Out-Null

    $msi = Join-Path $releaseDirectory "FitSend_${version}_x64_en-US.msi"
    $setup = Join-Path $releaseDirectory "FitSend_${version}_x64-setup.exe"
    Copy-Item -LiteralPath (Join-Path $bundleDirectory "msi\FitSend_${version}_x64_en-US.msi") -Destination $msi -Force
    Copy-Item -LiteralPath (Join-Path $bundleDirectory "nsis\FitSend_${version}_x64-setup.exe") -Destination $setup -Force
    Copy-Item -LiteralPath (Join-Path $CargoTargetDirectory "release\fitsend.exe") -Destination (Join-Path $portableDirectory "FitSend.exe") -Force
    Copy-Item -LiteralPath (Join-Path $projectRoot "src-tauri\resources\ffmpeg") -Destination $portableDirectory -Recurse -Force

    $portableZip = Join-Path $releaseDirectory "FitSend_${version}_portable.zip"
    if (Test-Path -LiteralPath $portableZip) { Remove-Item -LiteralPath $portableZip -Force }
    Compress-Archive -Path (Join-Path $portableDirectory "*") -DestinationPath $portableZip -CompressionLevel Optimal

    $sourceAsset = Join-Path $releaseDirectory ([System.IO.Path]::GetFileName($FfmpegSourceArchive))
    if ([System.IO.Path]::GetFullPath($FfmpegSourceArchive) -ine [System.IO.Path]::GetFullPath($sourceAsset)) {
        Copy-Item -LiteralPath $FfmpegSourceArchive -Destination $sourceAsset -Force
    }
    $artifacts = @($msi, $setup, $portableZip, $sourceAsset)
    foreach ($artifact in $artifacts) {
        if (-not (Test-Path -LiteralPath $artifact -PathType Leaf) -or (Get-Item -LiteralPath $artifact).Length -eq 0) {
            throw "Expected a non-empty release artifact: $artifact"
        }
    }
    $checksumLines = foreach ($artifact in ($artifacts | Sort-Object { [System.IO.Path]::GetFileName($_) })) {
        $hash = Get-ReleaseHash $artifact
        "$hash  $([System.IO.Path]::GetFileName($artifact))"
    }
    $checksumPath = Join-Path $releaseDirectory "SHA256SUMS.txt"
    if (Test-Path -LiteralPath $checksumPath) {
        Remove-Item -LiteralPath $checksumPath -Force
    }
    [System.IO.File]::WriteAllLines(
        $checksumPath,
        [string[]]$checksumLines,
        [System.Text.UTF8Encoding]::new($false)
    )

    Write-Output "Bundled release files:"
    Get-Item -LiteralPath $msi,$setup,$portableZip,$sourceAsset,$checksumPath | Select-Object Name,Length
} finally {
    Pop-Location
}
