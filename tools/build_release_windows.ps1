$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$homeDir = [Environment]::GetFolderPath('UserProfile')
if (-not $homeDir) { throw 'Cannot determine the user profile path for path remapping.' }
# Cargo's unit separator permits spaces in paths, unlike plain RUSTFLAGS.
# Remap both project and dependency source paths in the distributable DLL.
$env:CARGO_ENCODED_RUSTFLAGS = @(
    "--remap-path-prefix=$homeDir=/build/user"
    "--remap-path-prefix=$root=/build/project"
) -join [char]0x1f
& cargo +stable-x86_64-pc-windows-gnu build --release --locked --target x86_64-pc-windows-gnu
if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
Write-Host 'Windows release build completed with local source paths remapped.'
