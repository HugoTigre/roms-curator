use crate::core::args::Args;
use std::path::{Path, PathBuf};

pub struct DestinationFolders {
    pub working: PathBuf,
    pub other: PathBuf,
    pub chd_working: PathBuf,
    pub chd_other: PathBuf,
}

impl Args {
    /// Build the category paths without creating directories.
    pub fn build_destination_folders_path(&self) -> DestinationFolders {
        let destination_dir = Path::new(&self.destination_path);
        let working = destination_dir.join("working");
        let other = destination_dir.join("other");
        let chd_working = destination_dir.join("chd_working");
        let chd_other = destination_dir.join("chd_other");

        DestinationFolders {
            working,
            other,
            chd_working,
            chd_other,
        }
    }
}
