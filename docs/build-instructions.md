# Build instructions

__Index__

- [Build for current platform](#build-for-current-platform)
- [Curator GUI (Tauri)](#curator-gui-tauri)
- [Run tests](#run-tests)
- [Build platform specific binary](#build-platform-specific-binary)

## Build for current platform

Install Rust using rustup. The repository's `rust-toolchain.toml` selects the
validated compiler and its formatting/Clippy components. Keep `Cargo.lock`
committed so local builds and CI use the same dependency versions.

```bash
cargo build --locked
```

## Curator GUI (Tauri)

Install Node.js 26 or newer (`package.json` `engines`; CI uses major 26). Then from the repository root:

```bash
npm install
npm run tauri dev
```

The React UI lives in `gui/`. Rust commands in `src-tauri/` call the same
`roms-curator` library as the CLI. The GUI crate keeps its own `src-tauri/Cargo.lock`;
build it with `cargo build --manifest-path src-tauri/Cargo.toml --locked` after
`npm install`. Linux CI runs GUI fmt/Clippy/check/test on relevant PRs,
`tauri build --no-bundle` on GUI PRs, and full `npm run tauri build` on main
and release when the GUI job runs. Partial exports and
cancellation: [gui-export-recovery.md](gui-export-recovery.md). Release policy:
[releasing.md](releasing.md#gui-release-delivery).

```bash
npm run tauri build
```

Installers appear under `src-tauri/target/release/bundle/`.

## Run tests

Run all normal tests, including documentation tests:

```bash
cargo test --locked --all-features
```

When you change the Tauri shell or `src-tauri/src/commands.rs`, also run the GUI
crate checks that Linux CI runs:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --locked -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

Run the formatting and lint checks used by CI:

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
```

CI also builds all targets on Linux, Windows and macOS.

Also run live metadata and full-database tests (macOS/Linux; needs network):

```bash
CARGO_RC_RUN_EXPENSIVE_TESTS=true cargo test --locked --test lib_it --test metadata_fetch_it -- --test-threads=1
```

Optional: set `CARGO_RC_MAME_VERSION` to another release (default `0.289`) when downloading metadata.

In Windows PowerShell:

```powershell
$env:CARGO_RC_RUN_EXPENSIVE_TESTS = "true"
try {
    cargo test --locked --test lib_it --test metadata_fetch_it -- --test-threads=1
} finally {
    Remove-Item Env:CARGO_RC_RUN_EXPENSIVE_TESTS
}
```

Expensive runs download listxml and CatVer into `target/tests/metadata-cache/` and reuse that cache across tests. The first run can take several minutes because of the listxml size. Use `--test-threads=1` to limit memory use.

## Benchmarks

See [performance measurements](performance.md) for Criterion targets, full-database
runs (`CARGO_RC_RUN_EXPENSIVE_TESTS`), and peak memory capture.

Quick sample run:

```bash
cargo bench --locked
```

## Build platform specific binary

Example for windows

```bash
cargo build --release --locked --target x86_64-pc-windows-msvc
```

Run this on Windows with the MSVC build tools. The release workflow builds
Windows and Linux binaries on native runners, so `cross` is not required.
See [release delivery](releasing.md) for the tag and publishing rules.
