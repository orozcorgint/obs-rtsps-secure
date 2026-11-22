# Installation and platform support

## Requirements

- **Windows x64** with OBS Studio **33.x** for the supported build.
- A camera that exposes **RTSPS** and supports **interleaved RTSP over TCP**.
- A PEM file containing the camera's exact server certificate.

This project is experimental. Keep a backup of your OBS scenes and configuration
before installing any third-party plugin.

## Windows installer

1. Close every OBS window, including portable and background instances.
2. Download `rtsps-secure-<version>-windows-x64-setup.exe` and
   `SHA256SUMS.txt` from [Releases](https://github.com/orozcorgint/obs-rtsps-secure/releases).
3. Verify the installer checksum in PowerShell:

   ```powershell
   Get-FileHash .\rtsps-secure-0.1.0-windows-x64-setup.exe -Algorithm SHA256
   ```

   Compare the result with the matching line in `SHA256SUMS.txt`.
4. Run the installer and approve the administrator prompt.
5. Start OBS normally. Confirm that **RTSPS Secure** appears under
   **Sources → +**.

Default installation path:

```text
C:\ProgramData\obs-studio\plugins\rtsps_secure\
```

The installer is not code-signed. Windows SmartScreen may display an
unknown-publisher warning. Do not disable system-wide security protections; if
you do not trust the download, verify the published checksum and review the
source before running it.

### Upgrade behavior

If `rtsps_secure.dll` already exists, the installer copies it to:

```text
C:\ProgramData\obs-studio\plugin-backups\rtsps_secure\
```

The existing DLL is replaced only after the backup succeeds. Scenes,
certificate files, and other plugin files are never modified.

### Uninstall

Use **Settings → Apps → Installed apps → RTSPS Secure for OBS**, or run
`C:\ProgramData\obs-studio\plugins\rtsps_secure\Uninstall.exe`.

The uninstaller removes only the DLL, manifest, license, notices, and uninstall
executable. It intentionally keeps camera certificates, OBS scenes, and plugin
backups.

## Portable OBS

For OBS 33 in portable mode, copy the ZIP's `rtsps_secure` folder into:

```text
<portable OBS root>\plugins\rtsps_secure\
```

Expected layout:

```text
<portable OBS root>\plugins\rtsps_secure\rtsps_secure.dll
<portable OBS root>\plugins\rtsps_secure\data\manifest.json
```

Use one installation method only. Do not mix a system-wide installation with a
portable copy of the same DLL.

## Manual ZIP installation

The ZIP contains a single `rtsps_secure` directory. Copy it into
`C:\ProgramData\obs-studio\plugins\`. If an older DLL exists, back it up first.
Do not replace files while OBS is running.

## Linux

There is **no supported Linux package** at this time.

The CI pipeline compiles the crate on Ubuntu with libobs development headers,
but this only proves that the code compiles and links. It does not validate the
OBS 33 ABI, GUI behavior, audio, video, or shutdown. Distribution packages such
as Debian and Ubuntu may ship older libobs versions that are incompatible with
OBS 33 plugins.

The standard per-user location for a native Linux plugin is
`~/.config/obs-studio/plugins/<plugin>/bin/64bit/<plugin>.so`, but no `.so`
artifact is distributed until a real OBS 33 runtime test passes.

### Flatpak

Flatpak is **not supported**. Its sandbox and OBS extension mechanism differ
from a native installation. Do not copy a system build into a Flatpak sandbox.

## macOS

There is **no supported macOS package** at this time.

CI performs a compile-only build using dynamic libobs symbol lookup. A
distributable release requires:

- a `.plugin` bundle with the correct architecture;
- validation on Apple Silicon and/or Intel hardware;
- loading, playback, GUI, audio, and clean-shutdown tests in OBS 33;
- code signing and notarization for Gatekeeper.

Until those checks are complete, no macOS bundle is published.

## Troubleshooting installation

- **Source does not appear:** verify the DLL path and restart OBS in normal
  mode, not safe mode. Safe mode does not load third-party plugins.
- **Plugin manager reports a load failure:** confirm OBS is version 33 x64.
  Check the OBS log for `[RTSPS Secure]` messages.
- **Installer says OBS is running:** close all OBS windows, including a
  portable copy. The installer never terminates OBS automatically.
- **Windows warns about the publisher:** the package is unsigned. Verify the
  checksum and source before continuing.
