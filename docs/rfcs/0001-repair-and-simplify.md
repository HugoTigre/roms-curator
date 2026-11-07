# RFC 0001 Repair and simplify roms curator

Status: accepted direction. Work through one task at a time. Check off an item in the same change that implements it, so master does not contain the behavior without the checklist. Keep implementation history and test results in commits and PRs.

The [software review](../software-review.md) contains the original findings.

## Target

- Merged MAME 0.289 collection split into two folders: `roms` is everything a selected game needs to run, and `rest` is every other source file, so the original collection can be deleted. `selected.txt` names the selected machines. Split-set file selection is a later decision.
- Accept minor emulation issues; retain required parents, BIOSes, devices and disks.
- MAME 0.289 on Windows and Android, with a separate game list for LaunchBox. Let the frontend group alternate versions.
- Keep Rust and the CLI. Repair safety first, then introduce selection/planning, then add a GUI.
- Retained merged archives may contain unselected variants. Archive rebuilding and device-specific performance filters are later decisions.

## Phase 0 Development baseline

- [x] **BASE-01** Reproducible CI with formatting, Clippy, builds and tests.
- [x] **BASE-02** Separate CD with intentional version-tag publishing and readable action versions.
- [x] **BASE-03** Document [release rules](../releasing.md).
- [x] **BASE-04** Create this implementation checklist.
- [x] **BASE-05** Confirm hosted Linux, Windows and macOS checks pass. Live publishing is verified only during an intentional release.

## Phase 1 Safe file operations

- [x] **SAFE-01** Reject overlapping/aliased source, output and report paths before writing. Resolve root aliases; reject nested links/reparse points and output hard links to source entries. Assumes stable trees during execution.
- [x] **SAFE-02** Preflight destination conflicts, including case-only names. Refuse replacement and CHD folder merges; publish complete files through sibling temporaries and clean up on normal failures. Completed files remain after later failures; whole-export transactions are deferred.
- [x] **SAFE-03** Record copy paths/causes in reports and return nonzero CLI status for copy/report failures. Retain completed files; publish flushed reports without replacement and fall back to stderr when saving fails.
- [x] **SAFE-04** Read-only simulation with explicit Planned outcomes and accurate counts. Leave source/output trees unchanged; write only an optional report outside both trees. Parsing performs no write probes.

## Phase 2 Metadata and selection

- [x] **META-01** Parse CatVer sections, MAME version, disk defaults/quality/optionality and required fields explicitly. Preserve native paths.
  - Done when sibling disks cannot inherit status, malformed input returns contextual errors and unknown categories/coverage are visible. Any MAME build string is accepted. Keep the sample listxml/catver fixtures as a regression sample and cover current listxml and CatVer shape with a small fixture.
- [x] **SELECT-01** Separate emulation status from category preferences and overrides. Default to good/imperfect runnable arcade machines; remove the implicit two-feature exclusion.
  - Done when minor sound/graphics issues are accepted, preliminary machines are excluded and every rule/override explains its effect. Decide genre preferences here.
  - Decision: no arcade-genre exclusions. Non-arcade CatVer groups stay out of the selected list, matched as a group rather than a substring, and each states why. The playable policy accepts good or imperfect driver and emulation, including imperfect sound and graphics. Strict (good only) is a library policy. Named system overrides keep a reason and an effect.
- [x] **MODEL-01** Separate selected machines from required storage. Retain parent, BIOS, device, ROM and disk relationships.
  - Done when supporting files can remain outside the visible game list and a clone remains selectable despite its parent's gameplay status.
  - Decision: `Catalog.selected` is the visible game list, chosen from each machine's own emulation policy. BIOS, devices, and other unselected machines stay in `catalog.roms`. Parent (`cloneof`), ROM parent (`romof`), device references, ROM dumps, and disks stay on every machine.
- [x] **DEPS-01** Resolve selected machines to required merged archives and support files. Replace status-based dependency demotion.
  - Done when clone-in-parent, BIOS/device, missing-reference and cycle cases are handled deterministically. Subset filters must retain dependencies.
  - Decision: `Catalog.dependencies` resolves a merged collection. A clone requires its ultimate `cloneof` archive and not its own name. Each other `romof` adds that ROM-parent archive. A `device_ref` adds an archive only when that device, or a `cloneof` / `romof` parent of it, stores ROM or disk entries. Missing names and cycles are sorted lists and do not stop the rest of the walk. Dump quality no longer changes the copy category. `Catalog::copy_roms` still applies a name-range subset to selected games, and also copies required archives that fall outside the range.
