# Architecture and threat model

## Overview

RTSPS Secure is an OBS input source implemented in Rust. It does not implement
a video decoder. Instead, it terminates TLS, validates the camera certificate,
and forwards decrypted interleaved RTSP over TCP to OBS's private
`ffmpeg_source` instance on loopback.

```text
┌──────────┐    RTSPS / TLS     ┌────────────────────┐   RTSP / TCP      ┌──────────────────┐
│  Camera  │ ◄────────────────► │ Rust TLS bridge    │ ◄───────────────► │ OBS media source │
└──────────┘   pinned leaf cert  │ 127.0.0.1:ephemeral│   loopback only   └──────────────────┘
                                 └────────────────────┘
                                           ▲
                                           │ certificate discovery
                                 ┌────────────────────┐
                                 │ OBS source UI      │
                                 │ manual fingerprint │
                                 └────────────────────┘
```

## Components

| Component | Responsibility |
| --- | --- |
| `src/lib.rs` | OBS source lifecycle, properties, certificate approval UI, child media source |
| `src/proxy.rs` | Loopback RTSP listener, pinned TLS connection, bidirectional relay, RTSP response guard |
| `src/discovery.rs` | Untrusted certificate retrieval, fingerprint display, PEM persistence |
| `src/audio.rs` | Lock-safe child audio forwarding to the parent OBS source |
| `src/obs.rs` | Minimal manually declared OBS 33 C ABI used by the plugin |
| `packaging/windows` | NSIS installer with backup and uninstall behavior |

## Trust boundaries

1. **Camera to Rust bridge:** untrusted network. TLS is mandatory. The exact
   leaf certificate must match the selected PEM, be valid at the current time,
   and prove possession of its private key through the TLS handshake signature.
2. **Rust bridge to OBS:** local process boundary. RTSP is plaintext on an
   ephemeral `127.0.0.1` port. Other local processes with sufficient privileges
   may be able to connect.
3. **Certificate discovery:** an unauthenticated TLS handshake captures the
   presented leaf certificate. No RTSP request, credentials, or application
   data is sent. The certificate is never trusted automatically.
4. **User confirmation:** the displayed SHA-256 fingerprint must be compared
   with information obtained through an independent channel. Approval writes
   the exact DER certificate to a PEM file and updates the source setting.

## Security invariants

- Only `rtsps://` URLs with numeric IP hosts are accepted.
- URLs containing a username, password, or fragment are rejected.
- The PEM must contain exactly one X.509 server certificate.
- A certificate is accepted only if it equals the pinned leaf byte-for-byte and
  is valid at the current time.
- TLS 1.2 and TLS 1.3 handshake signatures are verified with the configured
  crypto provider.
- Server responses with RTSP redirects are rejected.
- `Content-Base`, `Content-Location`, and `a=control:` values must remain on
  the loopback bridge.
- Non-interleaved transports are rejected; RTP and RTCP must travel inside the
  RTSP TCP connection.
- The listener accepts only loopback clients.

## Certificate lifecycle

1. The user enters the camera URL.
2. **Fetch device certificate** starts a background TLS handshake to the
   numeric IP and port.
3. The custom verifier captures the leaf certificate and validates its date
   range. It does not send application data.
4. OBS displays the SHA-256 fingerprint. The approval button remains disabled
   until a certificate is available.
5. On approval, the URL's current IP and port are compared with the discovery
   target. A certificate for a different endpoint is rejected.
6. The PEM is stored under the OBS module configuration directory:
   `plugin_config/rtsps_secure/certificates/<fingerprint>.pem`.
7. The source updates its settings and starts the pinned TLS bridge.

If the camera changes its certificate, the bridge rejects the new leaf until
the user repeats verification.

## Concurrency and OBS ABI

OBS owns the parent source until the destroy callback returns. The plugin stops
and joins certificate discovery and the TLS bridge before releasing the child
media source. Audio callbacks copy planar float samples into a bounded queue;
a separate worker forwards them to OBS so no child lock is held while the parent
source is queried. Dropping audio under backpressure is preferred over blocking
OBS's media thread.

The OBS ABI is declared manually in `src/obs.rs`. Field order and structure
sizes are tied to OBS 33. Any OBS update can change this ABI; do not assume that
a successful compile proves compatibility.

## Non-goals

- Replacing OBS's decoder or bundling FFmpeg.
- Authenticating camera users or storing credentials.
- Hiding plaintext RTSP from other processes on the local machine.
- Providing a VPN, firewall, or network-isolation solution.
- Supporting every camera vendor or RTSP extension.

## Review checklist

When changing security-sensitive code, verify:

- pinning still compares the complete leaf certificate;
- validity and handshake signatures are still checked;
- no path can bypass the loopback bridge;
- discovery cannot implicitly approve a certificate;
- locks are not held while calling into OBS or joining workers;
- failures release every OBS resource and stop every thread;
- tests cover the negative path as well as the successful path.
