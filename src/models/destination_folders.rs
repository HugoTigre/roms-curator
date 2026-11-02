use crate::core::args::Args;
use std::fs;
use std::path::{Path, PathBuf};

pub struct DestinationFolders {
    pub working: PathBuf,
    pub other: PathBuf,
    pub chd_working: PathBuf,
    pub chd_other: PathBuf,
}

impl Args {
    pub fn build_destination_folders_path(&self) -> DestinationFolders {
        let destination_dir = Path::new(&self.destination_path);
        let working = destination_dir.join("working");
        let other = destination_dir.join("other");
        let chd_working = destination_dir.join("chd_working");
        let chd_other = destination_dir.join("chd_other");

        fs::create_dir_all(&working).expect("Error creating working directory");
        fs::create_dir_all(&other).expect("Error creating other directory");
        fs::create_dir_all(&chd_working).expect("Error creating chd_working directory");
        fs::create_dir_all(&chd_other).expect("Error creating chd_other directory");

        DestinationFolders {
            working,
            other,
            chd_working,
            chd_other,
        }
    }
}
