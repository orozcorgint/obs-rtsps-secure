# Roadmap

This roadmap describes intent, not commitments. Security and tested behavior
take priority over adding platforms or features.

## Current: 0.1.x

- Windows x64, OBS Studio 33.
- Exact certificate pinning with manual first-use verification.
- Video and audio through OBS's built-in media source.
- Installer and portable ZIP with checksum verification.

## Next

- Expand automated compatibility testing across OBS 33 patch releases.
- Validate long-running streams, reconnection, and camera certificate rotation.
- Improve diagnostics without exposing URLs or certificate paths in logs.
- Evaluate support for camera credentials through a secure settings model.
- Validate Linux against a real OBS 33 runtime before publishing any package.
- Validate macOS Apple Silicon and Intel bundles before publishing any package.

## Explicitly deferred

- Flatpak packaging.
- Windows ARM64.
- RTSP over UDP or non-interleaved RTP/RTCP.
- Bundling FFmpeg or camera vendor libraries.
- Claims of universal camera compatibility.

Any new platform or feature must include runtime tests in OBS, negative security
tests, and documented limitations before it is described as supported.
