//! Library screen commands. Separate from MAME export.

use std::process::Command;

use roms_curator::library::{
    allowed_game_page, default_library_db_path, delete_library_storage, discover_library_stopping,
    enrich_library_reporting, library_credentials_path, library_igdb_path, library_thumb_cache_dir,
    platform, store_metadata, ArcadeCatalog, ArcadeSource, BrowsePage, DiscoverStats,
    EnrichCallbacks, EnrichStats, Igdb, IgdbCandidate, IgdbCreds, LibraryDb, LibraryPrefs,
    MetadataProvider, PcSources, QuotaPause, ScreenScraper, ScreenScraperCreds, ScreenScraperQuota,
    UnmappedList,
};
use roms_curator::{default_metadata_cache_dir, ensure_metadata};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::job::JobState;
use crate::progress::{emit_files, emit_phase};

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LibraryStatus {
    pub db_path: String,
    pub exists: bool,
    pub credentials_saved: bool,
    pub igdb_credentials_saved: bool,
    pub download_covers: bool,
    pub screenscraper: ScreenScraperQuota,
    pub platforms: Vec<PlatformStatus>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PlatformStatus {
    pub id: String,
    pub name: String,
    pub uses_mame_version: bool,
    pub mame_version: Option<String>,
    pub roots: Vec<String>,
    pub present: i64,
    pub enriched: i64,
    pub pending: i64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CredentialInput {
    pub dev_id: String,
    pub dev_password: String,
    pub user: String,
    pub password: String,
}

fn credentials_saved() -> bool {
    library_credentials_path()
        .ok()
        .is_some_and(|path| path.is_file())
}

fn igdb_credentials_saved() -> bool {
    library_igdb_path().ok().is_some_and(|path| path.is_file())
}

fn load_prefs() -> Result<LibraryPrefs, String> {
    LibraryPrefs::load().map_err(|error| error.to_string())
}

fn empty_status(db_path: String) -> Result<LibraryStatus, String> {
    let prefs = load_prefs()?;
    Ok(LibraryStatus {
        db_path,
        exists: false,
        credentials_saved: credentials_saved(),
        igdb_credentials_saved: igdb_credentials_saved(),
        download_covers: prefs.download_covers,
        screenscraper: prefs.screenscraper,
        platforms: platform_rows(None).unwrap_or_default(),
    })
}

fn read_status() -> Result<LibraryStatus, String> {
    let db = LibraryDb::open_default().map_err(|error| error.to_string())?;
    let prefs = load_prefs()?;
    let path_text = db
        .path()
        .map_err(|error| error.to_string())?
        .display()
        .to_string();
    Ok(LibraryStatus {
        db_path: path_text,
        exists: true,
        credentials_saved: credentials_saved(),
        igdb_credentials_saved: igdb_credentials_saved(),
        download_covers: prefs.download_covers,
        screenscraper: prefs.screenscraper,
        platforms: platform_rows(Some(&db))?,
    })
}

fn platform_rows(db: Option<&LibraryDb>) -> Result<Vec<PlatformStatus>, String> {
    let mut rows = Vec::new();
    for spec in platform::all() {
        let (mame_version, roots, present, enriched, pending) = match db {
            Some(db) => match db
                .find_library(spec.id)
                .map_err(|error| error.to_string())?
            {
                Some(library_id) => {
                    let overview = db.overview(library_id).map_err(|error| error.to_string())?;
                    let version = if spec.uses_mame_version {
                        db.mame_version(library_id)
                            .map_err(|error| error.to_string())?
                    } else {
                        None
                    };
                    (
                        version,
                        overview.roots,
                        overview.present,
                        overview.enriched,
                        overview.pending,
                    )
                }
                None => (None, Vec::new(), 0, 0, 0),
            },
            None => (None, Vec::new(), 0, 0, 0),
        };
        rows.push(PlatformStatus {
            id: spec.id.to_string(),
            name: spec.name.to_string(),
            uses_mame_version: spec.uses_mame_version,
            mame_version,
            roots,
            present,
            enriched,
            pending,
        });
    }
    rows.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(rows)
}

fn ensure_library(db: &LibraryDb, platform_id: &str) -> Result<i64, String> {
    let spec = platform::require(platform_id)?;
    if let Some(id) = db
        .find_library(spec.id)
        .map_err(|error| error.to_string())?
    {
        return Ok(id);
    }
    db.create_library(spec.id, spec.name)
        .map_err(|error| error.to_string())
}

struct PreparedProvider {
    provider: Box<dyn MetadataProvider>,
    note: Option<String>,
    /// ScreenScraper requests one game in this pass will spend. Zero skips the allowance check.
    calls_per_game: u32,
}

fn provider_for(
    db: &LibraryDb,
    library_id: i64,
    platform_id: &str,
    download_covers: bool,
    cover_pass: bool,
) -> Result<PreparedProvider, String> {
    let spec = platform::require(platform_id)?;
    if let Some(system_id) = spec.screenscraper_id {
        return Ok(PreparedProvider {
            provider: Box::new(
                ScreenScraper::new(creds_for_enrich()?)
                    .download_covers(download_covers)
                    .for_system(spec.id, system_id),
            ),
            note: None,
            calls_per_game: if download_covers { 2 } else { 1 },
        });
    }
    match spec.id {
        "pc" => {
            let igdb = igdb_for_enrich(download_covers)?;
            let screenscraper = match creds_for_enrich() {
                Ok(creds) => Some(
                    ScreenScraper::new(creds)
                        .download_covers(download_covers)
                        .for_pc(),
                ),
                Err(error) if igdb.is_none() => return Err(error),
                Err(_) => None,
            };
            let calls = if igdb.is_some() {
                0
            } else if download_covers {
                2
            } else {
                1
            };
            Ok(PreparedProvider {
                provider: Box::new(PcSources::new(screenscraper, igdb)),
                note: None,
                calls_per_game: calls,
            })
        }
        "arcade" => {
            let version = db
                .mame_version(library_id)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "Set the MAME version for Arcade before scraping.".to_string())?;
            let paths = ensure_metadata(&version, &default_metadata_cache_dir(), false)
                .map_err(|error| error.to_string())?;
            let catalog = ArcadeCatalog::load(&paths.mame_xml, &paths.catver)
                .map_err(|error| error.to_string())?;
            if cover_pass {
                let covers = ScreenScraper::new(creds_for_enrich()?).download_covers(true);
                return Ok(PreparedProvider {
                    provider: Box::new(
                        ArcadeSource::new(catalog, Some(covers)).keep_existing_cover(),
                    ),
                    note: None,
                    calls_per_game: 2,
                });
            }
            let mut source = ArcadeSource::new(catalog, None);
            if download_covers {
                source = source.keep_existing_cover();
            }
            Ok(PreparedProvider {
                provider: Box::new(source),
                note: None,
                calls_per_game: 0,
            })
        }
        other => Err(format!("Scraping is not implemented for {other}.")),
    }
}

fn igdb_for_enrich(download_covers: bool) -> Result<Option<Igdb>, String> {
    let Some(creds) = IgdbCreds::load_stored().map_err(|error| error.to_string())? else {
        return Ok(None);
    };
    Igdb::stored(creds)
        .map(|client| Some(client.download_covers(download_covers)))
        .map_err(|error| error.to_string())
}

fn creds_for_enrich() -> Result<ScreenScraperCreds, String> {
    if let Some(stored) = ScreenScraperCreds::load_stored().map_err(|error| error.to_string())? {
        return Ok(stored);
    }
    ScreenScraperCreds::from_env().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn library_status() -> Result<LibraryStatus, String> {
    let db_path = default_library_db_path().map_err(|error| error.to_string())?;
    if !db_path.is_file() {
        return empty_status(db_path.display().to_string());
    }
    read_status()
}

#[tauri::command]
pub fn library_credentials() -> Result<CredentialInput, String> {
    let stored = ScreenScraperCreds::load_stored().map_err(|error| error.to_string())?;
    Ok(match stored {
        Some(creds) => CredentialInput {
            dev_id: creds.dev_id,
            dev_password: creds.dev_password,
            user: creds.user,
            password: creds.password,
        },
        None => CredentialInput {
            dev_id: String::new(),
            dev_password: String::new(),
            user: String::new(),
            password: String::new(),
        },
    })
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IgdbCredentialInput {
    pub client_id: String,
    pub client_secret: String,
}

#[tauri::command]
pub fn library_igdb_credentials() -> Result<IgdbCredentialInput, String> {
    let stored = IgdbCreds::load_stored().map_err(|error| error.to_string())?;
    Ok(match stored {
        Some(creds) => IgdbCredentialInput {
            client_id: creds.client_id,
            client_secret: creds.client_secret,
        },
        None => IgdbCredentialInput {
            client_id: String::new(),
            client_secret: String::new(),
        },
    })
}

#[tauri::command]
pub fn library_save_igdb_credentials(input: IgdbCredentialInput) -> Result<(), String> {
    let creds = IgdbCreds::new(input.client_id, input.client_secret);
    creds.save_stored().map_err(|error| error.to_string())
}

fn required_igdb() -> Result<Igdb, String> {
    let Some(creds) = IgdbCreds::load_stored().map_err(|error| error.to_string())? else {
        return Err("Save the IGDB Client ID and Client Secret in Settings.".into());
    };
    let prefs = load_prefs()?;
    Igdb::stored(creds)
        .map(|client| client.download_covers(prefs.download_covers))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn library_igdb_search(query: String) -> Result<Vec<IgdbCandidate>, String> {
    required_igdb()?
        .search_candidates(&query)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn library_igdb_assign(file_id: i64, game_id: i64) -> Result<(), String> {
    let mut db = LibraryDb::open_default().map_err(|error| error.to_string())?;
    db.require_present_file(file_id)
        .map_err(|error| error.to_string())?;
    let metadata = required_igdb()?
        .metadata_by_id(game_id)
        .map_err(|error| error.to_string())?;
    let cache = library_thumb_cache_dir().map_err(|error| error.to_string())?;
    store_metadata(&mut db, &cache, file_id, &metadata).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn library_save_credentials(input: CredentialInput) -> Result<(), String> {
    let creds = ScreenScraperCreds {
        dev_id: input.dev_id,
        dev_password: input.dev_password,
        user: input.user,
        password: input.password,
    };
    creds.save_stored().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn library_set_download_covers(enabled: bool) -> Result<(), String> {
    let mut prefs = load_prefs()?;
    prefs.download_covers = enabled;
    prefs.save().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn library_add_root(platform: String, path: String) -> Result<LibraryStatus, String> {
    let db = LibraryDb::open_default().map_err(|error| error.to_string())?;
    let library_id = ensure_library(&db, &platform)?;
    db.add_root(library_id, std::path::Path::new(&path))
        .map_err(|error| error.to_string())?;
    read_status()
}

#[tauri::command]
pub fn library_set_mame_version(
    platform: String,
    version: String,
) -> Result<LibraryStatus, String> {
    let spec = platform::require(&platform)?;
    if !spec.uses_mame_version {
        return Err(format!("{} does not use a MAME version.", spec.name));
    }
    let db = LibraryDb::open_default().map_err(|error| error.to_string())?;
    let library_id = ensure_library(&db, spec.id)?;
    let version = if version.trim().is_empty() {
        None
    } else {
        Some(version.as_str())
    };
    db.set_mame_version(library_id, version)
        .map_err(|error| error.to_string())?;
    read_status()
}

#[tauri::command]
pub fn library_remove_root(platform: String, path: String) -> Result<LibraryStatus, String> {
    let db = LibraryDb::open_default().map_err(|error| error.to_string())?;
    let spec = platform::require(&platform)?;
    let library_id = db
        .find_library(spec.id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Add a {} folder first.", spec.name))?;
    let ids = db
        .remove_root(library_id, &path)
        .map_err(|error| error.to_string())?;
    if let Ok(cache) = library_thumb_cache_dir() {
        for id in ids {
            let _ = std::fs::remove_file(cache.join(format!("{id}.jpg")));
            let _ = std::fs::remove_file(cache.join(format!("{id}.png")));
        }
    }
    read_status()
}

#[tauri::command]
pub async fn library_scan(
    app: AppHandle,
    state: State<'_, JobState>,
    platform: String,
    run_id: u64,
) -> Result<DiscoverStats, String> {
    let cancel = state
        .start(run_id, crate::job::JobKind::Scan)
        .map_err(str::to_string)?;
    let worker = app.clone();
    let result =
        tauri::async_runtime::spawn_blocking(move || scan_blocking(&worker, &platform, &cancel))
            .await
            .map_err(|error| log_error(&app, format!("Scan failed: {error}")))?;
    state.clear(run_id);
    result
}

fn scan_blocking(
    app: &AppHandle,
    platform_id: &str,
    cancel: &roms_curator::CancelFlag,
) -> Result<DiscoverStats, String> {
    let spec = platform::require(platform_id).map_err(|error| log_error(app, error))?;
    emit_phase(app, 1, 2, format!("Scanning {} folders", spec.name));
    let mut db = LibraryDb::open_default().map_err(|error| log_error(app, error))?;
    let library_id = db
        .find_library(spec.id)
        .map_err(|error| log_error(app, error))?
        .ok_or_else(|| log_error(app, format!("Add a {} folder first.", spec.name)))?;
    let mut named = 0u32;
    let mut hidden = 0u32;
    let stats = discover_library_stopping(&mut db, library_id, Some(cancel), &mut |message| {
        if named < 40 {
            emit_log(app, message);
            named += 1;
        } else {
            hidden += 1;
        }
    })
    .map_err(|error| log_error(app, format!("Scan failed: {error}")))?;
    if hidden > 0 {
        emit_log(
            app,
            &format!("Scan: {hidden} more file changes were not listed."),
        );
    }
    let summary = if stats.cancelled {
        "Scan stopped".to_string()
    } else {
        format!(
            "Scan finished: {} added, {} changed, {} unchanged, {} removed, {} restored",
            stats.added, stats.changed, stats.unchanged, stats.removed, stats.restored
        )
    };
    emit_log(app, &summary);
    if stats.cancelled {
        emit_phase(app, 2, 2, "Scan stopped");
    } else {
        emit_phase(app, 2, 2, summary);
    }
    Ok(stats)
}

#[tauri::command]
pub async fn library_enrich(
    app: AppHandle,
    state: State<'_, JobState>,
    platform: String,
    run_id: u64,
    file_ids: Option<Vec<i64>>,
    cover_pass: Option<bool>,
    ignore_quota: Option<bool>,
) -> Result<EnrichStats, String> {
    let cancel = state
        .start(run_id, crate::job::JobKind::Scrape)
        .map_err(str::to_string)?;
    let worker = app.clone();
    let cover_pass = cover_pass.unwrap_or(false);
    let ignore_quota = ignore_quota.unwrap_or(false);
    let result = tauri::async_runtime::spawn_blocking(move || {
        enrich_blocking(
            &worker,
            &platform,
            file_ids.as_deref(),
            cover_pass,
            ignore_quota,
            &cancel,
        )
    })
    .await
    .map_err(|error| log_error(&app, format!("Scrape failed: {error}")))?;
    state.clear(run_id);
    result
}

fn enrich_blocking(
    app: &AppHandle,
    platform_id: &str,
    file_ids: Option<&[i64]>,
    cover_pass: bool,
    ignore_quota: bool,
    cancel: &roms_curator::CancelFlag,
) -> Result<EnrichStats, String> {
    let spec = platform::require(platform_id).map_err(|error| log_error(app, error))?;
    let cache = library_thumb_cache_dir().map_err(|error| log_error(app, error))?;
    let mut db = LibraryDb::open_default().map_err(|error| log_error(app, error))?;
    let library_id = db
        .find_library(spec.id)
        .map_err(|error| log_error(app, error))?
        .ok_or_else(|| log_error(app, format!("Add a {} folder first.", spec.name)))?;
    emit_phase(app, 1, 1, format!("Loading {}", spec.name));
    let prefs = load_prefs().map_err(|error| log_error(app, error))?;
    let prepared = provider_for(&db, library_id, spec.id, prefs.download_covers, cover_pass)
        .map_err(|error| log_error(app, error))?;
    if let Some(note) = prepared.note.as_deref() {
        emit_log(app, note);
    }
    let calls_per_game = prepared.calls_per_game;
    let missing_covers = cover_pass && spec.id == "arcade";
    let ids = db
        .scrape_ids(library_id, file_ids, missing_covers)
        .map_err(|error| log_error(app, error))?;
    if ids.is_empty() {
        return Ok(EnrichStats::default());
    }

    let mut totals = EnrichStats::default();
    let mut completed = 0u64;
    let total = ids.len() as u64;
    if calls_per_game > 0 && !ignore_quota {
        match games_that_fit(calls_per_game) {
            Some(fits) if total > fits => {
                return Ok(paused(app, &ids, fits, totals));
            }
            None => {
                let mut run = IdRun {
                    app,
                    db: &mut db,
                    library_id,
                    cache: &cache,
                    provider: prepared.provider.as_ref(),
                    cancel,
                };
                let stats = run_ids(&mut run, &[ids[0]], &mut completed, total)?;
                fold_stats(&mut totals, &stats);
                if stats.cancelled || stats.failed > 0 {
                    return Ok(totals);
                }
                if let Some(fits) = games_that_fit(calls_per_game) {
                    let rest = &ids[1..];
                    if rest.len() as u64 > fits {
                        return Ok(paused(app, rest, fits, totals));
                    }
                }
                let rest = ids[1..].to_vec();
                if !rest.is_empty() {
                    let stats = run_ids(&mut run, &rest, &mut completed, total)?;
                    fold_stats(&mut totals, &stats);
                }
                return Ok(totals);
            }
            Some(_) => {}
        }
    }
    let mut run = IdRun {
        app,
        db: &mut db,
        library_id,
        cache: &cache,
        provider: prepared.provider.as_ref(),
        cancel,
    };
    let stats = run_ids(&mut run, &ids, &mut completed, total)?;
    fold_stats(&mut totals, &stats);
    let done = usize::try_from(totals.enriched + totals.missing).unwrap_or(ids.len());
    totals.visited_file_ids = ids.into_iter().take(done).collect();
    Ok(totals)
}

fn games_that_fit(calls_per_game: u32) -> Option<u64> {
    load_prefs()
        .ok()
        .and_then(|prefs| prefs.screenscraper.games_that_fit(calls_per_game))
}

fn paused(app: &AppHandle, ids: &[i64], fits: u64, mut stats: EnrichStats) -> EnrichStats {
    let left = ids.len() as u64;
    let message = if fits == 0 {
        format!("ScreenScraper has no requests left today. {left} games are still queued.")
    } else {
        format!("ScreenScraper can take {fits} more games today. {left} games are still queued.")
    };
    emit_log(app, &message);
    stats.quota_pause = Some(QuotaPause {
        games_left: left,
        fits,
        remaining_file_ids: ids.to_vec(),
    });
    stats
}

fn fold_stats(total: &mut EnrichStats, stats: &EnrichStats) {
    total.enriched += stats.enriched;
    total.missing += stats.missing;
    total.failed += stats.failed;
    total.cancelled |= stats.cancelled;
}

struct IdRun<'a> {
    app: &'a AppHandle,
    db: &'a mut LibraryDb,
    library_id: i64,
    cache: &'a std::path::Path,
    provider: &'a dyn MetadataProvider,
    cancel: &'a roms_curator::CancelFlag,
}

fn run_ids(
    run: &mut IdRun<'_>,
    ids: &[i64],
    completed: &mut u64,
    total: u64,
) -> Result<EnrichStats, String> {
    let files_app = run.app.clone();
    let log_app = run.app.clone();
    let mut totals = EnrichStats::default();
    for chunk in ids.chunks(200) {
        let done = *completed;
        let stats = enrich_library_reporting(
            run.db,
            run.library_id,
            run.cache,
            run.provider,
            Some(chunk),
            Some(run.cancel),
            &mut EnrichCallbacks {
                on_file: &mut |pos, _len, name| {
                    emit_files(&files_app, 1, 1, done + pos, total, name);
                },
                on_note: &mut |message| {
                    emit_log(&log_app, message);
                },
            },
        )
        .map_err(|error| {
            let message = error.to_string();
            if !message.contains("Enrichment stopped on ") {
                emit_log(run.app, &message);
            }
            message
        })?;
        fold_stats(&mut totals, &stats);
        *completed += stats.enriched + stats.missing;
        if stats.cancelled {
            break;
        }
    }
    Ok(totals)
}

#[derive(Clone, Serialize)]
struct LibraryLog {
    message: String,
}

fn log_error(app: &AppHandle, error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    emit_log(app, &message);
    message
}

fn emit_log(app: &AppHandle, message: &str) {
    let _ = app.emit(
        "library-log",
        LibraryLog {
            message: message.to_string(),
        },
    );
}

#[tauri::command]
pub fn library_search(
    platform: String,
    query: String,
    limit: u32,
    offset: i64,
    anchor: Option<i64>,
) -> Result<BrowsePage, String> {
    let spec = platform::require(&platform)?;
    let db_path = default_library_db_path().map_err(|error| error.to_string())?;
    let db = match LibraryDb::open_readonly(&db_path) {
        Ok(db) => db,
        Err(_) if !db_path.is_file() => {
            return Ok(BrowsePage {
                hits: Vec::new(),
                total: 0,
                offset: 0,
            });
        }
        Err(error) => return Err(error.to_string()),
    };
    let Some(library_id) = db
        .find_library(spec.id)
        .map_err(|error| error.to_string())?
    else {
        return Ok(BrowsePage {
            hits: Vec::new(),
            total: 0,
            offset: 0,
        });
    };
    db.search(library_id, &query, limit, offset, anchor)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn library_unmapped(platform: String) -> Result<UnmappedList, String> {
    let spec = platform::require(&platform)?;
    let empty = UnmappedList {
        total: 0,
        files: Vec::new(),
    };
    let db_path = default_library_db_path().map_err(|error| error.to_string())?;
    let db = match LibraryDb::open_readonly(&db_path) {
        Ok(db) => db,
        Err(_) if !db_path.is_file() => return Ok(empty),
        Err(error) => return Err(error.to_string()),
    };
    let Some(library_id) = db
        .find_library(spec.id)
        .map_err(|error| error.to_string())?
    else {
        return Ok(empty);
    };
    db.unmapped_files(library_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn library_delete() -> Result<LibraryStatus, String> {
    let db_path = default_library_db_path().map_err(|error| error.to_string())?;
    delete_library_storage().map_err(|error| error.to_string())?;
    empty_status(db_path.display().to_string())
}

#[tauri::command]
pub fn library_open_db_folder() -> Result<(), String> {
    let db_path = default_library_db_path().map_err(|error| error.to_string())?;
    let folder = db_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    std::fs::create_dir_all(&folder)
        .map_err(|error| format!("Cannot create {}: {error}", folder.display()))?;
    open_folder(&folder)
}

#[tauri::command]
pub fn library_open_search(platform: String, title: String) -> Result<(), String> {
    let spec = platform::require(&platform)?;
    let query = format!("{title} {}", spec.name);
    let url = format!("https://www.google.com/search?q={}", url_query(&query));
    open_url(&url)
}

#[tauri::command]
pub fn library_open_link(url: String) -> Result<(), String> {
    if !allowed_game_page(&url) {
        return Err("That page is not an IGDB or Arcade Database link.".into());
    }
    open_url(&url)
}

fn url_query(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn open_url(url: &str) -> Result<(), String> {
    let mut command = browse_command(url);
    let mut child = command
        .spawn()
        .map_err(|error| format!("Cannot open the browser: {error}"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

fn open_folder(path: &std::path::Path) -> Result<(), String> {
    let mut command = folder_command(path);
    let mut child = command
        .spawn()
        .map_err(|error| format!("Cannot open the folder: {error}"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

fn folder_command(path: &std::path::Path) -> Command {
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = Command::new("explorer");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = Command::new("xdg-open");
    command.arg(path);
    command
}

fn browse_command(url: &str) -> Command {
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = Command::new("cmd");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = Command::new("xdg-open");

    #[cfg(target_os = "windows")]
    command.args(["/C", "start", "", url]);
    #[cfg(not(target_os = "windows"))]
    command.arg(url);
    command
}
