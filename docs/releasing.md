# CI and release delivery

CI validates ordinary changes. CD publishes only when you deliberately push a matching version tag. Merging a PR or bumping Cargo.toml does not publish anything.

## CI

`.github/workflows/ci.yml` runs for PRs targeting main, pushes to main, manual runs and calls from CD. A path filter skips Rust and GUI jobs when unrelated files change (for example docs-only); edits to `ci.yml` or `cd.yml` still run the affected jobs. Rust checks cover formatting and Clippy on Linux and build/tests on Linux, Windows and macOS. The GUI job runs when the app stack or shared library changes. On GUI PRs, CI runs fmt, Clippy, check, test, and a release **`tauri build --no-bundle`**; the full **`npm run tauri build`** (installers) runs on main pushes, manual CI, and CD only when the GUI job runs (GUI-related paths changed). Cargo and npm dependencies are cached on repeat runs. Rust is selected by `rust-toolchain.toml`; Cargo.lock keeps dependency resolution consistent.

Actions use readable major-version references, such as `actions/checkout@v7`. Dependabot checks GitHub Actions and Cargo updates weekly. Full commit pins are optional extra protection against an action tag changing; this project uses major tags for simpler maintenance.

## Release rules

`.github/workflows/cd.yml` runs only for pushed tags matching `v[0-9]*`. It has three jobs: run CI checks, build optimized Linux/Windows binaries, then publish.

Before publication:

- The tag must be a stable `vMAJOR.MINOR.PATCH` version matching Cargo.toml, such as `v0.2.0`.
- The tagged commit must already be on origin/main history.
- CI and both native release builds must pass.
- Both binaries must be present and Cargo's publication dry-run must pass.
- The repository's `CRATES_TOKEN` secret must permit publishing the new version.

The workflow publishes the crate to crates.io and creates a GitHub release with `roms-curator-linux`, `roms-curator.exe` and LICENSE. Only the publishing job has repository write permission. CI has no registry credentials.

### GUI release delivery

**Decision (GUI-03):**

- **CI** runs the Linux GUI checks when GUI-related paths change. PRs use **`tauri build --no-bundle`**; the full **`npm run tauri build`** (installers) runs on main, manual CI, and release (CD) when that job runs. Artifacts are validated in CI but not uploaded automatically yet.
- **Signing** (Windows Authenticode, Apple notarization) and **in-app updates** are deferred until the first GUI release ships; document any manual signing steps when attaching GUI installers to a tag.
- **GitHub release assets** for the curator app (`.deb` / AppImage / `.msi` / `.dmg`) will be added to CD in the same tag as the CLI binaries when GUI is included in an intentional release. Until then, build installers locally with `npm run tauri build` and attach them manually if needed.

See [gui-export-recovery.md](gui-export-recovery.md) for cancellation and partial-export behavior.

The release workflow uses Cargo and a small inline Bash check for the version. It has no Python setup, custom packaging scripts or checksum-generation helpers. The Linux download may need `chmod +x roms-curator-linux` before running.

## Publish intentionally

1. Change Cargo.toml to a new unpublished version and update Cargo.lock with Cargo.
2. Merge that change to main after CI passes.
3. Update your local main to the intended commit.
4. Create and push the matching tag. For version 0.2.0:

```sh
git tag -a v0.2.0 -m "Release 0.2.0"
git push origin v0.2.0
```

Follow the CD run and verify its release assets. Ordinary merges continue to run CI only. Bare tags such as `0.2.0` do not trigger CD; prerelease tags fail validation under this initial stable-release policy.

The next release targets **0.2.0** because these changes break source compatibility with 0.1.5 (do not ship as 0.1.6; see [Cargo semver](https://doc.rust-lang.org/cargo/reference/semver.html#change-categories)):

- **`ExportConfig`** replaces ad-hoc `Args` helpers; validate before plan/copy.
- **`run` → `Catalog`** with dependency-aware **`copy_roms`** into **`roms`** / **`rest`**, version-2 **plans**, and **`selected.txt`** (launch-ready names by default).
- Selection/plan/CHD behavior is summarized in [RFC 0001](rfcs/0001-repair-and-simplify.md) and [library usage](lib-usage-instructions.md).

Library API detail lives in `cargo doc` and the source; this file covers CI/CD only.

These are instructions, not publication actions. Bumping the package version does not publish it. Version 0.1.5 already exists on crates.io; published versions cannot be replaced.

## Failure recovery

If checks or builds fail, fix the problem before publishing. The first hosted run and registry authentication still need verification; local validation does not exercise publishing.

crates.io and GitHub publication are separate steps. If the crate publishes but GitHub release creation fails, inspect that run and finish the GitHub release for the same tag using its binary artifacts, retained for seven days. Do not retry Cargo publication for an existing version. If artifacts have expired, rebuild from the original tag. A fix to published code needs a new version.

## Previous behavior

The original combined workflow also required a valid version tag before publishing; it did not normally publish after every PR merge. However, it discovered tags during a main push using `git describe --tags --contains`, and a tag-only push did not trigger it. The dedicated CD workflow makes tag pushes the explicit release action.

References: [GitHub workflow triggers](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows), [Dependabot action updates](https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/secure-your-dependencies/auto-update-actions), and [Cargo publication](https://doc.rust-lang.org/cargo/commands/cargo-publish.html).
