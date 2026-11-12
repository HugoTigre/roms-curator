//! Collection library: SQLite index of ROM folders, separate from MAME export.
//!
//! Discover records files. Each scan reports added, changed, unchanged, removed,
//! and restored files. Enrichment and the GUI browse screen are later steps.

mod db;
mod discover;
mod paths;

pub use db::{default_library_db_path, FileRow, FileState, LibraryDb};
pub use discover::{discover_library, DiscoverStats};
