# Contributing

Contributions and bug reports are welcome. The goal is to improve an
experimental RTSPS source for OBS without weakening TLS verification or
breaking existing scenes.

## Before you start

- Use synthetic test data. Do not commit camera certificates, private keys,
  private IP addresses, OBS scenes, tokens, or unredacted logs.
- Describe the problem and the smallest reproducible case. Report security
  issues through [SECURITY.md](SECURITY.md), not a public issue.
- OBS exposes a C ABI. Check field order, structure sizes, and function
  signatures against the matching OBS headers before changing FFI code.
- Do not change the source ID `rtsps_secure_source` or the property keys
  without a migration plan. Existing scenes store those values.
- Test changes in an isolated OBS profile or a disposable portable copy. A
  successful compile is not evidence that a platform works at runtime.

## Development setup

### Windows x64

Install stable Rust with the GNU target, MinGW-w64, Python, and OBS Studio 33.
Add both Rust and MinGW to `PATH`:

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu --component rustfmt --component clippy
$env:Path = "$env:USERPROFILE\.cargo\bin;C:\msys64\mingw64\bin;$env:Path"
cargo +stable-x86_64-pc-windows-gnu fmt -- --check
./tools/build_release_windows.ps1
cargo +stable-x86_64-pc-windows-gnu test --release --locked --target x86_64-pc-windows-gnu
cargo +stable-x86_64-pc-windows-gnu clippy --locked --all-targets --target x86_64-pc-windows-gnu -- -D warnings
```

The tests link against `obs.dll`; add `C:\Program Files\obs-studio\bin\64bit`
to `PATH` before running them.

### Linux and macOS

Linux requires a C compiler and libobs development headers. macOS uses dynamic
symbol lookup at load time. Both are compile-only experiments in CI; they do
not prove runtime compatibility and do not produce release packages.

## Packaging

Distribution builds must use `tools/build_release_windows.ps1`, which remaps
local source paths embedded by Rust. To build the installer and ZIP, install
NSIS 3 and Python, then run:

```powershell
./tools/package_windows.ps1 -MakeNSIS 'C:\Program Files (x86)\NSIS\makensis.exe'
```

Inspect `dist/` and `SHA256SUMS.txt` before publishing. Never publish a build
that has not been tested on the platform it targets. `install.ps1` is a
development helper that installs a local build into `ProgramData`; it is not the
supported end-user installation path and refuses to overwrite an existing
installation.

## Tests

See [docs/TESTING.md](docs/TESTING.md). The synthetic and GUI tests are Windows
integration tests: they require FFmpeg, Python with `cryptography`, a local
MediaMTX binary, and a disposable portable OBS copy. They use loopback ports
only and do not need access to a real camera.

## Pull requests

- Keep changes focused and explain the behavior they change.
- Add or update tests for functional changes.
- State exactly which platforms and OBS versions you tested.
- Update documentation when behavior, limitations, or packaging changes.
- Confirm that no secrets, personal data, or generated binaries are included.

By contributing, you agree that your changes are licensed under the project's
MIT License and that you have the right to submit them.
