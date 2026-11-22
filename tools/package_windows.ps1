param(
    [Parameter(Mandatory=$true)][string]$MakeNSIS,
    [switch]$SandboxTest
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$dll = Join-Path $root 'target\x86_64-pc-windows-gnu\release\rtsps_secure.dll'
$dist = Join-Path $root 'dist'
$stageRoot = if ($SandboxTest) { Join-Path $dist 'windows-zip-test' } else { Join-Path $dist 'windows-zip' }
$stage = Join-Path $stageRoot 'rtsps_secure'
if (-not (Test-Path -LiteralPath $dll -PathType Leaf)) { throw 'Build the Windows x64 release first.' }
if (-not (Test-Path -LiteralPath $MakeNSIS -PathType Leaf)) { throw 'NSIS makensis.exe not found.' }
# Release binaries must not accidentally disclose the builder's home or project
# path (which Rust panic messages can otherwise embed, even with symbols stripped).
$bytes = [IO.File]::ReadAllBytes($dll)
$binary = [Text.Encoding]::GetEncoding(28591).GetString($bytes)
foreach ($privatePath in @([Environment]::GetFolderPath('UserProfile'), $root)) {
    if ($privatePath -and ($binary.IndexOf($privatePath, [StringComparison]::OrdinalIgnoreCase) -ge 0 -or
        $binary.IndexOf($privatePath.Replace('\','/'), [StringComparison]::OrdinalIgnoreCase) -ge 0)) {
        throw 'DLL contains a local path. Rebuild with tools/build_release_windows.ps1 before packaging.'
    }
}
New-Item -ItemType Directory -Path (Join-Path $stage 'data') -Force | Out-Null
Copy-Item -LiteralPath $dll -Destination (Join-Path $stage 'rtsps_secure.dll')
Copy-Item -LiteralPath (Join-Path $root 'data\manifest.json') -Destination (Join-Path $stage 'data\manifest.json')
Copy-Item -LiteralPath (Join-Path $root 'LICENSE') -Destination (Join-Path $stage 'LICENSE.txt')
$notices = Join-Path $stage 'THIRD_PARTY_NOTICES.txt'
& python (Join-Path $root 'tools\collect_licenses.py') $notices
if ($LASTEXITCODE -ne 0) { throw 'License collection failed.' }
$zip = if ($SandboxTest) {
    Join-Path $dist 'rtsps-secure-test.zip'
} else {
    Join-Path $dist 'rtsps-secure-0.1.0-windows-x64.zip'
}
Compress-Archive -Path $stage -DestinationPath $zip -Force
$exe = if ($SandboxTest) { Join-Path $dist 'rtsps-secure-test-installer.exe' } else { Join-Path $dist 'rtsps-secure-0.1.0-windows-x64-setup.exe' }
$defines = @(
    "/DPLUGIN_DLL=$dll"
    "/DMANIFEST=$(Join-Path $root 'data\manifest.json')"
    "/DLICENSE_FILE=$(Join-Path $root 'LICENSE')"
    "/DNOTICES=$notices"
    "/DOUTPUT_FILE=$exe"
)
if ($SandboxTest) { $defines += '/DTEST_SANDBOX'; $defines += '/DTEST_NO_GUARD' }
& $MakeNSIS '/WX' @defines (Join-Path $root 'packaging\windows\installer.nsi')
if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed.' }
$hashes = Get-FileHash -Algorithm SHA256 -LiteralPath $zip,$exe
$checksumFile = if ($SandboxTest) { 'SHA256SUMS-test.txt' } else { 'SHA256SUMS.txt' }
$lines = @($hashes | ForEach-Object { "$($_.Hash.ToLowerInvariant())  $([IO.Path]::GetFileName($_.Path))" })
[IO.File]::WriteAllLines((Join-Path $dist $checksumFile), $lines, [Text.UTF8Encoding]::new($false))
$hashes | Format-Table -AutoSize
