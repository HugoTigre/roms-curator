# RFC 0002 Collection library

Status: **accepted direction**, implementation starting. Tracks [TODO](../TODO.md).

MAME **set curation** (export) remains the primary feature. This RFC is the **collection library**: discover, enrich, browse, export HTML.

## Principles

- **One SQLite index** (`rusqlite`) under **application user data** (path shown in UI; user can delete DB).
- **Discover is incremental for enrichment** — each scan still walks the configured roots and loads the rows for those roots. It records new, changed, restored, and removed files. Unchanged files stay in the index and are not queued for enrichment again.
- **Browse is not useful without a thumbnail** — at least **one cover image per game** in the GUI (cached on disk under user data, not embedded in SQLite blobs).
- **Free defaults:** [ScreenScraper](https://www.screenscraper.fr/) (+ dev registration) for console/computer metadata and covers; LaunchBox / EmuMovies **later**, opt-in only.
- **Testing:** ignore paths like `MAME 0.288 CHDs (merged)` until arcade+CHD library work is scoped separately.

## User-facing behavior

- Top-level GUI: **MAME export** vs **Library**.
- User assigns **folder roots per platform** (one platform wired at a time in code).
- Pipeline: **Discover** → **Enrich** → **Browse** (search) / **Export HTML**.
- Every row: **Google search** link; curated deep links when mapped (e.g. adb for MAME).

## Database

Single DB file in app user data. Tables at minimum:

- **libraries** — platform id and display name.
- **roots** — one row per folder the user assigned to that library (canonical path).
- **files** — stable id, `root_id`, relative path, size, mtime, optional hash (later); **discover state** (`present` / `removed`). Unique key is `(root_id, relative_path)`.
- **games** — enriched display fields (title, year, publisher, …), provider ids, **playable/condition**, `thumb_path` (filesystem cache).

A scan walks each root's tree and compares it to the files already stored for that root. Only **new, changed, and restored** files are marked `needs_enrich`. Unchanged files are counted and left alone.

## First platform (implementation order)

| Order | Platform | Why |
| --- | --- | --- |
| **1** | **SNES** | Flat No-Intro-style zips on test volume; ScreenScraper system id well defined; proves **discover + incremental + API enrich + thumbnail + search** end-to-end. |
| **2** | **Arcade (MAME)** | listxml/CatVer for text/playable; adb links; ScreenScraper arcade is weaker — thumbnails via ScreenScraper when match works, else placeholder + links. |
| **3** | **PC** | Later; messy layouts. |

## Enrich (per platform plugin)

- **SNES:** ScreenScraper (filename/hash when available) → title, year, publisher/genre, description; download **one box cover** into cache; IGDB fallback later on miss.
- **Arcade:** match stem to cached **listxml** (+ CatVer); emulation status; adb + Google links; thumbnail from ScreenScraper when matched else placeholder.

Enrich runs in **batches** with rate limits; skips rows already enriched unless file mtime changed or user forces refresh.

## Online providers

**Policy:** Core flows work with **free** ScreenScraper + IGDB (fallback) + local listxml (arcade). Optional: Hasheous (free), LaunchBox local DB/Images, EmuMovies — not required for others.

| Role | Provider |
| --- | --- |
| Console/computer metadata + **thumbnail** | ScreenScraper (primary) |
| Fallback metadata | IGDB |
| Arcade text / playable | MAME listxml + CatVer (local) |
| Arcade links | adb.arcadeitalia URLs |
| Hash → id (later) | Hasheous |
| Every row | Google search URL |
| Opt-in later | LaunchBox, EmuMovies |

## HTML export

- **Simplified:** text + links (no mass image embed).
- **Full-style:** reference cached thumbs beside HTML (sidecar folder).

## Code organization

| Module | Role |
| --- | --- |
| `core/` + export GUI | MAME curator (unchanged) |
| `src/library/` | discover, db, enrich providers, cache, html export |
| `gui/library/` or routes | Library top-level UI |

## Implementation phases

- [x] **LIB-01** `src/library/` + `rusqlite` schema + user-data DB path. Settings UI (show/delete) comes with the Library screen.
- [x] **LIB-02** Incremental **discover**: new, changed, restored, and removed files. Platform-agnostic; SNES is the first folder we will point it at.
- [ ] **LIB-03** **Enrich** SNES via ScreenScraper (credentials in settings): title, year, publisher, **thumbnail cache**.
- [ ] **LIB-04** GUI Library: pick root, scan, **search + grid with thumb**.
- [ ] **LIB-05** Export HTML from DB.
- [ ] **LIB-06** Arcade platform plugin (listxml + links + thumb/placeholder).
- [ ] **LIB-07** Additional platforms one at a time.

## Open later

- CHD / merged CHD roots in library.
- Hasheous, DAT files, adb API, LaunchBox/EmuMovies import.
