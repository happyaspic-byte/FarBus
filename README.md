# FarBus

Secure, high-performance open-source USB sharing over IPv6 and IPv4.

> **Open-source USB sharing that works in one minute—unlimited devices, secure by default.**

## What FarBus Does Today

- **Zero-Config Discovery:** UDP broadcast beacon on IPv6 (`[ff02::1]:7421`) and IPv4 (`255.255.255.255:7421`).
- **Encrypted by Default:** TLS 1.3 with ephemeral self-signed ECDSA P-256 certificates pinned by 32-byte SHA-256 fingerprint (`farbus-v1` ALPN).
- **One-Time Pairing:** 6-digit rate-limited PIN with constant-time SHA-256 hash verification issuing scoped 256-bit bearer auth tokens.
- **Lease State Machine:** Exclusive, reentrant device leasing with owner-only release protection.
- **Linux USB Inventory:** Reads physical USB devices from `/sys/bus/usb/devices` (VID, PID, speed, class, product). Falls back to simulated test devices if run in headless/container environments.
- **USB/IP 1.1 Wire Codec:** Complete big-endian parser and encoder for `OP_REQ_DEVLIST`, `OP_REP_DEVLIST`, `OP_REQ_IMPORT`, `OP_REP_IMPORT`, `USBIP_CMD_SUBMIT`, and `USBIP_RET_SUBMIT` (compatible with Linux kernel `usbip-host` and Windows `usbip-win2`).
- **Loopback USB/IP Proxy:** Binds to `127.0.0.1:3240` so standard Windows/Linux USB/IP clients connect locally without exposing raw plaintext traffic over the physical network.
- **Reproducible Benchmarks:** `farbus-bench` harness measuring round-trip URB latency (< 0.4 ms), bulk throughput, and reconnect cycles (~4.5 ms).
- **Dual-Stack Pathing:** Happy Eyeballs address interleave prioritizing IPv6 while retaining IPv4 fallback.
- **Full Quality Suite:** 28 unit and end-to-end integration tests, `-D warnings` on all Clippy lints, `unsafe_code = "forbid"`, and dual Linux/Windows CI with cargo-audit.

## Quick Start

### 1. Build and Run the Server (Linux/Pi)

```bash
cargo run --release -p farbus-server
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
# List devices exported by the server
cargo run --release -p farbus-client -- devices <server-fingerprint>

# Attach device #1
cargo run --release -p farbus-client -- attach <server-fingerprint> 1
```

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

# Bulk transfer throughput
cargo run --release -p farbus-bench -- --scenario bulk-throughput

# Disconnect and reconnect speed (50 cycles)
cargo run --release -p farbus-bench -- --scenario reconnect
```

Typical results on modern hardware (loopback TLS):
- **URB Control RTT:** ~0.39 ms
- **Throughput:** > 2,500 operations/second
- **Reconnect Time:** ~4.5 ms per full TLS+handshake cycle

## Workspace Structure

```text
crates/
├── farbus-protocol/  # Bounded wire formats (FarBus control plane + USB/IP 1.1)
├── farbus-core/      # Identity, TLS 1.3, state machines, USB inventory, USB/IP proxy
├── farbus-server/    # Linux server binary with sysfs scanning & mDNS/UDP beacon
├── farbus-client/    # Cross-platform CLI client with session persistence & pairing
└── farbus-bench/     # Automated performance & latency measurement tool
```

## Security Model

- **No Remote Plaintext:** Standard USB/IP traffic is confined to localhost (`127.0.0.1`); all remote network hops require TLS 1.3.
- **Fail-Closed Framing:** Parsers reject oversized buffers, unknown protocol versions, bad magics, and malformed strings.
- **No Passwords on CLI:** PINs are read from the terminal interactively and hashed with constant-time equality checks.
- **Zero Unsafe:** `#![forbid(unsafe_code)]` enabled across the entire workspace.

## License

FarBus is licensed under either:

- Apache License 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE))
- MIT License ([`LICENSE-MIT`](LICENSE-MIT))

at your option.
