# Testing

## Goals

Testing must demonstrate that the plugin:

- loads in OBS 33 without destabilizing the application;
- rejects untrusted or mismatched certificates;
- produces observable video and audio through OBS;
- survives repeated certificate-button clicks and settings changes;
- shuts down cleanly without hung threads or use-after-free;
- installs, upgrades, and uninstalls without touching unrelated files.

## Automated checks

The CI workflow runs:

| Platform | Checks |
| --- | --- |
| Windows x64 | `cargo fmt --check`, release build with path remapping, unit tests against OBS 33.0.1, Clippy with `-D warnings`, installer and ZIP packaging, installer smoke test |
| Ubuntu 24.04 | Release build and unit tests against the distribution libobs |
| macOS | Release build with dynamic libobs lookup |
| Dependency audit | RustSec advisory database check against `Cargo.lock` |

Linux and macOS jobs prove compilation only. They do not load the plugin into
OBS, play media, or validate the OBS 33 ABI. No Linux or macOS package is
published from CI.

## Rust unit tests

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;C:\msys64\mingw64\bin;C:\Program Files\obs-studio\bin\64bit;$env:Path"
cargo +stable-x86_64-pc-windows-gnu test --release --locked --target x86_64-pc-windows-gnu
cargo +stable-x86_64-pc-windows-gnu clippy --locked --all-targets --target x86_64-pc-windows-gnu -- -D warnings
```

Covered behavior includes URL validation, credential rejection, certificate
fingerprints, approval preconditions, fragmented RTSP forwarding, redirect
blocking, and external SDP control rejection.

Run the dependency audit locally with:

```powershell
cargo install cargo-audit --locked
cargo audit --file Cargo.lock
```

## Synthetic integration tests

`tools/synthetic_camera_test.py` starts a loopback-only RTSPS server and a
synthetic H.264/AAC publisher. It does not access a real camera or microphone.

Prerequisites:

- Windows x64 and OBS 33;
- FFmpeg and ffprobe on `PATH`;
- Python 3 with `cryptography`;
- MediaMTX test binary at the path configured in the script;
- the disposable portable OBS copy expected by the test harness.

Run the full suite:

```powershell
python tools\synthetic_camera_test.py
```

Focused runs:

```powershell
python tools\synthetic_camera_test.py --discovery-only
python tools\synthetic_camera_test.py --gui-cert-only
python tools\synthetic_camera_test.py --negative-only
```

The GUI test drives the real Qt properties window through UI Automation. It
matches the plugin's English labels and expects an English OBS UI; update the
selectors if you run it with a localized OBS installation. It verifies:

- the fetch button can be invoked repeatedly without crashing OBS;
- the SHA-256 fingerprint is displayed;
- the approval button stays disabled until discovery completes;
- approval persists the expected certificate;
- invalid URLs show an error without closing OBS;
- a mismatched certificate is rejected;
- OBS remains responsive and exits cleanly.

The portable OBS copy is disposable and must not contain personal scenes or
certificates.

## Installer tests

Build an isolated test installer with `TEST_SANDBOX` and run it against a
temporary directory. Sandbox packaging writes to a separate test ZIP so it can
never modify the production archive or invalidate its published checksum. The
test must confirm:

1. a previous DLL is backed up before replacement;
2. the installed DLL hash matches the release build;
3. the manifest and notices are present;
4. uninstall removes only installed files;
5. an unrelated sentinel file and the backup survive uninstall;
6. a running OBS process blocks installation when the normal guard is enabled.

Do not use the production installer to test upgrades on a personal OBS
installation. Use a disposable OBS profile or machine.

## Release criteria

Before publishing a release:

- all CI jobs are green on the release commit;
- the full synthetic suite passes on Windows;
- repeated GUI certificate tests pass;
- a smoke test with a real RTSPS camera confirms pinned TLS, video, audio, and a
  clean OBS shutdown without publishing the camera's address or certificate;
- the installer sandbox test passes, including the OBS-running guard;
- the all-users install path resolves to `C:\ProgramData`;
- the packaged DLL contains no local paths or private data;
- SHA-256 checksums match the uploaded assets;
- the release is marked as a prerelease when platform support is experimental;
- limitations and tested platforms are stated explicitly.

## Reporting results

Include the OBS version, plugin commit, Windows version, camera model (without
network identifiers), test command, and result. Remove tokens, private IP
addresses, certificates, and scene contents from logs.
