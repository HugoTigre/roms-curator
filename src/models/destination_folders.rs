use crate::core::args::Args;
use std::path::{Path, PathBuf};

pub struct DestinationFolders {
    /// Files a selected game needs: merged archives, BIOS, devices, and CHD directories.
    pub roms: PathBuf,
    /// Every other source file, so the original collection can be removed.
    pub rest: PathBuf,
}

impl Args {
    /// Build the export paths without creating directories.
    pub fn build_destination_folders_path(&self) -> DestinationFolders {
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
