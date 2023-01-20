# Build instructions

- [Build instructions](#build-instructions)
  - [Build for current platform](#build-for-current-platform)
  - [Build platform specific binary](#build-platform-specific-binary)

## Build for current platform

```bash
cargo build
```

## Build platform specific binary

Example for windows

```bash
cross build --target x86_64-pc-windows-gnu
```
