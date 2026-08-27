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

M0 does not forward USB traffic and has no production network listener. Future prereleases remain unsuitable for exposure to untrusted networks until their release notes explicitly state that TLS pairing and security review are complete.
