# Build instructions

__Index__

- [Build for current platform](#build-for-current-platform)
- [Run tests](#run-tests)
- [Build platform specific binary](#build-platform-specific-binary)

## Build for current platform

Install Rust using rustup. The repository's `rust-toolchain.toml` selects the
validated compiler and its formatting/Clippy components. Keep `Cargo.lock`
committed so local builds and CI use the same dependency versions.

```bash
cargo build --locked
```

## Run tests

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
```

CI also builds all targets on Linux, Windows and macOS.

Run with expensive tests

```bash
CARGO_RC_RUN_EXPENSIVE_TESTS=true cargo test --locked --test lib_it -- --test-threads=1
```

## Build platform specific binary

Example for windows

```bash
cargo build --release --locked --target x86_64-pc-windows-msvc
```

Run this on Windows with the MSVC build tools. The release workflow builds
Windows and Linux binaries on native runners, so `cross` is not required.
See [release delivery](releasing.md) for the tag and publishing rules.
