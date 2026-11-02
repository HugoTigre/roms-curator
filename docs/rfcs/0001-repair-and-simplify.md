# RFC 0001 Repair and simplify roms curator

Status: accepted direction; individual feature decisions remain open.

Created: 2 November 2026. This is the living implementation plan. The [software review](../software-review.md) records the original findings and evidence; this RFC tracks what we decide and complete.

## Outcome and scope

Create a smaller collection of playable arcade games from a merged MAME 0.289 source and separately supplied matching CHDs. Include minor emulation issues, retain every required parent, BIOS, device and disk, and supply an explicit game list for a clean LaunchBox library. MAME 0.289 is fixed on both Windows and Android. Let the frontend group alternate versions.

Keep Rust and the CLI. First repair unsafe or misleading behavior, then introduce a smaller selection and planning architecture, then add a graphical interface over the same library. Whole merged archives may retain unselected variants. Archive rebuilding, Android performance profiles and a native Android curator interface are later decisions.

## How to work through this plan

Pick one task ID at a time. Before implementation, record any needed decision under that task. Complete its acceptance criteria and focused checks, then check it off and record the result. Keep unrelated behavior changes in later tasks. Update this file in the same change as the implementation.

The checkboxes track implementation. A separate validation note records anything still dependent on GitHub, a real collection or a target device. The first recommended functional task after the foundation is **SAFE-01**.

## Phase 0 Establish a reliable development baseline

- [x] **BASE-01 Make CI reproducible and passing locally.** Track Cargo.lock, pin Rust, check formatting and Clippy, and run native builds/tests. Normalize existing formatting and apply mechanical lint fixes without redesigning ROM behavior.
  - Acceptance: local formatting, Clippy, builds and tests pass; GitHub checks are configured for Linux, Windows and macOS. Hosted execution must be confirmed after the first push.
- [x] **BASE-02 Separate release delivery from CI.** Keep validation in `ci.yml`; put publishing in `cd.yml`, triggered only by explicit stable version tags. Match the tag to Cargo.toml and require the commit to be on master. Build optimized Windows/Linux binaries natively and publish them directly.
  - Acceptance: PRs and normal branch pushes have no publishing steps or registry credentials; release configuration passes actionlint and version checks. Keep release logic in the workflow using readable action version tags. Live publishing remains untested until an intentional release.
- [x] **BASE-03 Document the release process.** Record triggers, credentials, version rules, checks and recovery in [releasing.md](../releasing.md).
- [x] **BASE-04 Establish this RFC and preserve review evidence.** Give every repair an ID and acceptance criteria; distinguish proposed GUI choices from accepted product requirements.
- [ ] **BASE-05 Confirm the first hosted CI run.** After pushing, confirm Windows/Linux/macOS jobs pass. Set desired branch protection outside this code change if required.
  - Acceptance: a linked passing GitHub run; no tag, crate or release is created as part of that check.

## Phase 1 Make existing file operations safe and honest

- [ ] **SAFE-01 Reject overlapping and aliased paths.** Review finding 1.
  - Resolve existing sources and existing ancestors of new destinations. Reject equal/overlapping source and destination trees and aliases through links. Include report paths in protection. Treat multiple source roots consistently.
  - Acceptance: source-in-output, output-in-source, relative aliases and supported symlink/junction cases fail before writing. Synthetic file contents remain unchanged on rejection. A separate ordinary destination succeeds on Windows and Unix.
  - Decision needed now: a conservative reject-links policy versus resolving and explicitly validating supported links. Choose this when implementing SAFE-01.
- [ ] **SAFE-02 Detect conflicts and publish copied files safely.** Review finding 1.
  - Plan conflicting destination names before execution. Default to refusing replacement. Copy each file through a temporary file in its destination directory and publish it only on successful completion.
  - Acceptance: duplicate source names and existing destination files produce explicit outcomes. Failed copies do not replace a previously valid file; temporary files are cleaned up. Source files are preserved.
  - Scope: file-level atomicity; a whole-export transaction is a separate later choice.
- [ ] **SAFE-03 Propagate operation errors and return truthful exit codes.** Review findings 4 and 8.
  - Preserve paths and underlying errors. Return nonzero on required copy or requested-report failure. Replace operational panics in touched paths with errors.
  - Acceptance: controlled copy/write failures are visible in reports and CLI status; success and failure cases are tested without relying on administrator/root permission behavior.
- [ ] **SAFE-04 Make simulation read-only.** Review finding 9.
  - Remove destination creation and create/delete probes from argument parsing and simulation. Label simulated operations Planned.
  - Acceptance: the source and destination trees do not change; only an explicitly requested report is written. Existing simulation tests are updated to stop expecting empty output folders.