- [x] **CHD-01** Resolve shared disks and delta parents; replace blanket non-working-CHD omission.
  - Done when selected games retain all required disks, unused disks are omitted and missing media is reported separately from emulation quality.
  - Decision: `Catalog.disks` lists the CHD files selected games need. A `merge` or a matching sha1 keeps one ancestor copy. A clone disk with different content also keeps the same-named non-optional disk on its ultimate `cloneof` parent as the delta base. A differently named or optional parent disk is missing when that parent has disks. A parent with no disks is not missing. Optional disks are omitted. `nodump` and `baddump` stay required and do not change `Rom.category`. An unresolved merge, delta parent, or device is `missing`. Copying keeps a disk directory when that machine owns a required disk, including a parent outside a name-range subset, and omits other disk directories. `ignore_not_working_chd` is still accepted and no longer drops a required disk.

## Phase 3 Plan and export

- [x] **PLAN-01** Produce a sorted, reusable plan with selection reasons, media availability, paths, expected bytes, conflicts and metadata identity.
  - Done when identical inputs produce equivalent plans and missing media/conflicts are visible before copying. Recheck affected assumptions before execution. Decide persistence/schema here; JSON is proposed.
  - Decision: `Catalog::plan` writes a JSON plan (`--plan-path`). The document records the MAME build, playable or strict policy, and SHA-1 of the XML and CatVer files. Selected games inside the name range carry the policy's selection reason. Media entries are the archives and disk directories the copy would use, with source and destination paths, byte size, and present or missing. Archive rows and disk rows keep only their own reasons. Unresolved dependency and disk names, and cycles, are listed separately. Destination clashes are `conflicts` and still stop the copy before anything is written. A new `--plan-path` is written once and `Plan::recheck` checks that value before copying. An existing plan file is read and rechecked, not rebuilt or replaced. Recheck fails when a metadata hash, source size, missing file, or conflict list no longer matches the disk. EXPORT-01 moved that copy to `roms` and `rest` and bumped the plan to version 2.
- [x] **EXPORT-01** Execute the plan into `roms` and `rest`, and export exact selected machine names separately.
  - Done when output works independently of the full source, dependencies remain available and retained merged variants are documented. Missing CHDs stay on the selected list with their ROM archives copied; listing those CHDs for the user is REPORT-01, not this step.
  - Decision: a merged collection copies into two folders under the destination. `roms` receives every archive and CHD directory a selected game needs, including a parent, BIOS, device, or disk whose own machine is not selected. `rest` receives every other source file this run keeps, including a clone's own zip and a disk directory no selected game needs. A file is copied once. A name-range subset still leaves out-of-range names that are not required in the source. `selected.txt`, beside the two folders, lists the exact selected machine names inside the range, one per line, and is written only after every copy succeeds. If that file cannot be published, the report still returns with `all_ok` false. The report path cannot be that file. A missing CHD does not remove the game or skip its ROM archive. Retained merged archives can contain unselected variants; rebuilding those archives is a later decision. Split-set file selection is also later. The plan schema is version 2 because destinations and `selected_names_path` changed.
- [x] **REPORT-01** List missing CHDs in the report so they can be obtained individually.
  - Done when each CHD a selected game needs and the source does not contain is named on its own, with the game that needs it and the machine that should hold the file. The list is separate from copied files and from emulation status. A full CHD collection is the wrong way to fill these gaps: that download is on the order of 2TB, so the report must show only the disks this export is missing.
  - Decision: a missing CHD does not remove the game or skip its ROM archive. The Markdown report adds a `Missing CHDs` section when the list is not empty, plus a summary count. Each row names the selected game, the machine folder, the disk, and why it is required. Rows come from `catalog.disks.missing` and from required disk edges whose CHD file is not in the source. The section is omitted when nothing is missing. Selection is unchanged.
  - **2026 follow-up:** `require_complete_dependencies` defaults to **true**. Export and `selected.txt` use **launch-ready** games only; incomplete games appear under **Held back** with blockers. CLI/GUI: `--include-incomplete-games` or “Include games with missing ROM or CHD dependencies”. Optional **CHD source** root (second `--source-path` or GUI field) for CHDs outside the merged ROM folder.
