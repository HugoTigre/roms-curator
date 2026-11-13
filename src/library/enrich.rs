//! Fill `needs_enrich` rows from a metadata provider and cache one thumbnail.

use std::error::Error;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use sha1::{Digest, Sha1};
use zip::ZipArchive;

use serde::Serialize;

use crate::CancelFlag;

use super::db::{GameRecord, LibraryDb};

/// How many files one enrich pass updated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct EnrichStats {
    pub enriched: u64,
    /// The provider had no match. The file is not queued again.
    pub missing: u64,
    /// Stopped on the first request or I/O error. That file stays queued.
    pub failed: u64,
    /// The user cancelled. Files not yet visited stay queued.
    pub cancelled: bool,
}

/// One matched game. `thumbnail` is the raw image bytes, if the provider had a cover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameMetadata {
    pub provider: String,
    pub provider_id: String,
    pub title: String,
    pub year: Option<String>,
    pub publisher: Option<String>,
    pub thumbnail: Option<Vec<u8>>,
    pub playable: Option<bool>,
    pub link: Option<String>,
    /// Arcade export category. Other platforms leave this empty.
    pub condition: Option<GameCondition>,
}

/// How an arcade set relates to the export list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameCondition {
    Working,
    NotWorking,
    Bios,
    System,
}

impl GameCondition {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::NotWorking => "not_working",
            Self::Bios => "bios",
            Self::System => "system",
        }
    }

    pub fn is_playable(self) -> bool {
        matches!(self, Self::Working)
    }
}

/// Result of looking up one ROM file name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    Found(GameMetadata),
    Missing,
}

/// Checksum ScreenScraper uses to identify a ROM.
///
/// A zip sends the CRC32 stored for the uncompressed entry. A loose ROM sends
/// the SHA1 of the file itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RomChecksum {
    /// Lowercase hex SHA1.
    Sha1(String),
    /// Uppercase hex CRC32.
    Crc32(String),
}

/// The file a provider should match.
///
/// `size` and `checksum` describe the ROM ScreenScraper indexed: the largest
/// uncompressed entry inside a zip, or the file itself otherwise.
pub struct RomQuery<'a> {
    pub name: &'a str,
    pub size: u64,
    /// Set when the provider matches file bytes. Arcade matches the file name.
    pub checksum: Option<&'a RomChecksum>,
}

/// Platform-specific metadata source.
///
/// ScreenScraper is the SNES implementation. Arcade reads a local MAME list.
/// A new platform adds a type that implements this trait and a row in the
/// platform registry.
pub trait MetadataProvider {
    fn supports(&self, platform: &str) -> bool;
    /// When false, enrichment does not hash the file before lookup.
    fn wants_content_identity(&self) -> bool {
        true
    }
    fn lookup(&self, platform: &str, rom: &RomQuery) -> Result<Lookup, Box<dyn Error>>;
}

/// Enrich every present file still marked `needs_enrich`.
///
/// Unchanged files are not passed to the provider. A missing match clears the
/// flag. A provider error stops the pass so later files can be retried.
pub fn enrich_library(
    db: &mut LibraryDb,
    library_id: i64,
    cache_dir: &Path,
    provider: &dyn MetadataProvider,
) -> Result<EnrichStats, Box<dyn Error>> {
    enrich_library_reporting(db, library_id, cache_dir, provider, None, &mut |_, _, _| {})
}

