//! Library screen commands. Separate from MAME export.

use std::process::Command;

use roms_curator::library::{
    default_library_db_path, delete_library_storage, discover_library_stopping,
    enrich_library_reporting, library_credentials_path, library_thumb_cache_dir, BrowseHit,
    DiscoverStats, EnrichStats, LibraryDb, LibraryOverview, ScreenScraper, ScreenScraperCreds,
};
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

fn status_from(db_path: String, exists: bool, overview: LibraryOverview) -> LibraryStatus {
    let credentials_saved = library_credentials_path()
        .ok()
        .is_some_and(|path| path.is_file());
    LibraryStatus {
        db_path,
        exists,
        credentials_saved,
        roots: overview.roots,
        present: overview.present,
        enriched: overview.enriched,
        pending: overview.pending,
    }
}

fn empty_status(db_path: String) -> LibraryStatus {
    status_from(
        db_path,
        false,
        LibraryOverview {
            roots: Vec::new(),
            present: 0,
            enriched: 0,
            pending: 0,
        },
    )
}

fn read_status() -> Result<LibraryStatus, String> {
    let db = LibraryDb::open_default().map_err(|error| error.to_string())?;
    let path_text = db
        .path()
        .map_err(|error| error.to_string())?
        .display()
        .to_string();
    let Some(library_id) = db.find_library("snes").map_err(|error| error.to_string())? else {
        return Ok(status_from(
            path_text,
            true,
            LibraryOverview {
                roots: Vec::new(),
                present: 0,
                enriched: 0,
                pending: 0,
            },
        ));
    };
    let overview = db.overview(library_id).map_err(|error| error.to_string())?;
    Ok(status_from(path_text, true, overview))
}

fn ensure_snes(db: &LibraryDb) -> Result<i64, String> {
    if let Some(id) = db.find_library("snes").map_err(|error| error.to_string())? {
        return Ok(id);
    }
    db.create_library("snes", "SNES")
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
pub fn library_add_root(path: String) -> Result<LibraryStatus, String> {
    let db = LibraryDb::open_default().map_err(|error| error.to_string())?;
    let library_id = ensure_snes(&db)?;
    db.add_root(library_id, std::path::Path::new(&path))
        .map_err(|error| error.to_string())?;
    read_status()
}

#[tauri::command]
pub fn library_remove_root(path: String) -> Result<LibraryStatus, String> {
    let db = LibraryDb::open_default().map_err(|error| error.to_string())?;
    let library_id = db
        .find_library("snes")
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Add a SNES folder first.".to_string())?;
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
    run_id: u64,
) -> Result<DiscoverStats, String> {
    let cancel = state.start(run_id);
    let app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || scan_blocking(&app, &cancel))
        .await
        .map_err(|error| format!("Scan task failed: {error}"))?;
    state.clear(run_id);
    result
}

fn scan_blocking(
    app: &AppHandle,
    cancel: &roms_curator::CancelFlag,
) -> Result<DiscoverStats, String> {
    emit_phase(app, 1, 2, "Scanning SNES folders");
    let mut db = LibraryDb::open_default().map_err(|error| error.to_string())?;
    let library_id = db
        .find_library("snes")
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Add a SNES folder first.".to_string())?;
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
    run_id: u64,
) -> Result<EnrichStats, String> {
    let cancel = state.start(run_id);
    let app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || enrich_blocking(&app, &cancel))
        .await
        .map_err(|error| format!("Enrich task failed: {error}"))?;
    state.clear(run_id);
    result
}

fn enrich_blocking(
    app: &AppHandle,
    cancel: &roms_curator::CancelFlag,
) -> Result<EnrichStats, String> {
    let creds = creds_for_enrich()?;
    let provider = ScreenScraper::new(creds);
    let cache = library_thumb_cache_dir().map_err(|error| error.to_string())?;
    let mut db = LibraryDb::open_default().map_err(|error| error.to_string())?;
    let library_id = db
        .find_library("snes")
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Add a SNES folder first.".to_string())?;
    let app = app.clone();
    enrich_library_reporting(
        &mut db,
        library_id,
        &cache,
        &provider,
        Some(cancel),
        &mut |pos, len, name| {
            emit_files(&app, 1, 1, pos, len, name);
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn library_search(query: String, limit: u32) -> Result<Vec<BrowseHit>, String> {
    let db_path = default_library_db_path().map_err(|error| error.to_string())?;
    let db = match LibraryDb::open_readonly(&db_path) {
        Ok(db) => db,
        Err(_) if !db_path.is_file() => return Ok(Vec::new()),
        Err(error) => return Err(error.to_string()),
    };
    let Some(library_id) = db.find_library("snes").map_err(|error| error.to_string())? else {
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
pub fn library_open_search(title: String) -> Result<(), String> {
    let query = format!("{title} SNES");
    let url = format!("https://www.google.com/search?q={}", url_query(&query));
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
