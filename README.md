# FarBus

Secure, high-performance open-source USB sharing over IPv6 and IPv4.

> **Open-source USB sharing that works in one minute—unlimited devices, secure by default.**

## What FarBus Does Today

- **Zero-Config Discovery:** UDP broadcast beacon on IPv6 (`[ff02::1]:7421`) and IPv4 (`255.255.255.255:7421`).
- **Encrypted by Default:** TLS 1.3 with ephemeral self-signed ECDSA P-256 certificates pinned by 32-byte SHA-256 fingerprint (`farbus-v1` ALPN).
- **One-Time Pairing:** 6-digit PIN, five failed attempts then lockout, constant-time SHA-256 verification. A successful pair consumes the PIN, rotates a new one, and issues a 256-bit bearer token.
- **Lease State Machine:** Exclusive, reentrant device leasing with owner-only release protection.
- **Linux USB Inventory & Hotplug:** Polls libusb/sysfs using stable physical topology paths, preserves in-process device IDs across scan ordering, and revokes leases when a device disappears. Emulated devices are confined to tests and benchmarks.
- **USB/IP 1.1 Wire Codec:** Big-endian parser and encoder for `OP_REQ_DEVLIST`, `OP_REP_DEVLIST`, `OP_REQ_IMPORT`, `OP_REP_IMPORT`, `USBIP_CMD_SUBMIT`/`USBIP_RET_SUBMIT`, and `USBIP_CMD_UNLINK`/`USBIP_RET_UNLINK` (Linux `usbip-host`, Windows `usbip-win2`).
- **Pipelined URBs:** Protocol v2 carries an explicit `requested_length`, so bulk/interrupt IN submits do not send a zero-filled TLS payload. Multiple in-flight submits complete out of order over one TLS session.
- **Loopback USB/IP Proxy:** Binds to `127.0.0.1:3240` so standard Windows/Linux USB/IP clients connect locally without exposing raw plaintext traffic over the physical network. Dropped TLS sessions reconnect, reattach the same lease, and retry the failed URB once.
- **Reproducible Benchmarks:** `farbus-bench` harness measuring URB latency, bulk throughput, and reconnect cycles.
- **Dual-Stack Pathing:** Happy Eyeballs address interleave prioritizing IPv6 while retaining IPv4 fallback.
- **Authenticated Data Plane:** Device list, attach, detach, and URB require a verified token bound to the TLS session. Hello fingerprints alone cannot list or steal a lease.
- **Bounded USB/IP:** `transfer_buffer_length` above 65,536 bytes is rejected before allocation.
- **Full Quality Suite:** Workspace tests, `-D warnings` Clippy, `unsafe_code = "forbid"`, Linux/Windows CI, dependency audit, and loopback benchmarks.

## Quick Start

Tagged releases publish SHA-256 checksummed Linux x86_64 and unsigned Windows x86_64 archives. The Windows archive contains user-space `farbus.exe` only; install the signed usbip-win2 driver separately.

### 1. Build and Run the Server (Linux/Pi)

Physical USB devices are **not exported by default**. Prefer exact `--export BUS-ID` entries. `--export-all` deliberately excludes HID, mass-storage, hubs, and composites containing those interfaces; export sensitive devices only by exact bus ID.

```bash
cargo run --release -p farbus-server -- --export 1-1.2
# or: scripts/install-linux.sh --systemd, then edit the unit with exact --export entries
```

Output:
```text
==================================================
 FarBus USB Server 0.1.0
 Fingerprint : 7a2f1b... (64 hex characters)
 Pairing PIN : 482910  (valid for 2 minutes)
 Listening   : [::]:7420
 Exported    : 3 devices
==================================================
```

### 2. Discover and Pair from the Client (Windows / Linux)

```bash
# Discover servers on the LAN
cargo run --release -p farbus-client -- discover

# Pair with the 6-digit PIN
cargo run --release -p farbus-client -- --connect 192.168.1.100:7420 pair <server-fingerprint>
```

### 3. List and Attach Remote Devices

```bash
# Uses the most recently paired session
farbus devices

# Attach device #1 and keep the local USB/IP proxy running
farbus attach 1

# Release device #1 from another terminal
farbus detach 1
```

Windows setup, including the unsigned per-user FarBus ZIP installer and separately installed usbip-win2 driver, is documented in [`docs/WINDOWS.md`](docs/WINDOWS.md). HID devices can inject input; export them only when you trust the client.

### 4. Connect with Standard USB/IP (Windows / Linux)

```bash
# On Windows (with usbip-win2):
usbip attach --remote=127.0.0.1 --busid=1-1.2

# On Linux:
sudo usbip attach --remote=127.0.0.1 --busid=1-1.2
```

## Benchmarks

Run the built-in benchmark harness:

```bash
# Control transfer latency (1,000 rounds)
cargo run --release -p farbus-bench -- --scenario control-latency

# Bulk transfer throughput (depth 64 pipeline)
cargo run --release -p farbus-bench -- --scenario bulk-throughput --depth 64

# Disconnect and reconnect speed (50 cycles)
cargo run --release -p farbus-bench -- --scenario reconnect
```

Measured on this workspace (loopback TLS 1.3, release, emulated devices — not physical USB):
- **URB Control RTT:** 0.197 ms (5,080 ops/s, 1,000 rounds)
- **Bulk OUT:** 125.22 MB/s (16 KiB × 1,000, pipeline depth 64)
- **Reconnect Time:** 1.56 ms per TLS reconnect cycle (50 cycles)

## Workspace Structure

```text
crates/
├── farbus-protocol/  # Bounded wire formats (FarBus control plane + USB/IP 1.1)
├── farbus-core/      # Identity, TLS 1.3, state machines, USB inventory, USB/IP proxy
├── farbus-server/    # Linux server binary with sysfs scanning & UDP discovery beacon
├── farbus-client/    # Cross-platform CLI client with session persistence & pairing
└── farbus-bench/     # Automated performance & latency measurement tool
```

## Security Model

- **No Remote Plaintext:** Standard USB/IP traffic is confined to localhost (`127.0.0.1`); all remote network hops require TLS 1.3.
- **Fail-Closed Framing:** Parsers reject oversized buffers, unknown protocol versions, bad magics, and malformed strings.
- **No Passwords on CLI:** PINs are read from the terminal interactively and hashed with constant-time equality checks.
- **Session Files:** `~/.config/farbus` is created mode `0700`; tokens are files mode `0600`.
- **Zero Unsafe:** `#![forbid(unsafe_code)]` enabled across the entire workspace.

## License

FarBus is licensed under either:

- Apache License 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE))
- MIT License ([`LICENSE-MIT`](LICENSE-MIT))

at your option.
