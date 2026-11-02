# RFC 0001 Repair and simplify roms curator

Status: accepted direction. Work through one task at a time; check off completed items. Keep implementation history and test results in commits and PRs.

The [software review](../software-review.md) contains the original findings.

## Target

- Smaller, self-contained collection of playable arcade games from merged MAME 0.289 ROMs and matching CHDs.
- Accept minor emulation issues; retain required parents, BIOSes, devices and disks.
- MAME 0.289 on Windows and Android, with a separate game list for LaunchBox. Let the frontend group alternate versions.
- Keep Rust and the CLI. Repair safety first, then introduce selection/planning, then add a GUI.
- Retained merged archives may contain unselected variants. Archive rebuilding and device-specific performance filters are later decisions.

## Phase 0 Development baseline

- [x] **BASE-01** Reproducible CI with formatting, Clippy, builds and tests.
- [x] **BASE-02** Separate CD with intentional version-tag publishing and readable action versions.
- [x] **BASE-03** Document [release rules](../releasing.md).
- [x] **BASE-04** Create this implementation checklist.
- [ ] **BASE-05** Confirm hosted Linux, Windows and macOS checks pass. Live publishing is verified only during an intentional release.

## Phase 1 Safe file operations

- [x] **SAFE-01** Reject overlapping/aliased source, output and report paths before writing. Resolve root aliases; reject nested links/reparse points and output hard links to source entries. Assumes stable trees during execution.
- [ ] **SAFE-02** Detect conflicting destination names and existing files before copying. Refuse replacement by default; copy through temporary files and publish completed files safely.
  - Done when conflicts are explicit, failed copies preserve existing files, temporary files are cleaned up and source files remain unchanged.
  - File-level atomicity first; whole-export transactions are a later choice.
- [ ] **SAFE-03** Propagate useful operation errors and return nonzero exit codes for required copy or requested-report failures.
  - Done when controlled write/copy failures identify their paths and causes, appear in reports and fail the CLI without operational panics.
- [ ] **SAFE-04** Make simulation read-only and label operations Planned.
  - Done when source/output trees stay unchanged and only an explicitly requested report is written.

## Phase 2 Metadata and selection

- [ ] **META-01** Parse CatVer sections, MAME version, disk defaults/quality/optionality and required fields explicitly. Preserve native paths.
  - Done when sibling disks cannot inherit status, malformed input returns contextual errors and unknown categories/coverage are visible. Add a small 0.289 fixture; retain 0.244 as a regression.
- [ ] **SELECT-01** Separate emulation status from category preferences and overrides. Default to good/imperfect runnable arcade machines; remove the implicit two-feature exclusion.
  - Done when minor sound/graphics issues are accepted, preliminary machines are excluded and every rule/override explains its effect. Decide genre preferences here.
- [ ] **MODEL-01** Separate selected machines from required storage. Retain parent, BIOS, device, ROM and disk relationships.
  - Done when supporting files can remain outside the visible game list and a clone remains selectable despite its parent's gameplay status.
- [ ] **DEPS-01** Resolve selected machines to required merged archives and support files. Replace status-based dependency demotion.
  - Done when clone-in-parent, BIOS/device, missing-reference and cycle cases are handled deterministically. Subset filters must retain dependencies.
- [ ] **CHD-01** Resolve shared disks and delta parents; replace blanket non-working-CHD omission.
  - Done when selected games retain all required disks, unused disks are omitted and missing media is reported separately from emulation quality.

## Phase 3 Plan and export

- [ ] **PLAN-01** Produce a sorted, reusable plan with selection reasons, media availability, paths, expected bytes, conflicts and metadata identity.
  - Done when identical inputs produce equivalent plans and missing media/conflicts are visible before copying. Recheck affected assumptions before execution. Decide persistence/schema here; JSON is proposed.
- [ ] **EXPORT-01** Execute the plan into a conventional ROM directory and export exact selected machine names separately.
  - Done when output works independently of the full source, dependencies remain available and retained merged variants are documented.
- [ ] **VERIFY-01** Verify selected names using MAME 0.289 against output paths only. Test reduced merged import/launching in LaunchBox and the intended Android setup.
  - Done when parent, merged clone, BIOS/device and CHD cases pass and failures identify missing media. Keep optional CHD integrity checks separate; auditing does not prove device performance.
- [ ] **CLEAN-01** Separate CLI parsing from library configuration and filesystem work. Simplify map extensions/builders, remove unused report fields/clones and move test-only dependencies.
  - Done when a library caller can select/plan independently and reports distinguish games, archives, disks and operation outcomes.
- [ ] **PERF-01** Measure full-database time/memory and representative copying before adding streaming or concurrency.
  - Done when measurements justify any optimization and results remain unchanged.

## Phase 4 Curator GUI

Build a local desktop interface for preparing collections; LaunchBox remains the launcher. A native Android curator UI is a separate decision.

Proposed stack: **Tauri 2 + React/TypeScript + Vite**, sharing the Rust library with the CLI. Choose at GUI-01. An all-Rust egui/eframe interface is the main alternative; a browser/server UI becomes relevant if NAS/server operation is needed.

- [ ] **GUI-01** Choose desktop targets and the UI stack after PLAN-01/CLEAN-01. Prototype source selection, plan preview and progress.
  - Done when Rust owns validation/operations, long work keeps the UI responsive and errors reach the user.
- [ ] **GUI-02** Add preferences, searchable game/dependency preview, export progress and results after GUI-01/EXPORT-01.
  - Done when exclusions/support files are explained and users review a plan before exporting through the shared core.
- [ ] **GUI-03** Define cancellation, incomplete-export recovery and desktop distribution after GUI-02/SAFE-02.
  - Done when cancellation preserves source/completed output and recovery is tested. Decide signing, updates and GUI release assets here.

## Remaining validation

- Hosted Windows junction/case-alias coverage for SAFE-01.
- Real MAME 0.289 collection export and frontend/device checks at VERIFY-01.
- First intentional crates.io/GitHub release.

The review's 0.244 counts are historical test evidence, not counts or savings for the target 0.289 collection.
