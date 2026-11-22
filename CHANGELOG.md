# Changelog

All notable changes are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-21

### Added

- Initial experimental RTSPS source for OBS Studio 33 on Windows x64.
- Rust TLS bridge with exact leaf-certificate pinning and certificate validity
  checks.
- Interactive certificate discovery with SHA-256 fingerprint confirmation.
- Video and audio forwarding through OBS's built-in media source.
- Windows installer and portable ZIP with backup, checksum, and uninstall
  support.
- English documentation, security policy, contribution guide, issue templates,
  and CI builds for Windows, Linux, and macOS.

### Fixed

- Deadlock when OBS queried source dimensions while audio was being delivered.
- Crash when certificate properties changed while the OBS property dialog was
  handling a button click.
- Relative certificate paths in portable OBS installations are now saved as
  absolute paths.

### Security

- RTSP redirects and SDP control URLs outside the loopback bridge are rejected.
- Credentials in URLs, non-numeric hosts, plaintext RTSP, and non-interleaved
  transports are rejected.

### Known limitations

- Windows x64 is the only runtime-tested platform.
- Linux and macOS are build-only experiments.
- No camera credentials, DNS hostnames, RTSP over UDP, or Flatpak packaging.
- Initial certificate trust requires manual fingerprint verification.

[Unreleased]: https://github.com/orozcorgint/obs-rtsps-secure/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/orozcorgint/obs-rtsps-secure/releases/tag/v0.1.0
