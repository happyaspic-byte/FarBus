# FarBus Threat Model

## Assets

- USB payloads and commands
- Server/client device identities
- Pairing PINs and bearer tokens
- Exclusive device leases
- Host USB devices, including HID and storage

## Trust Boundaries

1. **Physical USB boundary:** A USB device may be malicious. Descriptor fields, endpoint data, and timing are untrusted.
2. **Remote network boundary:** Every remote frame is untrusted until TLS and the protocol parser accept it.
3. **Local client boundary:** Standard USB/IP is exposed only on loopback. A local process can still attempt malformed USB/IP frames.
4. **OS driver boundary:** Windows `usbip-win2` and Linux `vhci-hcd` are privileged kernel components outside FarBus.

## Controls

- TLS 1.3 only, ECDSA P-256 self-signed identity, SHA-256 certificate fingerprint pinning, and real handshake signature verification.
- Six-digit pairing PIN valid for 120 seconds; maximum five verification attempts; constant-time hash comparison.
- 256-bit random bearer tokens stored with mode `0600` on Unix.
- Physical devices default to `exported = false`; `--export-all` is explicit opt-in.
- HID devices are visible in the list but cannot attach unless explicitly exported.
- Exclusive per-device leases reject a second client.
- Protocol frames cap payloads at 65,536 bytes and reject unknown versions, invalid enum values, truncated frames, malformed UTF-8, and trailing payload bytes.
- USB/IP management structures use fixed sizes; device count and URB payloads are bounded.
- Plain USB/IP listeners bind to `127.0.0.1` only. No plaintext remote mode exists.
- Rust workspace forbids `unsafe` code. Native `libusb` is isolated behind `rusb`.

## Residual Risks

- USB device drivers can contain kernel vulnerabilities triggered by malicious devices. FarBus cannot sandbox the OS USB stack.
- Bearer token theft by a process with access to the user's account permits device attachment until token invalidation/restart.
- PIN pairing authenticates physical possession/display access, not a centralized user identity.
- Isochronous transfers remain unsupported by the physical backend.
- USB/IP `UNLINK` is acknowledged, but cancellation of an already-running synchronous libusb operation is best-effort.
- Network latency can violate USB device timing expectations.

## Out of Scope

- Protection from a compromised server or client operating system
- Multi-tenant enterprise RBAC and centralized revocation
- Secure boot or driver signing for third-party USB/IP kernel drivers
- Isolation of malicious USB firmware from host kernel drivers
