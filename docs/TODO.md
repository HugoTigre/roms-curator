# TODO

Lightweight backlog. RFC [0001](rfcs/0001-repair-and-simplify.md) tracks shipped work.

## Split ROM sets (storage mode)

Today the curator assumes a **merged** collection: a selected clone needs only its ultimate `cloneof` parent archive in `roms`, not its own zip (`src/models/dependencies.rs`).

**Goal:** Support **split** sources (and optionally **non-merged**) so export puts the right archives in `roms` and launch-ready checks match what MAME expects.

**Essential work:**

- Add `storage_mode` (`merged` default | `split` | `nonmerged`) on `ExportConfig` + CLI/GUI.
- Adjust dependency resolution: split clones need **parent + own** stem; non-merged clones rely on **own** (still resolve BIOS/device `romof`).
- Plan, copy, and launch-ready already follow `retained_archives` — no new copy engine if stems are correct.
- Tests with a small split fixture tree (parent + clone zips).

**Later / optional:** Auto-detect split when a clone’s archive exists in source (watch dummy clone zips in merged sets).

**Out of scope:** Rebuilding or filtering ROMs inside zips (use clrmame / RomVault).

## Other (unscheduled)

- Optional `mame -verifyroms` against export paths only.
- LaunchBox / Android smoke tests on `selected.txt` + `roms`.
- GUI signing, auto-update, CD installer assets (see [releasing.md](releasing.md)).