/// Same as [`enrich_library`], plus cancel and per-file progress.
///
/// `on_file` receives the 1-based index, the pending total, and the relative path
/// before that file is looked up.
pub fn enrich_library_reporting(
    db: &mut LibraryDb,
    library_id: i64,
    cache_dir: &Path,
    provider: &dyn MetadataProvider,
    cancel: Option<&CancelFlag>,
    on_file: &mut dyn FnMut(u64, u64, &str),
) -> Result<EnrichStats, Box<dyn Error>> {
    let platform = db.library_platform(library_id)?;
    if !provider.supports(&platform) {
        return Err(format!("Enrichment is not implemented for platform {platform}").into());
    }
    fs::create_dir_all(cache_dir)?;
    let pending = db.pending_files(library_id)?;
    let total = pending.len() as u64;
    let mut stats = EnrichStats::default();
    for (index, file) in pending.into_iter().enumerate() {
        if cancel.is_some_and(CancelFlag::poll) {
            stats.cancelled = true;
            return Ok(stats);
        }
        let rom_name = Path::new(&file.relative_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(file.relative_path.as_str());
        on_file((index as u64) + 1, total, &file.relative_path);
        let content = if provider.wants_content_identity() {
            Some(rom_identity(&file.path, file.size)?)
        } else {
            None
        };
        let rom = RomQuery {
            name: rom_name,
            size: content.as_ref().map(|item| item.size).unwrap_or(file.size),
            checksum: content.as_ref().map(|item| &item.checksum),
        };
        match provider.lookup(&platform, &rom) {
            Ok(Lookup::Found(metadata)) => {
                let thumb_path =
                    write_thumbnail(cache_dir, file.id, metadata.thumbnail.as_deref())?;
                db.save_game(
                    file.id,
                    &GameRecord {
                        file_id: file.id,
                        title: metadata.title,
                        year: metadata.year,
                        publisher: metadata.publisher,
                        provider: metadata.provider,
                        provider_id: metadata.provider_id,
                        thumb_path: thumb_path.map(|path| path.display().to_string()),
                        playable: metadata.playable,
                        link: metadata.link,
                        condition: metadata
                            .condition
                            .map(|condition| condition.as_str().to_string()),
                    },
                )?;
                stats.enriched += 1;
            }
            Ok(Lookup::Missing) => {
                remove_cached_thumbnails(cache_dir, file.id)?;
                db.mark_enriched_missing(file.id)?;
                stats.missing += 1;
            }
            Err(error) => {
                stats.failed += 1;
                return Err(format!(
                    "Enrichment stopped on {} ({error}). {stats}",
                    file.relative_path
                )
                .into());
            }
        }
    }
    Ok(stats)
}

fn write_thumbnail(
    cache_dir: &Path,
    file_id: i64,
    bytes: Option<&[u8]>,
) -> Result<Option<PathBuf>, Box<dyn Error>> {
    remove_cached_thumbnails(cache_dir, file_id)?;
    let jpg = cache_dir.join(format!("{file_id}.jpg"));
    let png = cache_dir.join(format!("{file_id}.png"));
    let Some(bytes) = bytes else {
        return Ok(None);
    };
    if bytes.is_empty() {
        return Ok(None);
    }
    let path = if bytes.starts_with(b"\x89PNG") {
        png
    } else {
        jpg
    };
    fs::write(&path, bytes)?;
    Ok(Some(path))
}

fn remove_cached_thumbnails(cache_dir: &Path, file_id: i64) -> Result<(), Box<dyn Error>> {
    for extension in ["jpg", "png"] {
        let path = cache_dir.join(format!("{file_id}.{extension}"));
        if let Err(error) = fs::remove_file(&path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                return Err(
                    format!("Cannot remove cached thumbnail {}: {error}", path.display()).into(),
                );
            }
        }
    }
    Ok(())
}

struct RomIdentity {
    size: u64,
    checksum: RomChecksum,
}

/// Identity ScreenScraper can match. Zip CRCs and sizes come from the largest
/// file entry, which is the ROM in a No-Intro archive. Hashing the zip itself
/// misses that record and the miss is then stored for good.
fn rom_identity(path: &Path, on_disk_size: u64) -> Result<RomIdentity, Box<dyn Error>> {
    let is_zip = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"));
    if is_zip {
        return zip_rom_identity(path);
    }
    Ok(RomIdentity {
        size: on_disk_size,
        checksum: RomChecksum::Sha1(sha1_file(path)?),
    })
}

fn zip_rom_identity(path: &Path) -> Result<RomIdentity, Box<dyn Error>> {
    let file = File::open(path)
        .map_err(|error| format!("Cannot read {} for enrichment: {error}", path.display()))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| format!("Cannot read zip {} for enrichment: {error}", path.display()))?;
    let mut best_index = None;
    let mut best_size = 0u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|error| {
            format!("Cannot read zip {} for enrichment: {error}", path.display())
        })?;
        if !entry.is_file() {
            continue;
        }
        if best_index.is_none() || entry.size() > best_size {
            best_size = entry.size();
            best_index = Some(index);
        }
    }
    let Some(index) = best_index else {
        return Err(format!("Zip {} has no ROM to identify", path.display()).into());
    };
    let entry = archive
        .by_index(index)
        .map_err(|error| format!("Cannot read zip {} for enrichment: {error}", path.display()))?;
    Ok(RomIdentity {
        size: entry.size(),
        checksum: RomChecksum::Crc32(format!("{:08X}", entry.crc32())),
    })
}

