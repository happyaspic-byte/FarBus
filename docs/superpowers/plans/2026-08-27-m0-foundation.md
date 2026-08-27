# FarBus M0 Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a testable Rust workspace containing FarBus protocol framing, dual-stack path planning, lease and connection state machines, CLI shells, documentation, and CI.

**Architecture:** Keep all remotely reachable framing in `farbus-protocol`; keep platform-independent policy in `farbus-core`; expose thin Linux server, Windows client, and benchmark binaries. M0 contains no physical USB operations, TLS listener, or Windows driver integration.

**Tech Stack:** Rust 1.80+, Cargo workspace, Tokio, Clap, thiserror, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-08-27-farbus-design.md`

## Global Constraints

- Linux x86_64 and ARM64 server; Windows 10/11 x64 client are the first supported platforms.
- Networking is IPv6-first dual stack with IPv4 fallback.
- Remote traffic will require TLS 1.3; M0 must not open a plaintext USB/IP listener.
- Wire messages have explicit size limits and reject malformed input.
- Rust-authored user-space code uses `MIT OR Apache-2.0`.
- Unsafe Rust is forbidden.

---

### Task 1: Workspace and protocol codec

**Files:**
- Create: `Cargo.toml`
- Create: `crates/farbus-protocol/Cargo.toml`
- Create: `crates/farbus-protocol/src/lib.rs`

**Interfaces:**
- Produces: `encode(&Message) -> Result<Vec<u8>, Error>` and `decode(&[u8]) -> Result<Message, Error>`.

- [x] Write tests for Hello and device-list round trips, bad magic, truncation, version rejection, and size limits.
- [x] Run `cargo test -p farbus-protocol` and confirm missing API failures.
- [x] Implement the bounded binary codec.
- [x] Run `cargo test -p farbus-protocol` and confirm all tests pass.

### Task 2: Core policy state machines

**Files:**
- Create: `crates/farbus-core/tests/policy.rs`
- Modify: `crates/farbus-core/src/lib.rs`
- Create: `crates/farbus-core/src/{fingerprint,lease,path,state}.rs`

**Interfaces:**
- Consumes: `farbus_protocol::DeviceId`.
- Produces: `PeerFingerprint`, `LeaseBook`, `connection_order`, and `ConnectionMachine`.

- [ ] Write integration tests covering fingerprint validation, exclusive leases, IPv6/IPv4 interleaving, valid connection transitions, and invalid transition rejection.
- [ ] Run `cargo test -p farbus-core` and confirm imports fail.
- [ ] Implement the smallest policy modules satisfying tests.
- [ ] Run `cargo test -p farbus-core` and confirm all tests pass.

### Task 3: CLI shells

**Files:**
- Create: `crates/farbus-server/src/main.rs`
- Create: `crates/farbus-client/src/main.rs`
- Create: `crates/farbus-bench/src/main.rs`

**Interfaces:**
- Produces: `farbus-server`, `farbus`, and `farbus-bench` command surfaces.

- [ ] Add command parsing smoke tests using Clap's `try_parse_from`.
- [ ] Confirm tests fail before exposing parsers as library modules.
- [ ] Implement parser modules and keep binary entry points thin.
- [ ] Confirm all CLI parser tests pass.

### Task 4: Project documentation, licenses, and CI

**Files:**
- Create: `README.md`
- Create: `LICENSE-MIT`
- Create: `LICENSE-APACHE`
- Create: `SECURITY.md`
- Create: `CONTRIBUTING.md`
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Produces: contributor build/test commands and automated verification.

- [ ] Document current M0 capabilities without claiming USB forwarding works.
- [ ] Add Apache-2.0 and MIT license texts.
- [ ] Add a security reporting policy and contribution checks.
- [ ] Configure Linux and Windows CI for format, Clippy, tests, and builds.
- [ ] Run the same checks locally and fix all failures.

### Task 5: Publish M0 foundation

**Files:**
- Modify: all M0 files after verification only.

**Interfaces:**
- Produces: public `develop` branch on `happyaspic-byte/FarBus`.

- [ ] Run `cargo fmt --check`.
- [ ] Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [ ] Run `cargo test --workspace`.
- [ ] Build all binaries in release mode.
- [ ] Review `git diff --check` and `git status`.
- [ ] Commit with an M0 foundation message.
- [ ] Push `develop` without force.
