# RFC 0002 Collection library

Status: **accepted direction**, not implemented. Tracks [TODO](../TODO.md); split-ROM work stays separate and later.

MAME **set curation** (export) remains the primary feature. This RFC is the **collection library**: discover what you own, enrich, browse in the app, export HTML.

## v1 scope

- **Platform:** **arcade (MAME)** only. Other platforms (SNES, Genesis, Amiga, …) add one at a time later with their own matchers and link templates.
- **Enrich:** **simplified** first (no image download). **Full** enrich (cached covers via APIs) is a later phase.
- **Split ROM storage mode:** not part of this RFC.

## User-facing behavior

- Top-level GUI: **MAME export** vs **Library** (separate flows).
- User configures **one or more folder roots per platform** (v1: arcade roots only).
- Pipeline: **Discover** → **Enrich** → **Browse** or **Export HTML**. One **SQLite** index; changing export style or re-enriching does **not** rediscover files.
- **Simplified enrich** may call **online sources for text/metadata** (titles, categories, emulation/playable where the provider gives it). It does **not** download box art or screenshots into the cache.
- Every game row should expose:
  - **Curated outbound links** when we can map the item (e.g. adb.arcadeitalia for MAME).
  - A **Google search** link always (fallback when mapping is wrong or missing).
- **Full enrich** (later): fetch/cache artwork; **HTML full export** references cached files or relative asset paths — **do not embed** thousands of images in one HTML file.

## Database

- Single library DB under **application user data** (survives app reinstall).
- UI: show **DB path**, optional **open folder**, **delete database** (with confirmation).

## HTML export

| Mode | Content |
| --- | --- |
| **Simplified** | Table/list: inventory + enriched text fields + curated links + Google search. No bundled images. |
| **Full** (later) | Same rows plus **links/paths** to cached artwork alongside the HTML (sidecar directory), not inline base64. |

## Code organization

Keep MAME export and library isolated so neither complicates the other:

| Area | Responsibility |
| --- | --- |
| **MAME curator** | Existing `core/`, export `Catalog`, plan/copy, Tauri commands used today for setup → review → export. |
| **Collection library** | New **`src/library/`** (discover, SQLite, enrich, link templates, HTML export). New Tauri commands and **`gui/library/`** (or equivalent route) for the Library top-level entry. |

Shared only where obvious: path validation, metadata cache paths, MAME listxml/CatVer loading for arcade matching.

## Arcade (v1) matching and links

- **Discover:** record files under user-configured arcade roots (stem, path, size, mtime).
- **Simplified enrich:** match stems to **cached MAME listxml** (+ CatVer when available) for title, category, driver/emulation **playable** fields; store provider id for link templates.
- **Curated links (v1):** hyperlinks to [adb.arcadeitalia](https://adb.arcadeitalia.net/) (Arcade Database Italia) from the MAME machine name — **no adb API in v1**, user opens the page in a browser.
- **Google:** `https://www.google.com/search?q=…` built from title + “MAME” (or machine name).

## Online providers

**Policy:** Default flows must work with **free** services and **no paid subscriptions**. Optional settings may use the user’s own **LaunchBox** / **EmuMovies** (or other) accounts or local files; those paths are **not required** for other users.

| Role | Provider | Cost | Notes |
| --- | --- | --- | --- |
| **Arcade identity / playable** | MAME listxml + CatVer (local/cache) | Free | v1 primary for arcade. |
| **Arcade links** | adb.arcadeitalia URLs | Free | Link-only in v1. |
| **Non-arcade default** | [ScreenScraper](https://www.screenscraper.fr/) API | Free account (+ dev id) | First choice when adding consoles/computers; hash or filename match. |
| **Non-arcade fallback** | [IGDB](https://api-docs.igdb.com/) | Free Twitch dev credentials | Second choice; rate limits. |
| **Hash → game id** | [Hasheous](https://hasheous.org/) | **Free** (hosted) | Optional: CRC/MD5/SHA1 lookup → IGDB / ScreenScraper ids; use only while the service remains free and terms allow. |
| **Local DAT match** | No-Intro / Redump files | Free | Offline integrity / naming; not a scraper. |
| **Link fallback** | Google search URL | Free | Every row. |
| **Full enrich images** | ScreenScraper, IGDB, SteamGridDB (optional key) | Mostly free tiers | Later phase; cache to user data, sidecar HTML. |

**Optional (personal, not default):**

| Provider | Use | Notes |
| --- | --- | --- |
| **LaunchBox** | Read local `LaunchBox.Metadata.db` and/or `Images` folders the user already has | No public games DB API; import path the user configures. |
| **EmuMovies** | Media via Sync/API with the user’s subscription | Developer API access; ship as **opt-in credentials**, not core. |

**Deferred / paid-only:** MobyGames API (paid tiers), scraping sites without APIs.

**Simplified enrich:** official APIs for **text** where configured; no image download. **Full enrich:** download media into cache. **Hasheous:** optional accelerator when hashes are computed; skip if unavailable.

## Implementation phases (checklist)

- [ ] **LIB-01** `src/library/` skeleton + SQLite schema + user-data DB path + delete/open in settings.
- [ ] **LIB-02** Discover arcade roots + incremental rescan (mtime/size).
- [ ] **LIB-03** Simplified enrich (listxml/CatVer + curated links + Google).
- [ ] **LIB-04** GUI Library top-level: configure roots, run discover/enrich, simple browse/search.
- [ ] **LIB-05** Export simplified HTML from DB.
- [ ] **LIB-06** (later) Full enrich + browse with artwork paths + full HTML template.
- [ ] **LIB-07** (later) Additional platforms one at a time.

## Open later (not blocking v1)

- Hash-based matching, DAT files for non-MAME platforms.
- adb API for arcade text (v1 stays adb **links** only).
- LaunchBox metadata / EmuMovies importers (opt-in).
- Hasheous client integration when adding hash-based console matching.
