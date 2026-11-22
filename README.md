# RTSPS Secure for OBS

[![CI](https://github.com/orozcorgint/obs-rtsps-secure/actions/workflows/ci.yml/badge.svg)](https://github.com/orozcorgint/obs-rtsps-secure/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/orozcorgint/obs-rtsps-secure?include_prereleases&label=release)](https://github.com/orozcorgint/obs-rtsps-secure/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

An experimental OBS Studio source that receives RTSPS video and audio through a
Rust TLS bridge with **exact certificate pinning**. The bridge verifies the
camera's leaf certificate, then hands decrypted, interleaved RTSP over TCP to
OBS's built-in media source on loopback. No external FFmpeg executable is
required.

> **Project status:** experimental and not security-audited. Windows x64 with
> OBS Studio 33 is the only tested runtime. Linux and macOS builds are
> compile-only experiments and are not distributed. This project is not
> affiliated with or endorsed by the OBS Project.

## Why this exists

OBS's built-in media source can play RTSP, but it does not provide explicit
certificate pinning for self-signed cameras. RTSPS Secure adds a narrow,
auditable layer:

- TLS terminates in Rust before OBS sees the stream.
- The exact server leaf certificate must match the selected PEM file.
- Certificate validity dates and TLS handshake signatures are checked.
- A newly discovered certificate is shown with its SHA-256 fingerprint and is
  never trusted until the user explicitly confirms it.
- RTSP redirects and SDP control URLs that would bypass the TLS bridge are
  rejected.

## Compatibility

| Platform | Status |
| --- | --- |
| Windows x64, OBS Studio 33.0.1 | Tested with synthetic RTSPS media and an Android IP Camera. |
| Windows ARM64 | Not tested; no package is provided. |
| Linux x64 | Build-only CI experiment; no runtime test or package. |
| macOS (Apple Silicon or Intel) | Build-only CI experiment; no runtime test or package. |
| OBS Studio 32 or older | Not supported. |
| OBS Flatpak | Not supported. |

Compatibility depends on the camera's RTSP implementation and codecs. The
plugin requires a numeric IP address and a PEM file containing exactly one
server certificate. Only interleaved RTSP over TCP is supported.

## Install on Windows

1. Close OBS completely.
2. Download the installer or ZIP from [Releases](https://github.com/orozcorgint/obs-rtsps-secure/releases).
3. Run the installer as administrator, or copy the ZIP's `rtsps_secure` folder
   into `C:\ProgramData\obs-studio\plugins\`.
4. Start OBS normally. The source appears as **RTSPS Secure** under
   **Sources → +**.

The installer is **not code-signed**, so Windows may show an unknown-publisher
warning. Verify the downloaded SHA-256 checksum against `SHA256SUMS.txt` before
running it.

The installer backs up an existing plugin DLL before upgrading, refuses to run
while OBS is open, and never modifies scenes, certificates, or unrelated files.
Uninstalling removes only the files installed by the package; saved camera
certificates are intentionally left in place.

See [docs/INSTALL.md](docs/INSTALL.md) for portable OBS, manual installation,
upgrade, uninstall, and unsupported-platform details.

## Use

1. Open the source properties in OBS.
2. Enter an RTSPS URL with a numeric camera IP, for example
   `rtsps://192.0.2.1:8554/live` (documentation address, not a real camera).
3. Select **Fetch device certificate**. The certificate is displayed but is
   **not trusted yet**.
4. Compare its SHA-256 fingerprint with the certificate obtained directly from
   the camera through an independent channel.
5. Only if the fingerprint matches, select **I verified it: use certificate**
   and confirm with **OK**.

If the camera regenerates its certificate, OBS will reject the connection until
the new certificate is verified and selected.

### Removing a camera

A camera stream is an OBS source, not a separate plugin list. Select the source
in its scene and click **−**. If the source is referenced by other scenes,
remove it there as well. Removing a source or uninstalling the plugin does not
delete previously saved certificates.

## Security model and limitations

Read [SECURITY.md](SECURITY.md) and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
before using this project with sensitive content.

- **First-use verification is manual.** Certificate discovery does not
  authenticate the camera by itself. Without an independent fingerprint check,
  a first connection may be impersonated.
- **The local bridge is plaintext.** OBS receives RTSP over a temporary
  `127.0.0.1` port. Other local processes with sufficient privileges could
  connect to that port.
- **Pinning authenticates the server certificate, not a user.** This version
  does not support camera credentials. Do not put passwords or tokens in URLs,
  and use the plugin only on trusted networks with appropriate camera-side
  security.
- **URLs and certificate paths can appear in OBS logs and scene files.** Review
  logs and exports before sharing them.
- **The project has not been audited.** Treat it as experimental software and
  keep independent backups of OBS scenes and configuration.

## Build from source

Windows x64 requires stable Rust (GNU toolchain), MinGW-w64, and OBS 33 for the
unit-test link path:

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu --component rustfmt --component clippy
./tools/build_release_windows.ps1
cargo +stable-x86_64-pc-windows-gnu test --release --locked --target x86_64-pc-windows-gnu
cargo +stable-x86_64-pc-windows-gnu clippy --locked --all-targets --target x86_64-pc-windows-gnu -- -D warnings
```

`build_release_windows.ps1` removes local source paths from the packaged DLL.
To build the installer and ZIP, install NSIS 3 and Python, then run:

```powershell
./tools/package_windows.ps1 -MakeNSIS 'C:\Program Files (x86)\NSIS\makensis.exe'
```

The packaging script refuses to package a DLL containing local paths and
collects third-party license notices. See [CONTRIBUTING.md](CONTRIBUTING.md)
for the full development workflow and [docs/TESTING.md](docs/TESTING.md) for
the test strategy.

## Documentation

- [Installation and platforms](docs/INSTALL.md)
- [Architecture and threat model](docs/ARCHITECTURE.md)
- [Testing](docs/TESTING.md)
- [Roadmap](docs/ROADMAP.md)
- [Contributing](CONTRIBUTING.md)
- [Support](SUPPORT.md)
- [Security policy](SECURITY.md)
- [Third-party notices](THIRD_PARTY.md)
- [Changelog](CHANGELOG.md)

## License

Copyright © 2026 orozcorgint and contributors.

Released under the [MIT License](LICENSE). Third-party dependencies retain their
own licenses; see [THIRD_PARTY.md](THIRD_PARTY.md) and the notice file included
with each release.
