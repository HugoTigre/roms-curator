use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::UNIX_EPOCH;

use rusqlite::params;
use same_file::Handle;

use super::db::{FileState, LibraryDb};

/// Result of one incremental scan of a library's roots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiscoverStats {
    pub added: u64,
    pub changed: u64,
    pub unchanged: u64,
    pub removed: u64,
    /// Previously removed files that are on disk again.
    pub restored: u64,
}

struct SeenFile {
    size: u64,
    mtime_secs: i64,
}

/// Scan every root of `library_id` and update the index.
///
/// Unchanged files keep their `needs_enrich` flag. New, changed, and restored
/// files are marked for enrichment. Files that disappeared are marked removed
/// and are not queued for enrichment.
pub fn discover_library(
    db: &mut LibraryDb,
    library_id: i64,
) -> Result<DiscoverStats, Box<dyn Error>> {
    let roots = db.roots(library_id)?;
    let mut stats = DiscoverStats::default();
    for (root_id, root_path) in roots {
        let seen = walk_files(&root_path)?;
        apply_root(db, root_id, &seen, &mut stats)?;
    }
    Ok(stats)
}

fn apply_root(
    db: &mut LibraryDb,
    root_id: i64,
    seen: &HashMap<String, SeenFile>,
    stats: &mut DiscoverStats,
) -> Result<(), Box<dyn Error>> {
    let tx = db.connection_mut().transaction()?;
    let existing = load_files(&tx, root_id)?;
    let mut insert = tx.prepare(
        "INSERT INTO files (root_id, relative_path, size, mtime_secs, state, needs_enrich)
         VALUES (?1, ?2, ?3, ?4, 'present', 1)",
    )?;
    let mut update_present = tx.prepare(
        "UPDATE files SET size = ?1, mtime_secs = ?2, state = 'present', needs_enrich = 1
         WHERE id = ?3",
    )?;
    let mut mark_removed =
        tx.prepare("UPDATE files SET state = 'removed', needs_enrich = 0 WHERE id = ?1")?;

    for (relative_path, file) in seen {
        match existing.get(relative_path) {
            None => {
                insert.execute(params![
                    root_id,
                    relative_path,
                    file.size as i64,
                    file.mtime_secs
                ])?;
                stats.added += 1;
            }
            Some(row) => {
                let same_bytes = row.size == file.size && row.mtime_secs == file.mtime_secs;
                if row.state == FileState::Removed {
                    update_present.execute(params![file.size as i64, file.mtime_secs, row.id])?;
                    stats.restored += 1;
                } else if same_bytes {
                    stats.unchanged += 1;
                } else {
                    update_present.execute(params![file.size as i64, file.mtime_secs, row.id])?;
                    stats.changed += 1;
                }
            }
        }
    }

    for (relative_path, row) in &existing {
        if seen.contains_key(relative_path) || row.state == FileState::Removed {
            continue;
        }
        mark_removed.execute([row.id])?;
        stats.removed += 1;
    }
    drop(insert);
    drop(update_present);
    drop(mark_removed);

    tx.commit()?;
    Ok(())
}

struct ExistingFile {
    id: i64,
    size: u64,
    mtime_secs: i64,
    state: FileState,
}

fn load_files(
    tx: &rusqlite::Transaction<'_>,
    root_id: i64,
) -> Result<HashMap<String, ExistingFile>, Box<dyn Error>> {
    let mut stmt = tx.prepare(
        "SELECT id, relative_path, size, mtime_secs, state FROM files WHERE root_id = ?1",
    )?;
    let rows = stmt.query_map([root_id], |row| {
        Ok((
            row.get::<_, String>(1)?,
            ExistingFile {
                id: row.get(0)?,
                size: row.get::<_, i64>(2)? as u64,
                mtime_secs: row.get(3)?,
                state: match row.get::<_, String>(4)?.as_str() {
                    "present" => FileState::Present,
                    _ => FileState::Removed,
                },
            },
        ))
    })?;
    let mut files = HashMap::new();
    for row in rows {
        let (path, file) = row?;
        files.insert(path, file);
    }
    Ok(files)
}

