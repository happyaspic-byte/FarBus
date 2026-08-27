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
- Six-digit pairing PIN valid for 120 seconds; one successful use consumes the PIN and rotates it; maximum five failed attempts; constant-time hash comparison.
- Device list, attach, detach, and URB require a verified bearer token bound to the TLS session principal. Hello fingerprints alone cannot list or detach.
- 256-bit random bearer tokens stored with mode `0600` on Unix; session directory mode `0700`.
- USB/IP `transfer_buffer_length` is rejected above 65,536 bytes before allocation.
- Physical devices default to `exported = false`. Broad `--export-all` excludes HID, mass-storage, hubs, and composite devices containing those interfaces.
- Sensitive devices require an exact repeated `--export BUS-ID`. A selector may name a currently unplugged topology and is applied when that device appears.
- Exclusive per-device leases reject a second client.
- Protocol frames cap payloads at 65,536 bytes and reject unknown versions, invalid enum values, truncated frames, malformed UTF-8, and trailing payload bytes.
- Each TLS session permits at most 64 concurrent in-flight URBs; additional reads receive backpressure until a slot completes.
- USB/IP management structures use fixed sizes; device count and URB payloads are bounded.
- Plain USB/IP listeners are restricted to IPv4/IPv6 loopback addresses by CLI validation and core bind checks. No plaintext remote mode exists.
- Rust workspace forbids `unsafe` code. Native `libusb` is isolated behind `rusb`.

## Residual Risks

- USB device drivers can contain kernel vulnerabilities triggered by malicious devices. FarBus cannot sandbox the OS USB stack.
- Bearer token theft by a process with access to the user's account permits device attachment until token invalidation/restart.
- PIN pairing authenticates physical possession/display access, not a centralized user identity.
- Isochronous transfers remain unsupported by the physical backend.
- USB/IP `UNLINK` is decoded into a structured `UsbipRetUnlink` and forwarded as a FarBus `UrbUnlink` on the TLS session. Safe `rusb` 0.9 has no in-flight transfer cancel API, so an already-running physical libusb call still runs until its 500 ms timeout.
- Hotplug polling uses strict libusb enumeration with physical topology plus VID:PID, revokes leases, invalidates queued URBs when removal is observed, and preserves inventory on scan failure. Startup-only sysfs fallback can still omit entries whose attributes are unreadable. An identical replacement cannot be distinguished without a serial number.
- Session and identity files are written to a mode-restricted temp file and renamed into place. Windows does not use DPAPI.
- Network latency can violate USB device timing expectations.

## Out of Scope

- Protection from a compromised server or client operating system
- Multi-tenant enterprise RBAC and centralized revocation
- Secure boot or driver signing for third-party USB/IP kernel drivers
- Isolation of malicious USB firmware from host kernel drivers
