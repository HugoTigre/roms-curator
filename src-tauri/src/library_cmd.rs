//! Library screen commands. Separate from MAME export.

use std::process::Command;

use roms_curator::library::{
    default_library_db_path, delete_library_storage, discover_library_stopping,
    enrich_library_reporting, library_credentials_path, library_thumb_cache_dir, platform,
    ArcadeCatalog, BrowseHit, DiscoverStats, EnrichStats, LibraryDb, MetadataProvider,
    ScreenScraper, ScreenScraperCreds,
};
use roms_curator::{default_metadata_cache_dir, ensure_metadata};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::job::JobState;
use crate::progress::{emit_files, emit_phase};

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LibraryStatus {
    pub db_path: String,
    pub exists: bool,
    pub credentials_saved: bool,
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

fn empty_status(db_path: String) -> LibraryStatus {
    LibraryStatus {
        db_path,
        exists: false,
        credentials_saved: credentials_saved(),
        platforms: platform_rows(None).unwrap_or_default(),
    }
}

fn read_status() -> Result<LibraryStatus, String> {
    let db = LibraryDb::open_default().map_err(|error| error.to_string())?;
    let path_text = db
        .path()
        .map_err(|error| error.to_string())?
        .display()
        .to_string();
    Ok(LibraryStatus {
        db_path: path_text,
        exists: true,
        credentials_saved: credentials_saved(),
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

fn provider_for(
    db: &LibraryDb,
    library_id: i64,
    platform_id: &str,
) -> Result<Box<dyn MetadataProvider>, String> {
    let spec = platform::require(platform_id)?;
    match spec.id {
        "snes" => Ok(Box::new(ScreenScraper::new(creds_for_enrich()?))),
        "arcade" => {
            let version = db
                .mame_version(library_id)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "Set the MAME version for Arcade before scraping.".to_string())?;
            let paths = ensure_metadata(&version, &default_metadata_cache_dir(), false)
                .map_err(|error| error.to_string())?;
            let catalog = ArcadeCatalog::load(&paths.mame_xml, &paths.catver)
                .map_err(|error| error.to_string())?;
            Ok(Box::new(catalog))
        }
        other => Err(format!("Scraping is not implemented for {other}.")),
    }
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
        return Ok(empty_status(db_path.display().to_string()));
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
    let cancel = state.start(run_id);
    let app = app.clone();
    let result =
        tauri::async_runtime::spawn_blocking(move || scan_blocking(&app, &platform, &cancel))
            .await
            .map_err(|error| format!("Scan task failed: {error}"))?;
    state.clear(run_id);
    result
}

fn scan_blocking(
    app: &AppHandle,
    platform_id: &str,
    cancel: &roms_curator::CancelFlag,
) -> Result<DiscoverStats, String> {
    let spec = platform::require(platform_id)?;
    emit_phase(app, 1, 2, format!("Scanning {} folders", spec.name));
    let mut db = LibraryDb::open_default().map_err(|error| error.to_string())?;
    let library_id = db
        .find_library(spec.id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Add a {} folder first.", spec.name))?;
    let stats = discover_library_stopping(&mut db, library_id, Some(cancel))
        .map_err(|error| error.to_string())?;
    if stats.cancelled {
        emit_phase(app, 2, 2, "Scan stopped");
    } else {
        emit_phase(
            app,
            2,
            2,
            format!(
                "Scan finished: {} added, {} changed, {} unchanged, {} removed, {} restored",
                stats.added, stats.changed, stats.unchanged, stats.removed, stats.restored
            ),
        );
    }
    Ok(stats)
}

#[tauri::command]
pub async fn library_enrich(
    app: AppHandle,
    state: State<'_, JobState>,
    platform: String,
    run_id: u64,
) -> Result<EnrichStats, String> {
    let cancel = state.start(run_id);
    let app = app.clone();
    let result =
        tauri::async_runtime::spawn_blocking(move || enrich_blocking(&app, &platform, &cancel))
            .await
            .map_err(|error| format!("Enrich task failed: {error}"))?;
    state.clear(run_id);
    result
}

fn enrich_blocking(
    app: &AppHandle,
    platform_id: &str,
    cancel: &roms_curator::CancelFlag,
) -> Result<EnrichStats, String> {
    let spec = platform::require(platform_id)?;
    let cache = library_thumb_cache_dir().map_err(|error| error.to_string())?;
    let mut db = LibraryDb::open_default().map_err(|error| error.to_string())?;
    let library_id = db
        .find_library(spec.id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Add a {} folder first.", spec.name))?;
    emit_phase(app, 1, 1, format!("Loading {}", spec.name));
    let provider = provider_for(&db, library_id, spec.id)?;
    let app = app.clone();
    enrich_library_reporting(
        &mut db,
        library_id,
        &cache,
        provider.as_ref(),
        Some(cancel),
        &mut |pos, len, name| {
            emit_files(&app, 1, 1, pos, len, name);
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn library_search(
    platform: String,
    query: String,
    limit: u32,
) -> Result<Vec<BrowseHit>, String> {
    let spec = platform::require(&platform)?;
    let db_path = default_library_db_path().map_err(|error| error.to_string())?;
    let db = match LibraryDb::open_readonly(&db_path) {
        Ok(db) => db,
        Err(_) if !db_path.is_file() => return Ok(Vec::new()),
        Err(error) => return Err(error.to_string()),
    };
    let Some(library_id) = db
        .find_library(spec.id)
        .map_err(|error| error.to_string())?
    else {
        return Ok(Vec::new());
    };
    db.search(library_id, &query, limit)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn library_delete() -> Result<LibraryStatus, String> {
    let db_path = default_library_db_path().map_err(|error| error.to_string())?;
    delete_library_storage().map_err(|error| error.to_string())?;
    Ok(empty_status(db_path.display().to_string()))
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
    if !url.starts_with("https://adb.arcadeitalia.net/")
        && !url.starts_with("http://adb.arcadeitalia.net/")
    {
        return Err("That link is not an Arcade Database page.".into());
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