fn walk_files(root: &Path) -> Result<HashMap<String, SeenFile>, Box<dyn Error>> {
    let mut seen = HashMap::new();
    let mut stack = vec![root.to_path_buf()];
    let mut visited = HashSet::new();
    while let Some(dir) = stack.pop() {
        if !visited.insert(directory_id(&dir)?) {
            continue;
        }
        let entries = fs::read_dir(&dir)
            .map_err(|error| format!("Cannot read library folder {}: {error}", dir.display()))?;
        for entry in entries {
            let entry = entry
                .map_err(|error| format!("Cannot read entry in {}: {error}", dir.display()))?;
            let path = entry.path();
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                return Err(format!("Unsupported ROM name: {}", path.display()).into());
            };
            if name.starts_with('.') {
                continue;
            }
            let link_meta = fs::symlink_metadata(&path)
                .map_err(|error| format!("Cannot read type of {}: {error}", path.display()))?;
            if is_link(&link_meta) {
                // A dangling or looping link must not abort the rest of the root.
                let Ok(followed) = fs::metadata(&path) else {
                    continue;
                };
                if followed.is_file() {
                    record_file(&mut seen, root, &path, &followed)?;
                }
                continue;
            }
            if link_meta.is_dir() {
                stack.push(path);
                continue;
            }
            if link_meta.is_file() {
                record_file(&mut seen, root, &path, &link_meta)?;
            }
        }
    }
    Ok(seen)
}

fn record_file(
    seen: &mut HashMap<String, SeenFile>,
    root: &Path,
    path: &Path,
    meta: &fs::Metadata,
) -> Result<(), Box<dyn Error>> {
    let relative = relative_key(root, path)?;
    let mtime_secs = meta
        .modified()
        .map_err(|error| format!("Cannot read mtime for {}: {error}", path.display()))?
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    seen.insert(
        relative,
        SeenFile {
            size: meta.len(),
            mtime_secs,
        },
    );
    Ok(())
}

/// Directory symlinks, Windows junctions, and other reparse points.
fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn directory_id(path: &Path) -> Result<u64, Box<dyn Error>> {
    let mut hasher = DefaultHasher::new();
    Handle::from_path(path)
        .map_err(|error| format!("Cannot identify directory {}: {error}", path.display()))?
        .hash(&mut hasher);
    Ok(hasher.finish())
}

