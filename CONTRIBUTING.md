# Contributing to THE-BRIDGE

We welcome contributions! This is a dual-licensed matching engine.

## Community Edition (AGPL v3)

All code in this repository is AGPL v3 unless otherwise noted.

## Before Contributing

1. Open an issue to discuss the change
2. Ensure `cargo test` and `cargo check` pass
3. Sign the Contributor License Agreement (CLA)

## Code Style

- Rust std style via `rustfmt`
- No unsafe code unless absolutely necessary and documented
- All public APIs must be documented
- Tests required for new features

## Enterprise Features

Some features are gated behind the `enterprise` Cargo feature. See `Cargo.toml`.
