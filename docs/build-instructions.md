# Build instructions

__Index__

- [Build for current platform](#build-for-current-platform)
- [Run tests](#run-tests)
- [Build platform specific binary](#build-platform-specific-binary)

## Build for current platform

```bash
cargo build
```

## Run tests

```bash
cargo test
```

## Build platform specific binary

Example for windows

```bash
cross build --target x86_64-pc-windows-gnu
```