fn sha1_file(path: &Path) -> Result<String, Box<dyn Error>> {
    let mut file = File::open(path)
        .map_err(|error| format!("Cannot read {} for enrichment: {error}", path.display()))?;
    let mut hasher = Sha1::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Cannot read {} for enrichment: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = hasher.finalize();
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

impl std::fmt::Display for EnrichStats {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} enriched, {} missing, {} failed",
            self.enriched, self.missing, self.failed
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::discover_library;
    use std::fs::File;
    use std::io::Write;

    struct FakeProvider {
        lookup: Lookup,
    }

    impl MetadataProvider for FakeProvider {
        fn supports(&self, platform: &str) -> bool {
            platform == "snes"
        }

        fn lookup(&self, _platform: &str, rom: &RomQuery) -> Result<Lookup, Box<dyn Error>> {
            if rom.name == "fail.sfc" {
                return Err("provider unavailable".into());
            }
            let checksum_ok = match rom.checksum {
                Some(RomChecksum::Sha1(sha1)) => sha1.len() == 40,
                Some(RomChecksum::Crc32(crc)) => crc.len() == 8,
                None => false,
            };
            if rom.size == 0 || !checksum_ok {
                return Err("lookup is missing the file size or checksum".into());
            }
            Ok(self.lookup.clone())
        }
    }

    fn library_with_game(name: &str) -> (tempfile::TempDir, tempfile::TempDir, LibraryDb, i64) {
        let db_dir = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        File::create(root.path().join(name))
            .unwrap()
            .write_all(b"rom")
            .unwrap();
        let mut db = LibraryDb::open(&db_dir.path().join("library.sqlite")).unwrap();
        let library = db.create_library("snes", "SNES").unwrap();
        db.add_root(library, root.path()).unwrap();
        discover_library(&mut db, library).unwrap();
        (db_dir, root, db, library)
    }

    #[test]
    fn enrich_stores_text_and_one_thumbnail() {
        let (_db_dir, _root, mut db, library) = library_with_game("game.sfc");
        let cache = tempfile::tempdir().unwrap();
        let provider = FakeProvider {
            lookup: Lookup::Found(GameMetadata {
                provider: "fake".into(),
                provider_id: "42".into(),
                title: "Super Metroid".into(),
                year: Some("1994".into()),
                publisher: Some("Nintendo".into()),
                thumbnail: Some(b"\x89PNG\r\n\x1a\nthumb".to_vec()),
                playable: None,
                link: None,
                condition: None,
            }),
        };
        let stats = enrich_library(&mut db, library, cache.path(), &provider).unwrap();
        assert_eq!(stats.enriched, 1);
        let file_id = db.files_for_root(db.roots(library).unwrap()[0].0).unwrap()[0].id;
        let game = db.game_for_file(file_id).unwrap().unwrap();
        assert_eq!(game.title, "Super Metroid");
        assert_eq!(game.year.as_deref(), Some("1994"));
        assert_eq!(game.publisher.as_deref(), Some("Nintendo"));
        let thumb = game.thumb_path.unwrap();
        assert!(thumb.ends_with(".png"));
        assert!(Path::new(&thumb).is_file());
        assert!(!db.files_for_root(db.roots(library).unwrap()[0].0).unwrap()[0].needs_enrich);

        let again = enrich_library(&mut db, library, cache.path(), &provider).unwrap();
        assert_eq!(again.enriched, 0);
    }

    #[test]
    fn missing_match_is_not_retried() {
        let (_db_dir, _root, mut db, library) = library_with_game("game.sfc");
        let cache = tempfile::tempdir().unwrap();
        let provider = FakeProvider {
            lookup: Lookup::Missing,
        };
        let stats = enrich_library(&mut db, library, cache.path(), &provider).unwrap();
        assert_eq!(stats.missing, 1);
        let row = &db.files_for_root(db.roots(library).unwrap()[0].0).unwrap()[0];
        assert!(!row.needs_enrich);
        assert!(db.game_for_file(row.id).unwrap().is_none());
    }

    #[test]
    fn missing_match_drops_a_stale_thumbnail() {
        let (_db_dir, _root, mut db, library) = library_with_game("game.sfc");
        let cache = tempfile::tempdir().unwrap();
        let found = FakeProvider {
            lookup: Lookup::Found(GameMetadata {
                provider: "fake".into(),
                provider_id: "42".into(),
                title: "Super Metroid".into(),
                year: None,
                publisher: None,
                thumbnail: Some(b"\x89PNGstale".to_vec()),
                playable: None,
                link: None,
                condition: None,
            }),
        };
        enrich_library(&mut db, library, cache.path(), &found).unwrap();
        let root_id = db.roots(library).unwrap()[0].0;
        let file_id = db.files_for_root(root_id).unwrap()[0].id;
        db.connection_mut()
            .execute("UPDATE files SET needs_enrich = 1 WHERE id = ?1", [file_id])
            .unwrap();
        let jpg = cache.path().join(format!("{file_id}.jpg"));
        fs::write(&jpg, b"stale-jpg").unwrap();
        let missing = FakeProvider {
            lookup: Lookup::Missing,
        };
        let stats = enrich_library(&mut db, library, cache.path(), &missing).unwrap();
        assert_eq!(stats.missing, 1);
        assert!(!jpg.exists());
        assert!(!cache.path().join(format!("{file_id}.png")).exists());
        assert!(db.game_for_file(file_id).unwrap().is_none());
    }

    #[test]
    fn provider_error_leaves_the_file_queued() {
        let (_db_dir, _root, mut db, library) = library_with_game("fail.sfc");
        let cache = tempfile::tempdir().unwrap();
        let provider = FakeProvider {
            lookup: Lookup::Missing,
        };
        let error = enrich_library(&mut db, library, cache.path(), &provider)
            .unwrap_err()
            .to_string();
        assert!(error.contains("fail.sfc"), "{error}");
        assert!(db.files_for_root(db.roots(library).unwrap()[0].0).unwrap()[0].needs_enrich);
    }

    #[test]
    fn cancel_leaves_pending_files_queued() {
        let (_db_dir, root, mut db, library) = library_with_game("game.sfc");
        File::create(root.path().join("second.sfc"))
            .unwrap()
            .write_all(b"rom")
            .unwrap();
        discover_library(&mut db, library).unwrap();
        let cancel = crate::CancelFlag::new();
        cancel.cancel();
        let cache = tempfile::tempdir().unwrap();
        let provider = FakeProvider {
            lookup: Lookup::Missing,
        };
        let stats = enrich_library_reporting(
            &mut db,
            library,
            cache.path(),
            &provider,
            Some(&cancel),
            &mut |_, _, _| {},
        )
        .unwrap();
        assert!(stats.cancelled);
        assert_eq!(stats.missing, 0);
        let root_id = db.roots(library).unwrap()[0].0;
        assert!(db
            .files_for_root(root_id)
            .unwrap()
            .iter()
            .all(|row| row.needs_enrich));
    }

    #[test]
    fn zip_lookup_sends_the_inner_rom_crc_and_uncompressed_size() {
        let db_dir = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let payload = b"inner-rom-bytes";
        let zip_path = root.path().join("game.zip");
        write_zip(
            &zip_path,
            &[("readme.txt", b"hi".as_slice()), ("game.sfc", payload)],
        );
        let mut db = LibraryDb::open(&db_dir.path().join("library.sqlite")).unwrap();
        let library = db.create_library("snes", "SNES").unwrap();
        db.add_root(library, root.path()).unwrap();
        discover_library(&mut db, library).unwrap();

        let seen = std::sync::Mutex::new(None);
        let cache = tempfile::tempdir().unwrap();
        enrich_library(&mut db, library, cache.path(), &Capture(&seen)).unwrap();
        let (size, checksum) = seen.lock().unwrap().clone().unwrap();
        let file = File::open(&zip_path).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        let rom = archive.by_name("game.sfc").unwrap();
        assert_eq!(size, payload.len() as u64);
        assert_eq!(checksum, RomChecksum::Crc32(format!("{:08X}", rom.crc32())));
        assert_ne!(size, fs::metadata(&zip_path).unwrap().len());
    }

    struct Capture<'a>(&'a std::sync::Mutex<Option<(u64, RomChecksum)>>);

    impl MetadataProvider for Capture<'_> {
        fn supports(&self, platform: &str) -> bool {
            platform == "snes"
        }

        fn lookup(&self, _platform: &str, rom: &RomQuery) -> Result<Lookup, Box<dyn Error>> {
            let Some(checksum) = rom.checksum else {
                return Err("lookup is missing the file checksum".into());
            };
            *self.0.lock().unwrap() = Some((rom.size, checksum.clone()));
            Ok(Lookup::Missing)
        }
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        for (name, bytes) in entries {
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }
}
