# FarBus

Secure, open-source USB sharing over IPv6 and IPv4.

> Open-source USB sharing that works in one minute—unlimited devices, secure by default.

## Status

FarBus is in **M0 foundation development**. The repository currently contains the protocol codec, shared policy state machines, and CLI shells. It does **not yet forward physical USB devices**.

Do not expose FarBus or USB/IP ports to untrusted networks. TLS transport and device pairing arrive in M1.

## Direction

The first supported path will be:

```text
Linux USB host → FarBus server → encrypted dual-stack network → FarBus Windows client
```

FarBus will initially wrap the Linux kernel USB/IP data plane and a compatible signed Windows USB/IP driver. It adds mandatory encryption, device identity, pairing, discovery, automatic reconnection, diagnostics, and an unlimited-device policy.

## Workspace

| Crate | Purpose |
|---|---|
| `farbus-protocol` | Bounded binary control-plane framing |
| `farbus-core` | Identity, leases, path ordering, and lifecycle state machines |
| `farbus-server` | Linux export service shell |
| `farbus-client` | Windows client CLI shell (`farbus`) |
| `farbus-bench` | Reproducible performance harness shell |

## Build

Rust 1.80 or newer is required.

```bash
cargo build --workspace
cargo test --workspace
```

Quality checks:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Current CLI shells:

```bash
cargo run -p farbus-client -- discover
cargo run -p farbus-server -- --help
cargo run -p farbus-bench -- --help
```

## Security model

The design requires TLS 1.3, mutual identity after PIN pairing, explicit export policy, loopback-only kernel USB/IP, bounded wire messages, and no USB payload logging. M0 implements bounded framing and policy primitives; the secure network listener is not implemented yet.

See [`SECURITY.md`](SECURITY.md) for vulnerability reporting and the [design specification](docs/superpowers/specs/2026-08-27-farbus-design.md) for the complete architecture.

## Roadmap

- **M0:** Protocol, state machines, CLI shells, CI
- **M1:** TLS 1.3, mDNS, IPv6-first dual stack, PIN pairing
- **M2:** Linux USB enumeration and explicit exports
- **M3:** Windows attach and installer
- **M4:** Per-device channels, benchmarks, compatibility matrix
- **M5:** Home-lab beta and GUI

## License

FarBus-authored user-space code is licensed under either:

- Apache License 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE))
- MIT License ([`LICENSE-MIT`](LICENSE-MIT))

at your option.
