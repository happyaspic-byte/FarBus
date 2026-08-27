# FarBus Design

## Product

FarBus is an open-source USB-over-network system for home labs. Its first supported path is a Linux server exporting physical USB devices to a Windows client. It provides unlimited devices, secure defaults, automatic LAN discovery, automatic reconnection, and useful diagnostics without exposing USB/IP setup to the user.

Product promise:

> Open-source USB sharing that works in one minute—unlimited devices, secure by default.

Release audiences expand in this order:

1. Home-lab users
2. Developers and CI labs
3. Raspberry Pi and other SBC deployments
4. Enterprise IT

## Scope

### Version 1

- Linux x86_64 and ARM64 server
- Windows 10/11 x64 client
- IPv6-first dual-stack networking with IPv4 fallback
- mDNS discovery on local networks
- One-time PIN pairing and mutual device identity
- TLS 1.3 for all remote traffic
- Device listing, attach, detach, and reconnect
- Unlimited concurrently exported devices
- Per-device data channels
- CLI-first implementation with a desktop GUI added after the data path is stable
- Bulk, control, and interrupt transfer support
- Public compatibility and benchmark results

### Experimental in Version 1

- Isochronous devices such as webcams and USB audio
- USB 3 storage using UASP
- High-latency WAN operation
- Timing-sensitive game controllers and security dongles

### Not in Version 1

- macOS client or server
- Windows server
- Android client
- Cloud account, hosted relay, or subscription
- Enterprise RBAC, audit service, 802.1X, or fleet management
- Concurrent use of one physical USB device by multiple clients

## Architecture

FarBus initially uses the Linux kernel USB/IP implementation and a compatible signed Windows virtual USB driver. FarBus adds a secure control and transport layer around that data plane.

```text
Windows USB driver
    | USB/IP over loopback
farbus-client service
    | TLS 1.3, IPv6 or IPv4
farbus-server service
    | USB/IP over loopback
Linux usbip-host
    | USB
physical device
```

This architecture is selected because writing, signing, and stabilizing a new Windows USB virtualization driver would delay useful releases. The transport boundary remains independent of USB/IP so a later native FarBus data path can replace it without changing discovery, identity, policy, CLI, or GUI.

## Components

### `farbus-protocol`

A Rust library containing versioned control messages, framing, device identifiers, capability negotiation, and transport interfaces. Wire messages have explicit size limits. Unknown protocol versions and capabilities fail closed.

### `farbus-server`

A Linux service that discovers local devices, manages `usbip-host` bindings, advertises through mDNS, authenticates clients, authorizes exports, opens per-device channels, and publishes health metrics. It never exposes the kernel USB/IP port directly to the network.

### `farbus-client`

A Windows service and CLI that discovers servers, performs pairing, maintains trusted identities, connects to devices, proxies loopback USB/IP traffic, and restores eligible connections after transient failures.

### `farbus-core`

Shared state machines for pairing, connection lifecycle, retry policy, device leases, path selection, and diagnostics. Platform operations sit behind narrow interfaces so state-machine tests run without USB hardware.

### `farbus-bench`

A reproducible benchmark tool that measures control latency, bulk throughput, CPU use, memory use, reconnect time, and packet retransmissions. Results record hardware, OS, network, USB class, VID/PID, and test parameters.

## Networking

FarBus listens on IPv6 and IPv4. Discovery publishes both address families. The client races viable paths with a Happy Eyeballs strategy and retains the best successful path rather than assuming IPv6 is faster.

IPv6 link-local addresses include a scope identifier. Server identity is bound to a certificate fingerprint, never an IP address, so DHCP and IPv6 privacy-address changes do not break trust.

The control channel persists for server state and health. Each attached USB device receives an independent data channel so bulk traffic or failure on one device cannot block another.

## Security

Encryption and authentication are mandatory; there is no plaintext remote mode.

- TLS 1.3 only
- Self-generated Ed25519 device identity on first start
- Six-digit, short-lived, rate-limited pairing PIN
- Mutual authentication after pairing
- Certificate fingerprint is the stable server identity
- Private keys use OS-protected storage where available and owner-only files on Linux
- Explicit device export policy; newly attached devices are not remotely available by default
- HID devices require an additional warning because they can inject input
- All frame lengths, device descriptors, and peer-provided strings are bounded and validated
- USB/IP listens only on loopback
- Sensitive payloads are never written to logs

Enterprise identity, RBAC, and audit retention are later additions, but the identity and authorization model supports them without a protocol reset.

## Performance

The initial implementation optimizes the existing USB/IP path before introducing a custom driver.

- Independent control and per-device data channels
- `TCP_NODELAY` for control and interrupt traffic
- Larger adaptive buffers for bulk transfers
- Fixed buffer pools instead of allocation per frame
- Scatter/gather writes where supported
- Asynchronous I/O: Tokio on Linux and Windows service user space
- TLS cipher selected from measured hardware support
- TLS session resumption for reconnects
- Compression disabled by default
- RTT, throughput, queue depth, reconnects, and retransmits exposed in diagnostics

