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

## Collection library (browse / index)

**Separate product surface** from MAME export: optional; default install cost is **zero** (no scan, no DB, no background work until the user opts in). Primary app remains **MAME set curation**.

**Goal:** Scan configured library roots (MAME, consoles, computers, PC, …), store inventory + enrichment in **SQLite**, browse/search in the **desktop GUI** (no embedded web server, no requirement to run LaunchBox).

**Isolation (avoid cross-feature coupling):**

- Own Rust module (or crate) + GUI **mode/screen** distinct from export `setup → review → export`.
- No shared `ExportConfig` / plan path with export; at most reuse generic pieces (path validation, metadata cache dirs, MAME listxml loader for MAME rows only).
- Library DB path optional (e.g. user data dir); absent DB = feature unused.

**MVP data:**

- Libraries: root path, platform id, last scan time.
- Files: path, size, platform, optional hash (background/optional).
- Games (when matched): canonical id, title, **playable / condition** fields, cover path (local cache), short description optional.

**Playable / condition (per platform, keep honest):**

| Source | Use for “playable”? |
| --- | --- |
| **MAME listxml** (local/cache) | **Yes** — `driver` / emulation status for arcade; same facts as export policy (not a second guess). |
| **No-Intro / Redump DAT** (local) | **Integrity** — matched dump vs unknown/bad; not “fun to play”. |
| **ScreenScraper** | Metadata + art; optional community tags — not emulation status. |
| **IGDB / MobyGames** | Release info + art; no ROM verification. |

Store a small normalized enum on each row, e.g. `verified` \| `emulation_status` \| `unknown`, plus provider + raw label when present.

**Online resources (short list to integrate first):**

| Provider | Typical use | Auth | Notes |
| --- | --- | --- | --- |
| [ScreenScraper](https://www.screenscraper.fr/) | Title, description, **box cover**, screenshots | User + dev id | Strong console/computer coverage; hash or filename match. |
| [IGDB](https://www.igdb.com/api) | Title, summary, cover | Twitch client id/secret | Rate limits; good cross-platform metadata. |
| [SteamGridDB](https://www.steamgriddb.com/) | Alternate **covers** only | API key | Optional polish after primary cover. |
| **LaunchBox** (local `Images` + optional community DB) | Covers already on disk | None for files | Import paths user already has; avoid re-downloading. |
| **MAME**: listxml + CatVer + [adb.arcadeitalia](https://adb.arcadeitalia.net/) | Titles, categories, **screens** | None / scrape policy TBD | Arcade-specific; listxml for status. |
| **No-Intro / Redump DAT** | Match ROM → game name | None (files) | Offline; pair with one art provider above. |

Defer: full MobyGames, RetroAchievements, browser emulators, multi-user, any always-on HTTP API in the app.

**Essential work:**

- Schema + migrations (SQLite); scan job (filesystem walk).
- Platform plugins: matcher per system (MAME stem, DAT hash, filename rules).
- Enrichment job: fetch/cache **one cover per game** (+ optional screenshot); respect API keys in settings.
- GUI: library settings, scan/enrich progress, grid/list + search (read DB only).
- CLI/library: `scan-library`, `enrich-library` (optional; GUI can call same Rust API).

**Out of scope:** Launching games, save sync, reorganizing user folders, replacing RomM/LaunchBox for play.

## Other (unscheduled)

- Optional `mame -verifyroms` against export paths only.
- LaunchBox / Android smoke tests on `selected.txt` + `roms`.
- GUI signing, auto-update, CD installer assets (see [releasing.md](releasing.md)).
