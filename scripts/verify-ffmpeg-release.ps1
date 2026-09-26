param(
    [Parameter(Mandatory = $true)][string]$SourceDirectory,
    [Parameter(Mandatory = $true)][string]$SourceArchive
)

$ErrorActionPreference = "Stop"
$packageRoot = (Resolve-Path -LiteralPath $SourceDirectory).Path
$archivePath = (Resolve-Path -LiteralPath $SourceArchive).Path
$expectedName = 'FitSend-FFmpeg-8.0.1-fitsend1-sources.tar.gz'
if ([System.IO.Path]::GetFileName($archivePath) -cne $expectedName) {
    throw "Expected the matching source bundle named $expectedName."
}
$expectedFiles = @('bin/ffmpeg.exe', 'bin/ffprobe.exe', 'LICENSE', 'README.txt', 'THIRD_PARTY_LICENSES.txt', 'SOURCES.lock', 'TOOLCHAIN.txt')
$manifestPath = Join-Path $packageRoot 'SHA256SUMS.txt'
$manifest = Get-Content -LiteralPath $manifestPath
$seen = @{}
foreach ($line in $manifest) {
    if ($line -notmatch '^([a-f0-9]{64})  (.+)$') { throw 'Malformed FFmpeg package checksum manifest.' }
    $hash = $Matches[1]
    $relativePath = $Matches[2]
    if ($relativePath -cnotin $expectedFiles -or $seen.ContainsKey($relativePath)) {
        throw "Unexpected or duplicate FFmpeg package entry: $relativePath"
    }
    $seen[$relativePath] = $true
    $actual = (Get-FileHash -LiteralPath (Join-Path $packageRoot $relativePath) -Algorithm SHA256).Hash
    if ($actual -ine $hash) { throw "FFmpeg package checksum mismatch: $relativePath" }
}
if ($seen.Count -ne $expectedFiles.Count) { throw 'Incomplete FFmpeg package checksum manifest.' }

# The source bundle must identify exactly these binaries, and contain all pinned
# source archives plus the recipe. tar only reads named text members here.
$sourceManifest = @(& tar -xOf $archivePath 'materials/binary-SHA256SUMS.txt')
if ($LASTEXITCODE -ne 0 -or ($sourceManifest -join "`n") -cne ($manifest -join "`n")) {
    throw 'This source bundle does not correspond to the selected FFmpeg package.'
}
$sourceLock = @(& tar -xOf $archivePath 'materials/recipe/sources.lock')
if ($LASTEXITCODE -ne 0 -or ($sourceLock -join "`n") -cne ((Get-Content -LiteralPath (Join-Path $packageRoot 'SOURCES.lock')) -join "`n")) {
    throw 'FFmpeg source manifests do not match.'
}
if (($sourceLock -join "`n") -cne ((Get-Content -LiteralPath (Join-Path $PSScriptRoot 'ffmpeg/sources.lock')) -join "`n")) {
    throw 'The FFmpeg source bundle does not use this release recipe lock.'
}

function Get-SourceMemberHash {
    param([string]$Member)
    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    $startInfo.FileName = 'tar.exe'
    $startInfo.Arguments = "-xOf `"$archivePath`" $Member"
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $startInfo
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        [void]$process.Start()
        # Hash raw bytes; PowerShell's text pipeline would corrupt ZIP/gzip data.
        $digest = $sha.ComputeHash($process.StandardOutput.BaseStream)
        $process.WaitForExit()
        if ($process.ExitCode -ne 0) { throw "Could not read source archive member $Member." }
        return [System.BitConverter]::ToString($digest).Replace('-', '').ToLowerInvariant()
    } finally {
        $sha.Dispose()
        $process.Dispose()
    }
}
$members = @(& tar -tf $archivePath)
if ($LASTEXITCODE -ne 0) { throw 'Could not inspect the FFmpeg source bundle.' }
$requiredMembers = @('materials/recipe/build-windows.sh', 'materials/recipe/mingw.ini', 'materials/recipe/README.md', 'materials/config/config.mak', 'materials/config/config.h', 'materials/toolchain.txt')
foreach ($line in $sourceLock) {
    if (-not $line -or $line.StartsWith('#')) { continue }
    $parts = $line.Split('|')
    if ($parts.Length -ne 4 -or $parts[1] -notmatch '^[a-z0-9]+\.(zip|tar\.gz)$' -or $parts[2] -notmatch '^[a-f0-9]{64}$') {
        throw 'Malformed FFmpeg source lock.'
    }
    $requiredMembers += "sources/$($parts[1])"
    if ((Get-SourceMemberHash "sources/$($parts[1])") -cne $parts[2]) {
        throw "Corresponding source checksum mismatch: $($parts[1])"
    }
}
foreach ($member in $requiredMembers) {
    if ($member -cnotin $members) { throw "FFmpeg source bundle is missing $member." }
}
Write-Output 'Verified FFmpeg package hashes, matching source provenance, recipe, and every dependency source archive hash.'
