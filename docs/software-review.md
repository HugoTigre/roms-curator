# roms-curator software review (summary)

**Historical.** Static review of **0.1.5** (2 Nov 2026). The refactor is tracked in [RFC 0001](rfcs/0001-repair-and-simplify.md). For how to use the tool today, see the [README](../README.md).

## What we wanted

- **Source:** merged MAME **0.289** ROMs + CHDs (CHDs may be a second folder).
- **Selection:** playable arcade games (good/imperfect emulation); frontends get a **name list**, MAME gets a **`roms`** folder with every required archive and CHD.
- **Output:** smaller tree you can run without the full source (`roms`, `rest`, `selected.txt`). Merged zips stay whole; unselected clones may remain inside required archives.

## What 0.1.5 did wrong (high level)

| Area | Problem |
| --- | --- |
| **Safety** | Weak path overlap checks; overwrites; simulation touched disks. |
| **Dependencies** | Copy loop did not resolve parents, BIOS, devices, CHDs for selected games. |
| **CHDs** | `ignore_not_working_chd` could drop disks another game still needed. |
| **Semantics** | “Success” ignored missing media; four folders conflated game list vs storage. |
| **Selection** | Extra sound+graphics rule, status demotion bugs, CatVer/disk parse quirks. |
| **UX** | Little explanation of why a game was in or out; no reusable plan. |

Counts quoted against **0.244** fixtures (e.g. 749 games dropped by the sound/graphics rule) were **policy impact on that corpus**, not measured savings on a 0.289 collection.

## Target design (still the mental model)

`metadata → selection → dependency resolution → plan → copy` (optional MAME verify later).

Games on the **list** are separate from **bytes on disk**. Whole-archive copy for merged sets; rebuild archives with a ROM manager only if you need smaller zips.

## What shipped since

RFC 0001 implemented the table above: safe copy, `Catalog` + dependencies/disks, `roms`/`rest`, version-2 plans, launch-ready `selected.txt`, reports, metadata fetch, streaming XML, GUI. Full **0.289** export has been run successfully; launcher smoke tests remain optional.

## Alternatives considered (not chosen)

[clrmame](https://mamedev.emulab.it/clrmamepro/binaries/readme.html), [MAME Curator](https://github.com/milnet01/mame-curator), [Retro-Refiner](https://github.com/atkins/retro-refiner), [RomVault](https://wiki.romvault.com/doku.php?id=mame_listxml) can help audit or rebuild sets but do not match this **playable-arcade + merged export + CatVer policy** stack out of the box. We kept a thin Rust curator instead of rebuilding clrmame.

## Full audit text

The long-form review (per-finding write-ups, line citations, and 0.244 characterization detail) lived in git history before this summary. Ask for `docs/software-review.md` at commit `3786b54` or earlier on `main` if you need the original document.
