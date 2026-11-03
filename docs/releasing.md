# CI and release delivery

CI validates ordinary changes. CD publishes only when you deliberately push a matching version tag. Merging a PR or bumping Cargo.toml does not publish anything.

## CI

`.github/workflows/ci.yml` runs for PRs targeting master, pushes to master, manual runs and calls from CD. It checks formatting and Clippy on Linux and builds/tests on Linux, Windows and macOS. Rust is selected by `rust-toolchain.toml`; Cargo.lock keeps dependency resolution consistent.

Actions use readable major-version references, such as `actions/checkout@v7`. Dependabot checks GitHub Actions and Cargo updates weekly. Full commit pins are optional extra protection against an action tag changing; this project uses major tags for simpler maintenance.

## Release rules

`.github/workflows/cd.yml` runs only for pushed tags matching `v[0-9]*`. It has three jobs: run CI checks, build optimized Linux/Windows binaries, then publish.

Before publication:

- The tag must be a stable `vMAJOR.MINOR.PATCH` version matching Cargo.toml, such as `v0.2.0`.
- The tagged commit must already be on origin/master history.
- CI and both native release builds must pass.
- Both binaries must be present and Cargo's publication dry-run must pass.
- The repository's `CRATES_TOKEN` secret must permit publishing the new version.

The workflow publishes the crate to crates.io and creates a GitHub release with `roms-curator-linux`, `roms-curator.exe` and LICENSE. Only the publishing job has repository write permission. CI has no registry credentials.

The release workflow uses Cargo and a small inline Bash check for the version. It has no Python setup, custom packaging scripts or checksum-generation helpers. The Linux download may need `chmod +x roms-curator-linux` before running.

## Publish intentionally

1. Change Cargo.toml to a new unpublished version and update Cargo.lock with Cargo.
2. Merge that change to master after CI passes.
3. Update your local master to the intended commit.
4. Create and push the matching tag. For version 0.2.0:

```sh
git tag -a v0.2.0 -m "Release 0.2.0"
git push origin v0.2.0
```

Follow the CD run and verify its release assets. Ordinary merges continue to run CI only. Bare tags such as `0.2.0` do not trigger CD; prerelease tags fail validation under this initial stable-release policy.

The next release targets **0.2.0** because these changes break source compatibility with 0.1.5:

- `RomsExt::copy_rom` returns `io::Result<()>` and reports use explicit operation statuses and error details instead of a `moved` boolean.
- `run` returns `Catalog` instead of `Roms` (`HashMap<String, Rom>`). `Catalog` iterates as that map and exposes `copy_roms`.
- `UnfilteredRomsExt::categorize_roms` is removed. Categorization goes through `ParsedMachines::categorize`, which names the XML that was parsed.
- Disk quality uses `ChdStatus::Good` for a good dump, including when `status` is omitted. `NoStatus` is gone.
- `EXCLUDED_CATEGORIES`, `SPECIAL_CASES_DEMOTE`, and `SPECIAL_CASES_PROMOTE` are removed. Non-arcade groups and named system overrides live in `models::selection`. A CatVer group matches as a whole group, so `Computer` matches `Computer / Home` and `Shooter / Computer` stays selected. Named overrides such as `sy888b` are categorized as System even when their CatVer entry is an arcade genre. Sound and graphics features no longer change selection.
- `RomData` stores device references and ROM dumps alongside the existing parent and disk links. `Rom.selected` and `Catalog.selected` are the visible game list. A clone stays on that list when its parent is not selected. `Catalog.dependencies` lists the merged archives those games need. Status-based dependency demotion is gone, so a bad disk or a non-working parent no longer changes `Rom.category`. A name-range subset still copies required parent, BIOS, and device archives.

Cargo treats 0.1.x updates as compatible, so these changes must not ship as 0.1.6. See [Cargo's compatibility rules](https://doc.rust-lang.org/cargo/reference/semver.html#change-categories).

These are instructions, not publication actions. Bumping the package version does not publish it. Version 0.1.5 already exists on crates.io; published versions cannot be replaced.

## Failure recovery

If checks or builds fail, fix the problem before publishing. The first hosted run and registry authentication still need verification; local validation does not exercise publishing.

crates.io and GitHub publication are separate steps. If the crate publishes but GitHub release creation fails, inspect that run and finish the GitHub release for the same tag using its binary artifacts, retained for seven days. Do not retry Cargo publication for an existing version. If artifacts have expired, rebuild from the original tag. A fix to published code needs a new version.

## Previous behavior

The original combined workflow also required a valid version tag before publishing; it did not normally publish after every PR merge. However, it discovered tags during a master push using `git describe --tags --contains`, and a tag-only push did not trigger it. The dedicated CD workflow makes tag pushes the explicit release action.

References: [GitHub workflow triggers](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows), [Dependabot action updates](https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/secure-your-dependencies/auto-update-actions), and [Cargo publication](https://doc.rust-lang.org/cargo/commands/cargo-publish.html).
