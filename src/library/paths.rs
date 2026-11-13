use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

/// SQLite file for the collection library.
///
/// Override with `ROMS_CURATOR_LIBRARY_DB`. Otherwise the file lives under the
/// platform user-data directory so it survives reinstalls.
pub fn default_library_db_path() -> Result<PathBuf, Box<dyn Error>> {
    if let Some(path) = env::var_os("ROMS_CURATOR_LIBRARY_DB") {
        return Ok(PathBuf::from(path));
    }
    Ok(user_data_dir()?.join("library.sqlite"))
}

/// Cached cover images for the library grid. Not embedded in the database.
pub fn library_thumb_cache_dir() -> Result<PathBuf, Box<dyn Error>> {
    Ok(user_data_dir()?.join("thumbs"))
}

/// ScreenScraper developer and member credentials saved by the Library screen.
pub fn library_credentials_path() -> Result<PathBuf, Box<dyn Error>> {
    Ok(user_data_dir()?.join("screenscraper.json"))
}

/// Delete the index, its SQLite sidecars, and cached covers.
///
/// Saved ScreenScraper credentials are left in place.
pub fn delete_library_storage() -> Result<(), Box<dyn Error>> {
    let db = default_library_db_path()?;
    remove_file_if_exists(&db)?;
    remove_file_if_exists(&sidecar(&db, "-wal"))?;
    remove_file_if_exists(&sidecar(&db, "-shm"))?;
    let thumbs = library_thumb_cache_dir()?;
    if thumbs.exists() {
        fs::remove_dir_all(&thumbs)?;
    }
    Ok(())
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_owned();
    name.push(suffix);
    path.with_file_name(name)
}

fn remove_file_if_exists(path: &Path) -> Result<(), Box<dyn Error>> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Cannot delete {}: {error}", path.display()).into()),
    }
}

pub(crate) fn user_data_dir() -> Result<PathBuf, Box<dyn Error>> {
    if let Some(dir) = env::var_os("ROMS_CURATOR_DATA_DIR") {
        return Ok(PathBuf::from(dir));
    }
    #[cfg(target_os = "macos")]
    let dir = PathBuf::from(env::var("HOME")?).join("Library/Application Support/roms-curator");
    #[cfg(target_os = "windows")]
    let dir = PathBuf::from(env::var("APPDATA")?).join("roms-curator");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let dir = xdg_data_dir()?;
    Ok(dir)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn xdg_data_dir() -> Result<PathBuf, Box<dyn Error>> {
    if let Some(xdg) = env::var_os("XDG_DATA_HOME") {
        return Ok(PathBuf::from(xdg).join("roms-curator"));
    }
    let home = env::var("HOME")?;
    Ok(PathBuf::from(home).join(".local/share/roms-curator"))
}
