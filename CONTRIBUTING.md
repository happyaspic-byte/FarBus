# Contributing

FarBus is early-stage. Discuss protocol changes and new platform scope in an issue before implementation.

## Development

Use Rust 1.80 or newer. Before opening a pull request, run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build --workspace --release
```

New behavior requires a failing test before implementation. Keep wire formats bounded and versioned. Do not add plaintext network modes, payload logging, embedded secrets, unsigned driver binaries, or third-party binary artifacts without provenance and license review.

## Commit and pull request scope

Keep changes focused. Describe the user-visible behavior, security impact, and test evidence. Hardware-dependent work must document the device class, VID/PID, host OS, client OS, and network conditions.