Transfer policy depends on USB type:

| Transfer | Policy |
|---|---|
| Control | Send immediately; never batch |
| Interrupt | Prioritize latency and disable Nagle |
| Bulk | Adaptive batching and deeper queues |
| Isochronous | Experimental jitter-aware path in a later milestone |

Initial benchmark targets on wired gigabit LAN:

- HID added latency: p95 at or below 2 ms
- USB serial throughput: at least 95% of direct operation where serial line speed is the bottleneck
- USB 2 storage: 35–45 MB/s
- Reconnection after a transient network interruption: at or below 3 seconds
- Idle server memory: at or below 30 MB
- At least 16 simultaneously visible devices

These are engineering targets, not compatibility promises. Releases publish measured results and regressions block release promotion.

## Connection Lifecycle

1. Server starts and discovers USB devices.
2. Server advertises identity, addresses, protocol version, and pairing state through mDNS.
3. Client discovers the server and displays its fingerprint.
4. User enters the server's short-lived PIN.
5. Both peers store trusted identities.
6. User exports a server device and attaches it from the client.
7. Server grants an exclusive lease and opens a dedicated data channel.
8. Client imports the device through the local Windows USB/IP driver.
9. Health checks detect path or peer failure.
10. Safe devices reconnect automatically; unsafe or ambiguous state requires confirmation.

Physical removal, lease conflict, permission denial, unsupported transfer type, authentication failure, and network failure have distinct user-facing errors.

## Reliability

Connection and device state are explicit state machines. Retries use bounded exponential backoff with jitter. Reconnection never silently transfers a lease to a different client. A device reset invalidates stale in-flight requests before re-enumeration.

Crash recovery cleans stale Linux USB/IP bindings and Windows virtual attachments owned by FarBus. Failures on one device are isolated from other device channels and the control channel.

## Observability

Human-readable status and structured JSON output expose:

- Server and client identity
- Selected IPv4 or IPv6 path
- RTT and throughput
- Device lifecycle state
- Export policy and lease owner fingerprint
- Reconnect count and last failure category
- USB class, VID/PID, speed, and transfer modes

A diagnostic bundle redacts keys, PINs, payloads, host usernames, and unnecessary network identifiers.

## Testing

- Unit tests for framing, limits, identity, leases, retries, and state transitions
- Property tests for hostile and truncated wire messages
- Integration tests using a simulated USB/IP endpoint
- Linux namespace tests for IPv4-only, IPv6-only, dual-stack, packet loss, delay, and address changes
- Hardware tests for HID, serial, printer, scanner, bulk storage, hubs, and reconnect behavior
- Windows VM smoke tests for install, attach, detach, reboot, service restart, and uninstall
- Fuzzing of all remotely reachable decoders
- Performance regression tests against pinned hardware baselines

## Repository and Tooling

The repository is a Rust workspace. Rust provides memory safety for network-facing parsers, one shared protocol implementation, cross-platform async I/O, and small deployable services.

Initial workspace:

```text
crates/farbus-protocol
crates/farbus-core
crates/farbus-server
crates/farbus-client
crates/farbus-bench
```

CI runs formatting, Clippy with warnings denied, tests, dependency auditing, and cross-platform builds where possible. Driver binaries are never copied into the repository without a license and provenance review.

The intended project license is Apache-2.0 OR MIT for FarBus-authored user-space code. Integration with GPL kernel components occurs through operating-system interfaces; no Linux kernel code is copied into FarBus.

## Delivery Milestones

### M0 — Foundation

Workspace, protocol framing, identity model, state machines, CI, threat model, and simulated endpoints.

### M1 — Secure Discovery

Dual-stack listeners, mDNS, Happy Eyeballs, PIN pairing, trusted peer storage, and diagnostics.

### M2 — Linux Export

USB enumeration, explicit export policy, safe bind/unbind operations, loopback-only USB/IP, and crash recovery.

### M3 — Windows Attach

Service, CLI, compatible signed driver integration, attach/detach, installer, and reboot recovery.

### M4 — Performance and Compatibility

Per-device channels, buffer pools, adaptive bulk queues, benchmark harness, hardware matrix, and regression gates.

### M5 — Home-Lab Beta

Simple GUI, one-minute guided setup, auto reconnect, diagnostic bundle, signed releases, and upgrade path.

### M6 — Developer and SBC Expansion

Headless tokens, stable local API, CI device reservations, ARM images, system packages, and low-resource tuning.

## Success Criteria

The beta is successful when a new user can install the Linux server and Windows client, securely pair them, attach a supported USB device, survive a temporary network interruption, and diagnose failures without manually invoking `usbip`, changing firewall rules, or exposing an unencrypted USB/IP port.
