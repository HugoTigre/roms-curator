//! Collection library: SQLite index of ROM folders, separate from MAME export.
//!
//! Discover records files. Each scan reports added, changed, unchanged, removed,
//! and restored files. Enrichment fills title, year, publisher, and one thumbnail
//! for files still marked `needs_enrich`. The Library screen searches those rows.

mod db;
mod discover;
mod enrich;
mod paths;
pub mod platform;
mod platforms;
mod screenscraper;

pub use db::{
    default_library_db_path, BrowseHit, FileRow, FileState, GameRecord, LibraryDb, LibraryOverview,
};
pub use discover::{discover_library, discover_library_stopping, DiscoverStats};
pub use enrich::{
    enrich_library, enrich_library_reporting, EnrichStats, GameCondition, GameMetadata, Lookup,
    MetadataProvider, RomChecksum, RomQuery,
};
pub use paths::{delete_library_storage, library_credentials_path, library_thumb_cache_dir};
pub use platform::Platform;
pub use platforms::ArcadeCatalog;
pub use screenscraper::{ScreenScraper, ScreenScraperCreds};
