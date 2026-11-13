//! Collection library: SQLite index of ROM folders, separate from MAME export.
//!
//! Discover records files. Each scan reports added, changed, unchanged, removed,
//! and restored files. Enrichment fills title, year, publisher, and one thumbnail
//! for files still marked `needs_enrich`. The GUI browse screen is a later step.

mod db;
mod discover;
mod enrich;
mod paths;
mod screenscraper;

pub use db::{default_library_db_path, FileRow, FileState, GameRecord, LibraryDb};
pub use discover::{discover_library, DiscoverStats};
pub use enrich::{
    enrich_library, EnrichStats, GameMetadata, Lookup, MetadataProvider, RomChecksum, RomQuery,
};
pub use screenscraper::{ScreenScraper, ScreenScraperCreds};
