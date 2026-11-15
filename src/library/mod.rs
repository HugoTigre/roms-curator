//! Collection library: SQLite index of ROM folders, separate from MAME export.
//!
//! Discover records files. Each scan reports added, changed, unchanged, removed,
//! and restored files. Enrichment fills title, year, publisher, and one thumbnail
//! for files still marked `needs_enrich`. The Library screen searches those rows.

mod db;
mod discover;
mod enrich;
mod igdb;
mod paths;
mod pc_names;
pub mod platform;
mod platforms;
mod prefs;
mod screenscraper;
mod trace;

pub use db::{
    default_library_db_path, BrowseHit, BrowsePage, FileRow, FileState, GameRecord, LibraryDb,
    LibraryOverview, UnmappedFile, UnmappedList,
};
pub use discover::{discover_library, discover_library_stopping, DiscoverStats};
pub use enrich::{
    allowed_game_page, enrich_library, enrich_library_reporting, store_metadata, EnrichCallbacks,
    EnrichStats, GameCondition, GameMetadata, Lookup, MetadataProvider, QuotaPause, RomChecksum,
    RomQuery,
};
pub use igdb::{Igdb, IgdbCandidate, IgdbCreds, PcSources};
pub use paths::{
    delete_library_storage, library_credentials_path, library_igdb_path, library_thumb_cache_dir,
};
pub use platform::Platform;
pub use platforms::{ArcadeCatalog, ArcadeSource};
pub use prefs::{LibraryPrefs, ScreenScraperQuota};
pub use screenscraper::{ScreenScraper, ScreenScraperCreds};