## Phase 2 Correct metadata and selection

- [ ] **META-01 Parse metadata explicitly and report compatibility.** Review findings 7, 8 and 10.
  - Parse CatVer sections and whitespace; retain the MAME build/version. Parse each disk independently, with quality/defaults and optionality. Validate required fields and enum values with machine-specific errors. Preserve native filesystem paths.
  - Acceptance: sibling disk defaults cannot inherit prior values; malformed input returns useful errors; unknown categories and category coverage are reported. Keep the 0.244 fixture as a historical regression and add a small 0.289-compatible fixture.
- [ ] **SELECT-01 Separate emulation facts from user preferences.** Review finding 6.
  - Default to good/imperfect overall driver status for runnable arcade machines. Remove the implicit two-feature exclusion. Keep category choices and allow/deny overrides separate, with reasons.
  - Acceptance: minor sound plus graphics issues are accepted by the playable preset; preliminary machines are excluded; overrides and category rules explain their effect. Do not silently drop playable genres.
  - Later choices: exact genre exclusions and whether a strict preset is useful.
- [ ] **MODEL-01 Separate machines from storage and support files.** Review findings 2 and 5.
  - Model launch targets, original metadata and actual required media separately. Retain parent, BIOS, device, ROM and disk relationships needed by the resolver.
  - Acceptance: a BIOS/device can be required while absent from the visible game list; a working clone remains selectable even if its parent is excluded as a game.
- [ ] **DEPS-01 Resolve dependencies for merged input.** Review findings 2 and 5.
  - Map selected machines to the archives containing their data. Retain every required supporting archive and detect missing references/cycles. Replace the status-based demotion pass rather than only fixing its discarded recursive return value.
  - Acceptance: selected clone in an excluded parent's archive, BIOS/device dependencies and missing required archives have correct, deterministic outcomes. Subset filtering never silently drops a required dependency.
- [ ] **CHD-01 Retain required shared and parent disks.** Review findings 2, 3 and 7.
  - Resolve CHDs by requirements and content relationships, including shared/merged and delta parents. Replace the blanket non-working-CHD omission rule.
  - Acceptance: every selected game's required disks are retained; wholly unused disks are omitted; missing disks are reported; disk quality and emulation status remain distinct.

## Phase 3 Introduce a reusable plan and export

- [ ] **PLAN-01 Produce a deterministic operation plan.** Review findings 4, 9 and 10.
  - Separate selection, availability and operations. Sort results. Include source/output paths, retaining reasons, expected bytes, conflicts and metadata identity. Use the same plan for preview and execution.
  - Acceptance: repeated planning on identical input gives equivalent output; missing media and conflicts are visible before copying; changing source facts invalidates affected assumptions before execution.
  - Decide plan persistence format and schema version in this task; JSON is the current proposal.
- [ ] **EXPORT-01 Create a self-contained conventional ROM directory.** Review findings 2 and 3.
  - Execute the plan using the safe copy layer. Preserve required archives and CHD folders and export exact selected machine names independently of storage filenames.
  - Acceptance: no full-source path is required to run exported games; dependencies are excluded from the visible list but remain on disk. Document retained unwanted variants inside merged archives.
- [ ] **VERIFY-01 Validate output and test frontend integration.** Review finding 4.
  - Use matching MAME 0.289 to verify selected names against output paths only. Keep optional CHD content verification distinct from availability checks. Test LaunchBox importing and launching a reduced merged selection.
  - Acceptance: representative parent, merged clone, BIOS/device and CHD cases pass; failures identify the game and media. Test the intended Android setup with the same versioned output.
  - Limits: metadata and successful media auditing do not prove acceptable speed or controls on every device.
- [ ] **CLEAN-01 Simplify library configuration and reporting.** Review findings 4, 8 and 10 plus maintenance notes.
  - Separate CLI parsing from library config and filesystem work. Replace map-extension/builder indirection with clear types/functions where useful. Remove discarded report clones and unused fields. Distinguish games, archives and disks in counts; move test-only dependencies out of production.
  - Acceptance: the CLI and a small library caller can use selection/planning independently; reports explain every exclusion, retained dependency and failed operation.
- [ ] **PERF-01 Measure before optimizing.** Review maintenance notes.
  - Measure full-database parsing time and peak memory, then decide whether streaming XML is warranted. Measure a representative export before introducing copy concurrency.
  - Acceptance: record machine/input versions and measurements; optimization has a demonstrated benefit and preserves results.

## Phase 4 Add a curator graphical interface

This is the interface for preparing collections. LaunchBox remains the game launcher. Proposed first scope is a local desktop application; producing collections for Android does not require implementing an Android curator UI now.

