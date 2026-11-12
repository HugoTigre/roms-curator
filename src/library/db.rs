use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

pub use super::paths::default_library_db_path;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS libraries (
    id INTEGER PRIMARY KEY,
    platform TEXT NOT NULL,
    name TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS roots (
    id INTEGER PRIMARY KEY,
    library_id INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    UNIQUE(library_id, path)
);
CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY,
    root_id INTEGER NOT NULL REFERENCES roots(id) ON DELETE CASCADE,
    relative_path TEXT NOT NULL,
    size INTEGER NOT NULL,
    mtime_secs INTEGER NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('present', 'removed')),
    needs_enrich INTEGER NOT NULL DEFAULT 1,
    UNIQUE(root_id, relative_path)
);
CREATE INDEX IF NOT EXISTS files_root_state ON files(root_id, state);
"#;

/// Whether a scanned file is still in its root folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileState {
    Present,
    Removed,
}

impl FileState {
    fn as_str(self) -> &'static str {
        match self {
            FileState::Present => "present",
            FileState::Removed => "removed",
        }
    }

    fn parse(value: &str) -> Result<Self, Box<dyn Error>> {
        match value {
            "present" => Ok(FileState::Present),
            "removed" => Ok(FileState::Removed),
            other => Err(format!("Unknown file state: {other}").into()),
        }
    }
}

/// One indexed file under a library root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRow {
    pub id: i64,
    pub root_id: i64,
    pub relative_path: String,
    pub size: u64,
    pub mtime_secs: i64,
    pub state: FileState,
    pub needs_enrich: bool,
}

/// Open connection to the collection library database.
pub struct LibraryDb {
    conn: Connection,
}

impl LibraryDb {
    /// Open or create the database, including parent directories.
    pub fn open(path: &Path) -> Result<Self, Box<dyn Error>> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        let db = LibraryDb { conn };
        db.migrate()?;
        Ok(db)
    }

    pub fn open_default() -> Result<Self, Box<dyn Error>> {
        Self::open(&default_library_db_path()?)
    }

    pub fn path(&self) -> Result<PathBuf, Box<dyn Error>> {
        let name: String = self.conn.query_row(
            "SELECT file FROM pragma_database_list WHERE name = 'main'",
            [],
            |row| row.get(0),
        )?;
        Ok(PathBuf::from(name))
    }

    fn migrate(&self) -> Result<(), Box<dyn Error>> {
        self.conn.execute_batch(SCHEMA)?;
        let mut versions = Vec::new();
        {
            let mut stmt = self.conn.prepare("SELECT version FROM schema_version")?;
            let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
            for row in rows {
                versions.push(row?);
            }
        }
        if versions.is_empty() {
            self.conn
                .execute("INSERT INTO schema_version (version) VALUES (1)", [])?;
        } else if versions != [1] {
            return Err(format!(
                "Unsupported library database version {} (this build reads version 1)",
                versions
                    .iter()
                    .map(|version| version.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .into());
        }
        Ok(())
    }

    /// Create a named library for a platform id such as `snes`.
    pub fn create_library(&self, platform: &str, name: &str) -> Result<i64, Box<dyn Error>> {
        self.conn.execute(
            "INSERT INTO libraries (platform, name) VALUES (?1, ?2)",
            params![platform, name],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Attach a folder. The path is canonicalized so later scans match the same root.
    ///
    /// A root cannot sit inside another root of the same library, or the other way around.
    pub fn add_root(&self, library_id: i64, root: &Path) -> Result<i64, Box<dyn Error>> {
        let canonical = root
            .canonicalize()
            .map_err(|error| format!("Cannot open library root {}: {error}", root.display()))?;
        let path = canonical
            .to_str()
            .ok_or_else(|| format!("Unsupported library root: {}", canonical.display()))?;
        for (_, existing) in self.roots(library_id)? {
            if existing == canonical
                || existing.starts_with(&canonical)
                || canonical.starts_with(&existing)
            {
                return Err(format!(
                    "Library root {} overlaps {}",
                    canonical.display(),
                    existing.display()
                )
                .into());
            }
        }
        self.conn.execute(
            "INSERT INTO roots (library_id, path) VALUES (?1, ?2)",
            params![library_id, path],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub(crate) fn roots(&self, library_id: i64) -> Result<Vec<(i64, PathBuf)>, Box<dyn Error>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, path FROM roots WHERE library_id = ?1 ORDER BY id")?;
        let rows = stmt.query_map([library_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                PathBuf::from(row.get::<_, String>(1)?),
            ))
        })?;
        let mut roots = Vec::new();
        for row in rows {
            roots.push(row?);
        }
        Ok(roots)
    }

    /// Files currently indexed under one root, including removed rows.
    pub fn files_for_root(&self, root_id: i64) -> Result<Vec<FileRow>, Box<dyn Error>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, root_id, relative_path, size, mtime_secs, state, needs_enrich
             FROM files WHERE root_id = ?1",
        )?;
        let rows = stmt.query_map([root_id], |row| {
            Ok(FileRow {
                id: row.get(0)?,
                root_id: row.get(1)?,
                relative_path: row.get(2)?,
                size: row.get::<_, i64>(3)? as u64,
                mtime_secs: row.get(4)?,
                state: FileState::parse(&row.get::<_, String>(5)?).expect("state check"),
                needs_enrich: row.get::<_, i64>(6)? != 0,
            })
        })?;
        let mut files = Vec::new();
        for row in rows {
            files.push(row?);
        }
        Ok(files)
    }

    pub(crate) fn connection_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// Count files in a library with the given state.
    pub fn count_files(&self, library_id: i64, state: FileState) -> Result<i64, Box<dyn Error>> {
        let count = self.conn.query_row(
            "SELECT COUNT(*) FROM files
             JOIN roots ON roots.id = files.root_id
             WHERE roots.library_id = ?1 AND files.state = ?2",
            params![library_id, state.as_str()],
            |row| row.get(0),
        )?;
        Ok(count)
    }
}
