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

**Separate product surface** from MAME export: optional; default install cost is **zero** until the user runs a library action. Primary app remains **MAME set curation**.

**One index:** a single **SQLite** database per library (or one global DB with library ids). **Discovery** walks the filesystem once and records files; **enrichment** and **export** read that index — no second full discovery for a “lite” vs “full” path.

**Isolation (avoid cross-feature coupling):**

- Own Rust module (or crate) + GUI **mode/screen** distinct from export `setup → review → export`.
- No shared `ExportConfig` / plan path with export; at most reuse generic pieces (path validation, metadata cache dirs, MAME listxml loader, report-style **HTML/Markdown writers**).

**Pipeline (three steps, reusable):**

1. **Discover** — scan configured roots → upsert files in SQLite (path, platform, size, mtime; optional hash in a later pass). Incremental rescans only diff changed paths.
2. **Enrich** — user chooses **simplified** or **full** when starting (or upgrading) enrichment; same rows, different columns filled:
   - **Simplified:** local matchers only (MAME stem/listxml, DAT filename/hash if configured), titles/playable from **local** metadata, store **outbound link templates** (adb, IGDB/ScreenScraper search, etc.). **No** media download, **no** API calls.
   - **Full:** call providers below; cache cover (and optional screenshot) on disk; store short description when returned.
   - User can run **simplified first**, then **re-enrich to full** later without rediscovering files.
3. **Export / browse** — **GUI** searches SQLite (no web server). **Export HTML** (optional Markdown) is a view over the DB:
   - **Simplified export:** minimal columns + external links; embed no images (or only `file://` paths if already cached).
   - **Full export:** include cached artwork paths / inline refs where present.

Discovery is **not** repeated for export format; export only chooses which columns/media to include.

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

**Essential work (order):**

1. SQLite schema; **discover** job + incremental rescan.
2. **Enrich** profiles (simplified vs full) + matchers per platform.
3. GUI browse (read DB) + **export HTML** with simplified/full template.
4. Optional CLI: `library discover`, `library enrich`, `library export`.

**Out of scope:** Launching games, save sync, reorganizing user folders, replacing RomM/LaunchBox for play.

## Other (unscheduled)

- Optional `mame -verifyroms` against export paths only.
- LaunchBox / Android smoke tests on `selected.txt` + `roms`.
- GUI signing, auto-update, CD installer assets (see [releasing.md](releasing.md)).
