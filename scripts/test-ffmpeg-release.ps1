param(
    [Parameter(Mandatory = $true)][string]$SourceDirectory,
    [Parameter(Mandatory = $true)][string]$SourceArchive
)

$ErrorActionPreference = 'Stop'
$verify = Join-Path $PSScriptRoot 'verify-ffmpeg-release.ps1'
$projectRoot = Split-Path -Parent $PSScriptRoot
$fixtureRoot = Join-Path $projectRoot ("output/ffmpeg-release-tests-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixtureRoot | Out-Null

function Expect-Rejection {
    param([string]$Name, [string]$Directory, [string]$ExpectedMessage, [string]$Archive = $SourceArchive)
    $caught = $null
    try { & $verify -SourceDirectory $Directory -SourceArchive $Archive } catch { $caught = $_ }
    if (-not $caught -or $caught.ToString() -notlike "*$ExpectedMessage*") {
        throw "Test '$Name' failed: expected '$ExpectedMessage'; got '$caught'."
    }
    Write-Output "PASS: $Name"
}

& $verify -SourceDirectory $SourceDirectory -SourceArchive $SourceArchive
Write-Output 'PASS: authentic binary and corresponding source package'

$corrupt = Join-Path $fixtureRoot 'corrupt-binary'
Copy-Item -LiteralPath $SourceDirectory -Destination $corrupt -Recurse
[System.IO.File]::WriteAllText((Join-Path $corrupt 'bin/ffmpeg.exe'), 'synthetic corrupted binary')
Expect-Rejection 'modified binary' $corrupt 'checksum mismatch: bin/ffmpeg.exe'

$traversal = Join-Path $fixtureRoot 'unlisted-path'
Copy-Item -LiteralPath $SourceDirectory -Destination $traversal -Recurse
$originalLines = Get-Content -LiteralPath (Join-Path $traversal 'SHA256SUMS.txt')
$extra = ('0' * 64) + '  ../outside.exe'
[System.IO.File]::WriteAllLines((Join-Path $traversal 'SHA256SUMS.txt'), [string[]]@($originalLines + $extra))
Expect-Rejection 'unlisted path rejected before file access' $traversal 'Unexpected or duplicate'

$incomplete = Join-Path $fixtureRoot 'incomplete-manifest'
Copy-Item -LiteralPath $SourceDirectory -Destination $incomplete -Recurse
[System.IO.File]::WriteAllLines((Join-Path $incomplete 'SHA256SUMS.txt'), [string[]]@($originalLines | Select-Object -Skip 1))
Expect-Rejection 'missing manifest entry' $incomplete 'Incomplete FFmpeg package'

$mismatched = Join-Path $fixtureRoot 'wrong-provenance'
Copy-Item -LiteralPath $SourceDirectory -Destination $mismatched -Recurse
# A self-consistent package hash list is insufficient: it must also match the
# manifest stored inside the source archive produced by that exact build.
[System.IO.File]::AppendAllText((Join-Path $mismatched 'README.txt'), "`nSynthetic different build.")
$newHash = (Get-FileHash -LiteralPath (Join-Path $mismatched 'README.txt') -Algorithm SHA256).Hash.ToLowerInvariant()
$newLines = @($originalLines | ForEach-Object { if ($_ -match '  README\.txt$') { "$newHash  README.txt" } else { $_ } })
[System.IO.File]::WriteAllLines((Join-Path $mismatched 'SHA256SUMS.txt'), [string[]]$newLines)
Expect-Rejection 'source and binary package from different builds' $mismatched 'does not correspond'

$corruptSources = Join-Path $fixtureRoot 'corrupt-sources'
New-Item -ItemType Directory -Path $corruptSources | Out-Null
& tar -xf $SourceArchive -C $corruptSources sources materials
if ($LASTEXITCODE -ne 0) { throw 'Could not create source corruption test fixture.' }
[System.IO.File]::WriteAllText((Join-Path $corruptSources 'sources/zlib.tar.gz'), 'synthetic corrupted source')
$corruptArchive = Join-Path $corruptSources 'FitSend-FFmpeg-8.0.1-fitsend1-sources.tar.gz'
& tar -czf $corruptArchive -C $corruptSources sources materials
if ($LASTEXITCODE -ne 0) { throw 'Could not archive source corruption test fixture.' }
Expect-Rejection 'corrupted dependency source despite matching provenance' $SourceDirectory 'Corresponding source checksum mismatch: zlib.tar.gz' $corruptArchive

Write-Output "All 6 release gate checks passed. Synthetic fixtures retained at $fixtureRoot"