fn relative_key(root: &Path, path: &Path) -> Result<String, Box<dyn Error>> {
    let relative = path.strip_prefix(root).map_err(|_| {
        format!(
            "Path {} is not under library root {}",
            path.display(),
            root.display()
        )
    })?;
    let mut parts = Vec::new();
    for component in relative.components() {
        let Some(part) = component.as_os_str().to_str() else {
            return Err(format!("Unsupported ROM name: {}", path.display()).into());
        };
        parts.push(part);
    }
    if parts.is_empty() {
        return Err(format!("Empty relative path for {}", path.display()).into());
    }
    Ok(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{File, OpenOptions};
    use std::io::Write;
    use std::time::Duration;

    fn temp_db() -> (tempfile::TempDir, LibraryDb) {
        let dir = tempfile::tempdir().unwrap();
        let db = LibraryDb::open(&dir.path().join("library.sqlite")).unwrap();
        (dir, db)
    }

    fn enrich_flag(db: &LibraryDb, library: i64, name: &str) -> (FileState, bool) {
        let mut roots = db.roots(library).unwrap();
        let (root_id, _) = roots.remove(0);
        let row = db
            .files_for_root(root_id)
            .unwrap()
            .into_iter()
            .find(|row| row.relative_path == name)
            .unwrap_or_else(|| panic!("missing {name}"));
        (row.state, row.needs_enrich)
    }

    #[test]
    fn discover_tracks_new_changed_and_removed_files() {
        let (_db_dir, mut db) = temp_db();
        let root = tempfile::tempdir().unwrap();
        let library = db.create_library("snes", "SNES").unwrap();
        db.add_root(library, root.path()).unwrap();

        let game = root.path().join("game.zip");
        File::create(&game).unwrap().write_all(b"aaa").unwrap();
        File::create(root.path().join(".hidden.zip")).unwrap();
        File::create(root.path().join("._junk.zip")).unwrap();

        let first = discover_library(&mut db, library).unwrap();
        assert_eq!(first.added, 1);
        assert_eq!(first.removed, 0);
        assert_eq!(db.count_files(library, FileState::Present).unwrap(), 1);
        assert_eq!(
            enrich_flag(&db, library, "game.zip"),
            (FileState::Present, true)
        );

        db.connection_mut()
            .execute(
                "UPDATE files SET needs_enrich = 0 WHERE relative_path = 'game.zip'",
                [],
            )
            .unwrap();
        let second = discover_library(&mut db, library).unwrap();
        assert_eq!(second.added, 0);
        assert_eq!(second.unchanged, 1);
        assert_eq!(second.changed, 0);
        assert_eq!(
            enrich_flag(&db, library, "game.zip"),
            (FileState::Present, false)
        );

        let file = OpenOptions::new().write(true).open(&game).unwrap();
        let mtime = file.metadata().unwrap().modified().unwrap();
        file.set_modified(mtime + Duration::from_secs(5)).unwrap();
        drop(file);
        let mtime_only = discover_library(&mut db, library).unwrap();
        assert_eq!(mtime_only.changed, 1);
        assert_eq!(mtime_only.unchanged, 0);
        assert_eq!(
            enrich_flag(&db, library, "game.zip"),
            (FileState::Present, true)
        );

        File::create(&game).unwrap().write_all(b"bbbb").unwrap();
        let third = discover_library(&mut db, library).unwrap();
        assert_eq!(third.changed, 1);
        assert_eq!(third.unchanged, 0);
        assert_eq!(
            enrich_flag(&db, library, "game.zip"),
            (FileState::Present, true)
        );

        fs::remove_file(&game).unwrap();
        File::create(root.path().join("other.zip")).unwrap();
        let fourth = discover_library(&mut db, library).unwrap();
        assert_eq!(fourth.removed, 1);
        assert_eq!(fourth.added, 1);
        assert_eq!(db.count_files(library, FileState::Present).unwrap(), 1);
        assert_eq!(db.count_files(library, FileState::Removed).unwrap(), 1);
        assert_eq!(
            enrich_flag(&db, library, "game.zip"),
            (FileState::Removed, false)
        );
        assert_eq!(
            enrich_flag(&db, library, "other.zip"),
            (FileState::Present, true)
        );

        File::create(&game).unwrap().write_all(b"bbbb").unwrap();
        let fifth = discover_library(&mut db, library).unwrap();
        assert_eq!(fifth.restored, 1);
        assert_eq!(db.count_files(library, FileState::Present).unwrap(), 2);
        assert_eq!(db.count_files(library, FileState::Removed).unwrap(), 0);
        assert_eq!(
            enrich_flag(&db, library, "game.zip"),
            (FileState::Present, true)
        );
    }

    #[test]
    fn rejects_a_root_nested_in_another() {
        let (_db_dir, db) = temp_db();
        let parent = tempfile::tempdir().unwrap();
        let child = parent.path().join("nested");
        fs::create_dir(&child).unwrap();
        let library = db.create_library("snes", "SNES").unwrap();
        db.add_root(library, parent.path()).unwrap();
        let error = db.add_root(library, &child).unwrap_err().to_string();
        assert!(error.contains("overlaps"), "{error}");
    }

    #[test]
    fn nested_files_use_relative_keys() {
        let (_db_dir, mut db) = temp_db();
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("usa")).unwrap();
        File::create(root.path().join("usa/title.zip")).unwrap();
        let library = db.create_library("snes", "SNES").unwrap();
        db.add_root(library, root.path()).unwrap();
        let stats = discover_library(&mut db, library).unwrap();
        assert_eq!(stats.added, 1);
        let rows = db.files_for_root(db.roots(library).unwrap()[0].0).unwrap();
        assert_eq!(rows[0].relative_path, "usa/title.zip");
    }

    #[cfg(unix)]
    #[test]
    fn indexes_a_symlinked_zip_and_does_not_follow_directory_links() {
        use std::os::unix::fs::symlink;

        let (_db_dir, mut db) = temp_db();
        let root = tempfile::tempdir().unwrap();
        File::create(root.path().join("game.zip"))
            .unwrap()
            .write_all(b"rom")
            .unwrap();
        symlink("game.zip", root.path().join("alias.zip")).unwrap();
        symlink("missing.zip", root.path().join("dangling.zip")).unwrap();
        symlink(root.path(), root.path().join("loopdir")).unwrap();
        let library = db.create_library("snes", "SNES").unwrap();
        db.add_root(library, root.path()).unwrap();
        let stats = discover_library(&mut db, library).unwrap();
        assert_eq!(stats.added, 2);
        let names: Vec<_> = db
            .files_for_root(db.roots(library).unwrap()[0].0)
            .unwrap()
            .into_iter()
            .map(|row| row.relative_path)
            .collect();
        assert!(names.contains(&"game.zip".to_string()));
        assert!(names.contains(&"alias.zip".to_string()));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_non_utf8_relative_keys() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let root = Path::new("/library");
        let mut path = root.to_path_buf();
        path.push(OsString::from_vec(vec![0xff, 0xfe]));
        let error = relative_key(root, &path).unwrap_err().to_string();
        assert!(error.contains("Unsupported ROM name"), "{error}");
    }
}
