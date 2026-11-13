use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

pub use super::paths::default_library_db_path;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY
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
CREATE TABLE IF NOT EXISTS games (
    file_id INTEGER PRIMARY KEY REFERENCES files(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    year TEXT,
    publisher TEXT,
    provider TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    thumb_path TEXT
);
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

/// One present file still waiting for metadata.
pub(crate) struct PendingFile {
    pub id: i64,
    pub path: PathBuf,
    pub relative_path: String,
    pub size: u64,
}

/// Metadata stored after a successful enrichment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameRecord {
    pub file_id: i64,
    pub title: String,
    pub year: Option<String>,
    pub publisher: Option<String>,
    pub provider: String,
    pub provider_id: String,
    pub thumb_path: Option<String>,
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
        let mut db = LibraryDb { conn };
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

    fn migrate(&mut self) -> Result<(), Box<dyn Error>> {
        self.conn.execute_batch(SCHEMA)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let version: Option<i64> = tx
            .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
            .optional()?;
        match version {
            None => {
                tx.execute("INSERT INTO schema_version (version) VALUES (2)", [])?;
            }
            Some(1) => {
                tx.execute(
                    "UPDATE schema_version SET version = 2 WHERE version = 1",
                    [],
                )?;
            }
            Some(2) => {}
            Some(version) => {
                return Err(format!(
                    "Unsupported library database version {version} (this build reads version 2)"
                )
                .into());
            }
        }
        tx.commit()?;
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

    pub(crate) fn library_platform(&self, library_id: i64) -> Result<String, Box<dyn Error>> {
        let platform = self
            .conn
            .query_row(
                "SELECT platform FROM libraries WHERE id = ?1",
                [library_id],
                |row| row.get(0),
            )
            .map_err(|_| format!("Library {library_id} was not found"))?;
        Ok(platform)
    }

    pub(crate) fn pending_files(
        &self,
        library_id: i64,
    ) -> Result<Vec<PendingFile>, Box<dyn Error>> {
        let mut stmt = self.conn.prepare(
            "SELECT files.id, roots.path, files.relative_path, files.size
             FROM files
             JOIN roots ON roots.id = files.root_id
             WHERE roots.library_id = ?1 AND files.state = 'present' AND files.needs_enrich = 1
             ORDER BY files.id",
        )?;
        let rows = stmt.query_map([library_id], |row| {
            let root = PathBuf::from(row.get::<_, String>(1)?);
            let relative_path: String = row.get(2)?;
            Ok(PendingFile {
                id: row.get(0)?,
                path: root.join(&relative_path),
                relative_path,
                size: row.get::<_, i64>(3)? as u64,
            })
        })?;
        let mut files = Vec::new();
        for row in rows {
            files.push(row?);
        }
        Ok(files)
    }

    pub(crate) fn save_game(
        &mut self,
        file_id: i64,
        game: &GameRecord,
    ) -> Result<(), Box<dyn Error>> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO games (file_id, title, year, publisher, provider, provider_id, thumb_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(file_id) DO UPDATE SET
                title = excluded.title,
                year = excluded.year,
                publisher = excluded.publisher,
                provider = excluded.provider,
                provider_id = excluded.provider_id,
                thumb_path = excluded.thumb_path",
            params![
                file_id,
                game.title,
                game.year,
                game.publisher,
                game.provider,
                game.provider_id,
                game.thumb_path
            ],
        )?;
        tx.execute("UPDATE files SET needs_enrich = 0 WHERE id = ?1", [file_id])?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn mark_enriched_missing(&mut self, file_id: i64) -> Result<(), Box<dyn Error>> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM games WHERE file_id = ?1", [file_id])?;
        tx.execute("UPDATE files SET needs_enrich = 0 WHERE id = ?1", [file_id])?;
        tx.commit()?;
        Ok(())
    }

    /// Stored metadata for one file, if enrichment found a game.
    pub fn game_for_file(&self, file_id: i64) -> Result<Option<GameRecord>, Box<dyn Error>> {
        let game = self
            .conn
            .query_row(
                "SELECT file_id, title, year, publisher, provider, provider_id, thumb_path
                 FROM games WHERE file_id = ?1",
                [file_id],
                |row| {
                    Ok(GameRecord {
                        file_id: row.get(0)?,
                        title: row.get(1)?,
                        year: row.get(2)?,
                        publisher: row.get(3)?,
                        provider: row.get(4)?,
                        provider_id: row.get(5)?,
                        thumb_path: row.get(6)?,
                    })
                },
            )
            .optional()?;
        Ok(game)
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