- [ ] **VERIFY-01** Verify selected names using MAME 0.289 against output paths only. Test reduced merged import/launching in LaunchBox and the intended Android setup.
  - Done when parent, merged clone, BIOS/device and CHD cases pass and failures identify missing media. Keep optional CHD integrity checks separate; auditing does not prove device performance.
  - Sequencing: may follow Phase 4 GUI work. Real collection export and frontend/device checks stay required before the first intentional release (see [Remaining validation](#remaining-validation)).
- [x] **CLEAN-01** Separate CLI parsing from library configuration and filesystem work. Simplify map extensions/builders, remove unused report fields/clones and move test-only dependencies.
  - Done when a library caller can select/plan independently and reports distinguish games, archives, disks and operation outcomes.
- [x] **META-FETCH-01** Download MAME listxml and CatVer for a given set version (e.g. 0.289) into a local cache so users are not required to fetch them manually.
  - Done when `--mame-version` (or `ensure_metadata` in the library) populates metadata paths, cache location is configurable, and CatVer is checked against the requested version.
- [x] **PERF-01** Measure full-database time/memory and representative copying before adding streaming or concurrency.
  - Done when benchmarks exist, sample baselines are recorded, and full-database categorize is measured; see [performance.md](../performance.md). Optional: full plan/copy rows and peak RSS in that doc.
- [x] **PERF-02** Stream MAME listxml parsing without loading a full DOM.
  - Done when `run()` reads listxml incrementally (one `<machine>` at a time), tests pass, and full-metadata **categorize** time is updated in [performance.md](../performance.md). Re-benchmark plan/full and peak RSS there when you need fresh rows (optional follow-up).

## Phase 4 Curator GUI

Build a local installable desktop interface for preparing collections; LaunchBox and other emulators remain the launchers. Not an always-on server or a frontend replacement. A native Android curator UI is a separate decision. Collections on network-mounted paths use normal OS folder pickers; a browser-based remote admin UI is out of scope unless that product shape is chosen later.

Phase 4 may start before **VERIFY-01**. MAME and frontend verification remain on the release checklist below; the GUI uses the same Rust library as the CLI for path validation, planning, export, and reports.

- Decision: **Tauri 2**, **React**, **TypeScript**, and **Vite** for the embedded UI; **Rust** library shared with the CLI owns validation, planning, export, and reports. Vite is dev/build tooling only (bundled static assets in the shipped app, no Node runtime for users). Ship **Windows**, **macOS**, and **Linux** desktop builds. An all-Rust egui/eframe UI was considered and not chosen.

- [x] **GUI-01** After PLAN-01/CLEAN-01, prototype source selection, plan preview and progress on the chosen stack.
  - Done when Rust owns validation/operations, long work keeps the UI responsive and errors reach the user.
  - Decision: Tauri app under `src-tauri/` with the React UI in `gui/`. Commands run `ExportConfig::validate`, then `run_with_progress` and `Catalog::plan_with_progress` on a background thread and emit structured `plan-progress` events (phase **step/total** plus message; during export, **file entry progress** with throttled updates). Folder pickers use the dialog plugin. Export copy progress uses `Catalog::copy_with_plan_progress`. Linux CI has a dedicated `gui` job; job layout and when bundles run are documented in [releasing.md](../releasing.md#gui-release-delivery) and updated as CI is tuned.
- [x] **GUI-02** Add preferences, searchable game/dependency preview, export progress and results after GUI-01/EXPORT-01.
  - Done when exclusions/support files are explained and users review a plan before exporting through the shared core.
  - Decision: `review_export_plan` returns games, filtered support media, unresolved rows, and the full `Plan` JSON. `execute_export` reloads the catalog and runs `Catalog::copy_with_plan` after the user confirms (export uses `copy_with_plan_progress` for UI file progress). Preferences cover strict emulation, subset range, optional report path, and metadata refresh. Frontend build and GUI crate checks run in Linux CI when path filters include the GUI stack; see [releasing.md](../releasing.md#ci).
- [x] **GUI-03** Define cancellation, incomplete-export recovery and desktop distribution after GUI-02/SAFE-02.
  - Done when cancellation preserves source/completed output and recovery is tested. Decide signing, updates and GUI release assets here.
  - Decision: cooperative `CancelFlag` in the library (catalog load, plan scan, per-file copy). The GUI exposes **Cancel** via `cancel_background_work`. Completed copies stay; `selected.txt` is not published when cancelled. Recovery is documented in [gui-export-recovery.md](../gui-export-recovery.md) with a regression test for mid-copy cancel. Linux CI validates the GUI on filtered changes (PR: fmt, Clippy, check, test, release **`tauri build --no-bundle`**; master, manual CI, and CD: full **`npm run tauri build`** when the GUI job runs). Path filters, caching, and permissions are in `.github/workflows/ci.yml`; summary in [releasing.md](../releasing.md#ci). Signing, auto-update, and attaching GUI installers to CD are documented in [releasing.md](../releasing.md#gui-release-delivery) and deferred for the first GUI release.

## Remaining validation

- Real MAME 0.289 collection export and frontend/device checks at VERIFY-01.
- First intentional crates.io/GitHub release.

The review's 0.244 counts are historical test evidence, not counts or savings for the target 0.289 collection.
