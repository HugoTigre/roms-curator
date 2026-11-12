use std::env;
use std::error::Error;
use std::path::PathBuf;

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

fn user_data_dir() -> Result<PathBuf, Box<dyn Error>> {
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
