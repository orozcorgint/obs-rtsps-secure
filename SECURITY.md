# Security policy

## Project status

RTSPS Secure is experimental software and has not undergone an independent
security audit. The project pinning model protects against a changed or
unexpected server certificate after verification, but it cannot establish trust
during a first connection without an independent fingerprint check.

## Supported versions

Security fixes are applied to the latest published prerelease. Older builds and
source snapshots are not supported. Check the [latest release](https://github.com/orozcorgint/obs-rtsps-secure/releases)
before reporting an issue.

## Reporting a vulnerability

Use a [private GitHub security advisory](https://github.com/orozcorgint/obs-rtsps-secure/security/advisories/new)
when available. If private advisories are disabled, open an issue containing
only a request for a private contact channel; do not include exploit details.

Please include:

- affected version, OBS version, and operating system;
- a minimal reproduction and the observed impact;
- whether the issue affects confidentiality, integrity, or availability;
- any relevant logs or certificates with secrets and personal data removed.

Do not publish details that could enable exploitation before a fix has been
coordinated. No response deadline or bug bounty is offered.

## Scope

In scope:

- bypasses of exact certificate pinning, validity checks, or TLS signature
  verification;
- acceptance of redirects or SDP control URLs that escape the loopback bridge;
- memory-safety or denial-of-service bugs in the Rust bridge;
- installer behavior that overwrites unrelated files or modifies OBS scenes.

Out of scope:

- attacks requiring control of the local machine or an administrator account;
- cameras that expose plaintext RTSP or weak credentials outside this plugin;
- vulnerabilities in OBS Studio, FFmpeg, Windows, or camera firmware;
- social-engineering scenarios that rely on a user confirming an unverified
  fingerprint.

## Sensitive data

Never submit passwords, tokens, private keys, personal certificates, real IP
addresses, complete OBS scenes, or unredacted logs. Replace them with synthetic
values that still demonstrate the issue.
