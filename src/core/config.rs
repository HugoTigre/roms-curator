//! Library configuration for metadata loading and collection export.
//!
//! Build [`ExportConfig`] in application code or load it with [`super::args::build_args`]
//! from the CLI. Call [`ExportConfig::validate`] before planning or copying.

use std::error::Error;
use std::path::{Path, PathBuf};

/// Paths and flags for one export run.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct ExportConfig {
    /// Path to MAME ROM database file. See README on how to get this.
    pub mame_xml_path: String,
    /// Path to MAME support file. See README on how to get this.
    pub catver_path: String,
    /// Where the original ROM collection is. Can be more than one directory.
    pub source_path: Vec<String>,
    /// Export root: creates `roms`, `rest`, and `selected.txt` beneath it.
    pub destination_path: String,
    /// Path to the generated report in markdown format. Ex: report.md.
    pub report_path: String,
    /// Path to the JSON copy plan. Empty skips the file.
    /// A new path is written before copying. An existing file is checked and not replaced.
    pub plan_path: String,
    /// Accepted for compatibility. Required disks are kept and unused disk directories are omitted either way.
    pub ignore_not_working_chd: bool,
    /// Plan operations without creating output directories or copying ROMs.
    /// An optional report must be outside the source/output trees.
    pub simulation: bool,
    /// If set, skip names that sort before this. Required archives and disk directories of selected games inside the range are still copied into `roms`.
    pub subset_start: String,
    /// If set, skip names that sort after this. Required archives and disk directories of selected games inside the range are still copied into `roms`.
    pub subset_end: String,
    /// When true, select only machines whose driver and emulation are both good.
    pub strict_emulation: bool,
    /// If true, show progress bar.
    pub progress: bool,
}

/// Output folders under [`ExportConfig::destination_path`].
pub struct DestinationFolders {
    /// Files a selected game needs: merged archives, BIOS, devices, and CHD directories.
    pub roms: PathBuf,
    /// Every other source file, so the original collection can be removed.
    pub rest: PathBuf,
}

impl ExportConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// Read-only path checks shared by the CLI and library callers.
    pub fn validate(&self) -> Result<(), Box<dyn Error>> {
        crate::core::paths::validate(self)
    }

    /// Build the export paths without creating directories.
    pub fn destination_folders(&self) -> DestinationFolders {
        let destination_dir = Path::new(&self.destination_path);
        DestinationFolders {
            roms: destination_dir.join("roms"),
            rest: destination_dir.join("rest"),
        }
    }

    /// Exact selected machine names, one per line, beside the two export folders.
    pub fn selected_names_path(&self) -> PathBuf {
        Path::new(&self.destination_path).join("selected.txt")
    }
}