**Proposed technology: Tauri 2 with React and TypeScript, using Vite for the UI build and the Rust library for all curation and filesystem work.** This is a recommendation to revisit at GUI-01, not an accepted dependency or an instruction to build the GUI in the foundation change.

Tauri supports web frontends and Rust commands, which suits a local file-heavy utility with a reviewable plan. React is a practical choice for searchable lists, filters and progress views; Svelte is a reasonable substitution if preferred. Keep the framework choice outside the Rust domain layer. See [Tauri frontend configuration](https://v2.tauri.app/start/frontend/) and [calling Rust](https://v2.tauri.app/develop/calling-rust/).

An all-Rust `egui`/`eframe` interface is the main alternative if maintaining one language matters more than web styling and UI component choices. A local browser UI is another option if server/NAS operation later becomes a requirement; it adds a server lifecycle and filesystem access boundary. See [eframe](https://docs.rs/eframe/latest/eframe/). These tradeoffs are design judgments; no GUI prototype has been measured yet.

- [ ] **GUI-01 Choose the shell and validate one vertical slice.** Dependencies: PLAN-01 and CLEAN-01.
  - Decide desktop operating systems and Tauri/web framework versus all-Rust UI. Prototype source selection, plan preview and one progress event without implementing a parallel curation engine.
  - Acceptance: a short decision record; Rust owns validation and operations; long work keeps the UI responsive; errors reach the user.
- [ ] **GUI-02 Build the review workflow.** Dependencies: GUI-01 and EXPORT-01.
  - Screens: source/metadata/output selection; playable/category preferences; searchable game and dependency preview; export progress and results.
  - Acceptance: every excluded game and retained support file has an explanation; the user reviews a plan before starting export; the same core behavior is accessible from the CLI.
- [ ] **GUI-03 Define cancellation and desktop distribution.** Dependencies: GUI-02 and SAFE-02.
  - Specify when cancellation takes effect, incomplete-export state and recovery. Add installers and GUI release assets to CD once the app exists.
  - Acceptance: stopping work preserves source files and completed output, clearly labels incomplete work, and has a tested recovery path. Desktop signing/update choices are decided here, not assumed today.

## Decisions already made

- **D-01 Accepted:** targeted repair followed by a smaller selection/plan architecture; preserve Rust and the CLI.
- **D-02 Accepted:** MAME 0.289 on Windows and Android, merged input, matching CHDs, playable arcade games including minor issues, separate visible game list and required storage.
- **D-03 Accepted:** work through small tasks one at a time; leave optional product decisions until their task.
- **D-04 Implemented for review:** CI validates ordinary changes; CD requires explicit stable `vX.Y.Z` tags matching Cargo.toml on master history. See the release guide.
- **D-05 Proposed:** desktop curator UI using Tauri 2 plus React/TypeScript; decide at GUI-01.
- **D-06 Accepted:** favor a small release workflow with readable action major tags and Dependabot updates. Use Cargo for metadata; keep custom Python release helpers and archive/checksum infrastructure out of the current scope.

## Validation and change log

- 2 November 2026: converted the review into this phased checklist. Inspected the latest GitHub run: Ubuntu failed Clippy and Windows was cancelled; detailed logs had expired. Reproduced lint failures locally with an isolated Rust installation and applied mechanical repairs.
- 2 November 2026: local foundation checks passed on Apple Silicon macOS with Rust 1.99.1: formatting, Clippy across all targets/features, all-target and optimized release builds, 16 existing Rust tests including doctests, actionlint and Cargo's packaged-crate publication dry-run. The optimized CLI reports version 0.1.5. No upload occurred. Windows/Linux hosted runs, registry credentials and a real MAME 0.289 export remain separate acceptance checks.
- 2 November 2026: simplified release delivery after review. Removed Python setup, custom release helpers and their tests, archive packaging and checksum generation. Replaced full action commit pins with readable major tags. Release remains an explicit matching version-tag push, followed by checks, native builds and publication.
- 2 November 2026: checked merged dependency updates through `ce92649`. Updated test/benchmark helpers for Rand 0.9 (`distr`, `rng`), ZIP 9 (`is_dir`) and Criterion 0.8 (`std::hint::black_box`). The all-target and optimized builds, strict Clippy, formatting, 16 normal Rust tests, five serialized full 0.244 metadata integration tests, both Criterion smoke tests, workflow lint and Cargo package verification passed locally. The release CLI reports 0.1.5; nothing was uploaded. Application selection/copy behavior and dependency versions were unchanged by these compatibility fixes. Real 0.289 collection validation and hosted Windows/Linux execution remain separate checks.

When completing a task, append its ID, date, decision summary, checks and remaining limits here. The original review's counts concern MAME 0.244 metadata and must not be presented as 0.289 collection counts or measured disk savings.
