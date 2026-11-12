# RFC 0001 Repair and simplify roms curator

Status: **implemented** (Phases 0–4). The [software review](../software-review.md) is a historical pre-refactor audit; behavior below is what ships today.

## Target

- Merged MAME **0.289** collection → **`roms`** (everything launch-ready games need) and **`rest`** (everything else this run keeps), so the full source can be removed after a successful export. **`selected.txt`** lists launch-ready machine names for frontends (LaunchBox, playlists, etc.).
- Default selection: good/imperfect arcade drivers; minor sound/graphics issues accepted. Parents, BIOSes, devices, and required CHDs are copied into `roms` even when those machines are not on the game list.
- Rust library + CLI + **Tauri** desktop curator. Launchers stay external.
- Later decisions (not in scope): split **storage** mode, **collection library** ([RFC 0002](0002-collection-library.md)), rebuilding merged archives, device performance filters, optional `mame -verifyroms` / `chdman verify` integration.

## Checklist

### Phase 0 — baseline

- [x] **BASE-01** CI: fmt, Clippy, build, test (Linux, Windows, macOS).
- [x] **BASE-02** Tag-only CD; readable action versions.
- [x] **BASE-03** [Release rules](../releasing.md).
- [x] **BASE-04** This checklist.
- [x] **BASE-05** Hosted multi-OS CI green.

### Phase 1 — safe file operations

- [x] **SAFE-01** Reject overlapping/aliased source, output, and report paths.
- [x] **SAFE-02** Preflight conflicts; temp sibling publish; no CHD merge / overwrite.
- [x] **SAFE-03** Report failures; nonzero CLI on copy/report errors.
- [x] **SAFE-04** Read-only simulation (`Planned` only).

### Phase 2 — metadata and selection

- [x] **META-01** CatVer sections, disk fields, contextual parse errors.
- [x] **SELECT-01** Playable vs strict policy; no sound+graphics count rule; non-arcade CatVer groups excluded as wholes.
- [x] **MODEL-01** `Catalog.selected` vs required storage on `catalog.roms`.
- [x] **DEPS-01** Merged-archive dependency walk; subset still pulls required archives.
- [x] **CHD-01** Required disks, merge/delta rules; `ignore_not_working_chd` no longer drops required CHDs.

### Phase 3 — plan and export

- [x] **PLAN-01** Version-2 JSON plan; write once, `Plan::recheck` before copy.
- [x] **EXPORT-01** Copy into `roms` / `rest`; `selected.txt` after all copies succeed.
- [x] **REPORT-01** **Missing CHDs** in Markdown report.
- [x] **CLEAN-01** `ExportConfig` library boundary; slimmer reports.
- [x] **META-FETCH-01** `--mame-version` cached metadata download.
- [x] **PERF-01** Benchmarks in [performance.md](../performance.md).
- [x] **PERF-02** Streaming listxml parse.

### Phase 4 — curator GUI

- [x] **GUI-01** Plan preview and progress (`src-tauri/`, `gui/`).
- [x] **GUI-02** Preferences, searchable preview, confirm-then-export.
- [x] **GUI-03** Cancellation and [recovery](../gui-export-recovery.md); installer CI on main/CD.

### Validation

- [x] **VERIFY-01 (export)** Full **0.289** merged export exercised end-to-end on a real collection (copy, `roms`/`rest` split, `selected.txt`, reports). Games run from `roms` when media is present.
- [ ] **VERIFY-01 (optional)** Spot-check LaunchBox or Android import against `selected.txt` / `roms`. Not required for the first release: the tool only copies files frontends already understand. Automated `mame -verifyroms` remains a possible follow-up, not part of this RFC.

## Behavior reference

| Topic | Behavior |
| --- | --- |
| **Launch list** | By default, `require_complete_dependencies` is true: only games with required ROM and CHD media in the source appear in **`selected.txt`**. Others show as **Held back** in the GUI/plan. Held-back ROM zips copy to **`rest`** when present; use `--include-incomplete-games=true` to export them like launch-ready games (`roms` + list). |
| **CHD roots** | Pass a second path in `--source-path` when CHDs live outside the ROM folder. |
| **Subset** | `--subset-start` / `--subset-end` limit the game list; required parent/BIOS/device archives and CHD dirs for in-range games still land in `roms`. |
| **Plan** | `--plan-path` must be outside source and destination; conflicts block copy. |
| **Cancelled export** | Completed files stay; `selected.txt` is not written. |

User-facing detail: [README](../../README.md), [CLI](../app-run-instructions.md), [library](../lib-usage-instructions.md).

## Before first intentional release

- Tag **`v0.2.0`** (or next) per [releasing.md](../releasing.md): CLI binaries on CD; GUI installers optional/manual until CD attaches them.

Historical 0.244 counts in the software review are fixture evidence only, not 0.289 savings estimates.
