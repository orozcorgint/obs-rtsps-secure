# Development helper only. Use the release installer or ZIP for normal installation.
$ErrorActionPreference = 'Stop'

$dll = Join-Path $PSScriptRoot 'target\x86_64-pc-windows-gnu\release\rtsps_secure.dll'
$destination = Join-Path $env:ProgramData 'obs-studio\plugins\rtsps_secure'

if (Get-Process obs64 -ErrorAction SilentlyContinue) {
    throw 'Close all OBS windows before installing the plugin. OBS will not be closed automatically.'
}
if (-not (Test-Path -LiteralPath $dll -PathType Leaf)) {
    throw "The compiled plugin was not found: $dll"
}
if (Test-Path -LiteralPath $destination) {
    throw "The destination already exists and will not be overwritten: $destination"
}

New-Item -ItemType Directory -Path (Join-Path $destination 'data') -Force | Out-Null
Copy-Item -LiteralPath $dll -Destination (Join-Path $destination 'rtsps_secure.dll')
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'data\manifest.json') -Destination (Join-Path $destination 'data\manifest.json')
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'LICENSE') -Destination (Join-Path $destination 'LICENSE.txt')
Write-Host "Installed: $destination\rtsps_secure.dll"
Write-Host 'Start OBS normally, then add the RTSPS Secure source.'
