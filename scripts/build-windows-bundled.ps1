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

    $artifacts = @($msi, $setup, $portableZip)
    foreach ($artifact in $artifacts) {
        if (-not (Test-Path -LiteralPath $artifact -PathType Leaf) -or (Get-Item -LiteralPath $artifact).Length -eq 0) {
            throw "Expected a non-empty release artifact: $artifact"
        }
    }
    $checksumLines = $artifacts |
        Sort-Object { [System.IO.Path]::GetFileName($_) } |
        ForEach-Object {
            $hash = (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()
            "$hash  $([System.IO.Path]::GetFileName($_))"
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
    Get-Item -LiteralPath $msi,$setup,$portableZip,$checksumPath | Select-Object Name,Length
} finally {
    Pop-Location
}
