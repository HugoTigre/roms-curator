use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde::Serialize;

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

/// Counts and folders for one platform library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibraryOverview {
    pub roots: Vec<String>,
    pub present: i64,
    pub enriched: i64,
    pub pending: i64,
}

/// One present file in a browse or search result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrowseHit {
    pub file_id: i64,
    pub name: String,
    pub title: String,
    pub year: Option<String>,
    pub publisher: Option<String>,
    pub thumb_path: Option<String>,
    pub needs_enrich: bool,
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

    /// Open an existing database for queries. A missing file is left missing.
    pub fn open_readonly(path: &Path) -> Result<Self, Box<dyn Error>> {
        if !path.is_file() {
            return Err(format!("Library database {} is missing", path.display()).into());
        }
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(LibraryDb { conn })
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

    /// Library id for `platform`, when a library row exists.
    pub fn find_library(&self, platform: &str) -> Result<Option<i64>, Box<dyn Error>> {
        let id = self
            .conn
            .query_row(
                "SELECT id FROM libraries WHERE platform = ?1 ORDER BY id LIMIT 1",
                [platform],
                |row| row.get(0),
            )
            .optional()?;
        Ok(id)
    }

    /// Folder paths and file counts for one library.
    pub fn overview(&self, library_id: i64) -> Result<LibraryOverview, Box<dyn Error>> {
        let roots = self
            .roots(library_id)?
            .into_iter()
            .map(|(_, path)| path.display().to_string())
            .collect();
        let present = self.count_files(library_id, FileState::Present)?;
        let pending: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM files
             JOIN roots ON roots.id = files.root_id
             WHERE roots.library_id = ?1 AND files.state = 'present' AND files.needs_enrich = 1",
            [library_id],
            |row| row.get(0),
        )?;
        let enriched: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM games
             JOIN files ON files.id = games.file_id
             JOIN roots ON roots.id = files.root_id
             WHERE roots.library_id = ?1 AND files.state = 'present'",
            [library_id],
            |row| row.get(0),
        )?;
        Ok(LibraryOverview {
            roots,
            present,
            enriched,
            pending,
        })
    }

    /// Remove one folder and the files indexed under it. Returns those file ids
    /// so the caller can drop their cached covers.
    pub fn remove_root(&self, library_id: i64, path: &str) -> Result<Vec<i64>, Box<dyn Error>> {
        let root_id: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM roots WHERE library_id = ?1 AND path = ?2",
                params![library_id, path],
                |row| row.get(0),
            )
            .optional()?;
        let Some(root_id) = root_id else {
            return Err(format!("Library folder is not registered: {path}").into());
        };
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM files WHERE root_id = ?1")?;
        let rows = stmt.query_map([root_id], |row| row.get(0))?;
        let mut ids = Vec::new();
        for row in rows {
            ids.push(row?);
        }
        self.conn
            .execute("DELETE FROM roots WHERE id = ?1", [root_id])?;
        Ok(ids)
    }

    /// Present files whose name, title, year, or publisher matches `query`.
    ///
    /// An empty query returns the first matches in title order. `limit` is
    /// clamped to 1..=200.
    pub fn search(
        &self,
        library_id: i64,
        query: &str,
        limit: u32,
    ) -> Result<Vec<BrowseHit>, Box<dyn Error>> {
        let limit = limit.clamp(1, 200);
        let pattern = like_contains(query.trim());
        let mut stmt = self.conn.prepare(
            "SELECT files.id, files.relative_path, files.needs_enrich,
                    games.title, games.year, games.publisher, games.thumb_path
             FROM files
             JOIN roots ON roots.id = files.root_id
             LEFT JOIN games ON games.file_id = files.id
             WHERE roots.library_id = ?1 AND files.state = 'present'
               AND (
                    ?2 = '%'
                    OR files.relative_path LIKE ?2 ESCAPE '\\'
                    OR IFNULL(games.title, '') LIKE ?2 ESCAPE '\\'
                    OR IFNULL(games.publisher, '') LIKE ?2 ESCAPE '\\'
                    OR IFNULL(games.year, '') LIKE ?2 ESCAPE '\\'
               )
             ORDER BY COALESCE(NULLIF(games.title, ''), files.relative_path) COLLATE NOCASE,
                      files.id
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![library_id, pattern, limit], |row| {
            let relative_path: String = row.get(1)?;
            let name = Path::new(&relative_path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(&relative_path)
                .to_string();
            let stored_title: Option<String> = row.get(3)?;
            let title = stored_title
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| name.clone());
            Ok(BrowseHit {
                file_id: row.get(0)?,
                name,
                title,
                year: row.get(4)?,
                publisher: row.get(5)?,
                thumb_path: row.get(6)?,
                needs_enrich: row.get::<_, i64>(2)? != 0,
            })
        })?;
        let mut hits = Vec::new();
        for row in rows {
            hits.push(row?);
        }
        Ok(hits)
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

fn like_contains(query: &str) -> String {
    if query.is_empty() {
        return "%".to_string();
    }
    let mut escaped = String::new();
    for ch in query.chars() {
        if ch == '\\' || ch == '%' || ch == '_' {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    format!("%{escaped}%")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::discover_library;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn search_matches_title_and_skips_removed_files() {
        let db_dir = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        for name in ["kept.zip", "other.zip"] {
            File::create(root.path().join(name))
                .unwrap()
                .write_all(b"rom")
                .unwrap();
        }
        let mut db = LibraryDb::open(&db_dir.path().join("library.sqlite")).unwrap();
        let library = db.create_library("snes", "SNES").unwrap();
        db.add_root(library, root.path()).unwrap();
        discover_library(&mut db, library).unwrap();
        let root_id = db.roots(library).unwrap()[0].0;
        let files = db.files_for_root(root_id).unwrap();
        let kept = files
            .iter()
            .find(|row| row.relative_path == "kept.zip")
            .unwrap();
        db.save_game(
            kept.id,
            &GameRecord {
                file_id: kept.id,
                title: "Super Metroid".into(),
                year: Some("1994".into()),
                publisher: Some("Nintendo".into()),
                provider: "fake".into(),
                provider_id: "1".into(),
                thumb_path: None,
            },
        )
        .unwrap();
        std::fs::remove_file(root.path().join("other.zip")).unwrap();
        discover_library(&mut db, library).unwrap();

        let hits = db.search(library, "metroid", 20).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Super Metroid");
        assert_eq!(hits[0].year.as_deref(), Some("1994"));
        assert!(!hits[0].needs_enrich);

        let by_file = db.search(library, "kept.zip", 20).unwrap();
        assert_eq!(by_file.len(), 1);
        assert!(db.search(library, "other.zip", 20).unwrap().is_empty());

        let overview = db.overview(library).unwrap();
        assert_eq!(overview.present, 1);
        assert_eq!(overview.enriched, 1);
        assert_eq!(overview.pending, 0);
    }

    #[test]
    fn remove_root_deletes_indexed_files_and_returns_their_ids() {
        let db_dir = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        File::create(root.path().join("game.zip"))
            .unwrap()
            .write_all(b"rom")
            .unwrap();
        let mut db = LibraryDb::open(&db_dir.path().join("library.sqlite")).unwrap();
        let library = db.create_library("snes", "SNES").unwrap();
        db.add_root(library, root.path()).unwrap();
        discover_library(&mut db, library).unwrap();
        let (root_id, root_path) = db.roots(library).unwrap().remove(0);
        let files = db.files_for_root(root_id).unwrap();
        let file_id = files[0].id;
        db.save_game(
            file_id,
            &GameRecord {
                file_id,
                title: "Game".into(),
                year: None,
                publisher: None,
                provider: "fake".into(),
                provider_id: "1".into(),
                thumb_path: None,
            },
        )
        .unwrap();

        let path = root_path.to_str().unwrap();
        let mut ids = db.remove_root(library, path).unwrap();
        ids.sort_unstable();
        assert_eq!(ids, vec![file_id]);
        assert!(db.roots(library).unwrap().is_empty());
        assert!(db.files_for_root(root_id).unwrap().is_empty());
        assert!(db.game_for_file(file_id).unwrap().is_none());
    }

    #[test]
    fn search_for_a_literal_underscore_does_not_match_another_character() {
        let db_dir = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        for name in ["a_b.zip", "axb.zip"] {
            File::create(root.path().join(name)).unwrap();
        }
        let db_path = db_dir.path().join("library.sqlite");
        let mut db = LibraryDb::open(&db_path).unwrap();
        let library = db.create_library("snes", "SNES").unwrap();
        db.add_root(library, root.path()).unwrap();
        discover_library(&mut db, library).unwrap();

        let hits = db.search(library, "_", 20).unwrap();
        let names: Vec<_> = hits.into_iter().map(|hit| hit.name).collect();
        assert_eq!(names, vec!["a_b.zip".to_string()]);

        let readonly = LibraryDb::open_readonly(&db_path).unwrap();
        let again = readonly.search(library, "_", 20).unwrap();
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].name, "a_b.zip");
    }

    #[test]
    fn readonly_open_leaves_a_missing_database_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.sqlite");
        assert!(LibraryDb::open_readonly(&path).is_err());
        assert!(!path.exists());
    }
}
