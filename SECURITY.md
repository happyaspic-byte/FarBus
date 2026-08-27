# Security Policy

## Supported versions

FarBus has not reached its first supported release. Security fixes currently target the latest `develop` branch.

## Reporting a vulnerability

Use GitHub's private vulnerability reporting feature in the repository's **Security** tab. Do not disclose suspected vulnerabilities in public issues, discussions, pull requests, or logs.

Include:

- affected commit or release
- operating systems and architectures
- USB device class and VID/PID when relevant
- minimal reproduction steps
- security impact
- sanitized logs without USB payloads, private keys, pairing PINs, or credentials

You should receive an initial response within seven days. No bounty is currently offered.

## Current warning

FarBus now forwards USB traffic over TLS 1.3, but remains pre-1.0. Restrict deployments to trusted LANs until independent security review and signed installers are complete. See [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md) for controls and residual risks.
