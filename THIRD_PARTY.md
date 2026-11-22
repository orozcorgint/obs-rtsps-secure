# Third-party notices

RTSPS Secure is distributed under the MIT License. It links Rust crates whose
package names and exact versions are pinned in `Cargo.lock`; each crate retains
its own copyright and license. The release package includes
`THIRD_PARTY_NOTICES.txt`, generated from the locked dependency graph by
`tools/collect_licenses.py`.

Direct dependencies include:

| Crate | Purpose | Typical license |
| --- | --- | --- |
| `rustls` | TLS implementation | Apache-2.0 OR ISC OR MIT |
| `ring` | Cryptography provider | ISC-style |
| `url` | URL parsing | Apache-2.0 OR MIT |
| `x509-parser` | Certificate parsing | Apache-2.0 OR MIT |
| `base64` | PEM certificate encoding | Apache-2.0 OR MIT |
| `rustls-pemfile` | PEM reading | Apache-2.0 OR ISC OR MIT |

The table is a summary, not a replacement for the generated notices. When
redistributing binaries, preserve all copyright and license texts required by
the exact dependency versions you ship.

OBS Studio and its components are separate works under their own licenses.
NSIS, used to build the Windows installer, is also licensed separately. The
source repository does not include OBS, FFmpeg, MediaMTX, camera firmware, or
private certificates.
