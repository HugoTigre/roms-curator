# TODO

Lightweight backlog. Shipped MAME work: [RFC 0001](rfcs/0001-repair-and-simplify.md).

## Collection library

**Spec:** [RFC 0002](rfcs/0002-collection-library.md) (decisions locked for v1).

**SNES** scrape still waits on a ScreenScraper developer id. **Arcade** (LIB-06) uses the local MAME list: title, year, manufacturer, playable, and an Arcade Database link. Add a platform in `src/library/platform.rs` and a provider under `src/library/platforms/`. Ignore `MAME 0.288 CHDs (merged)` for now.

Checklist: RFC 0002 (`LIB-01` …).

## Split ROM sets (storage mode)

**Later** — not in v1 library work.

Merged-only dependency resolution today (`src/models/dependencies.rs`). When picked up: `storage_mode`, parent+own archives for split, tests. Details unchanged from prior notes; see git history or ask before implementing.

## Other (unscheduled)

- Optional `mame -verifyroms` against export paths only.
- LaunchBox / Android smoke tests on `selected.txt` + `roms`.
- GUI signing, auto-update, CD installer assets (see [releasing.md](releasing.md)).
