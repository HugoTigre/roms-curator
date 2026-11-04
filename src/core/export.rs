//! Copy source entries into export folders.

use std::error::Error;
use std::io;
use std::path::Path;

use crate::core::config::ExportConfig;
use crate::utils::copy_dir_recursive;

pub(crate) fn check_export_paths(config: &ExportConfig) -> Result<(), Box<dyn Error>> {
    if config.source_path.is_empty() {
        return Err("Missing roms source path.".into());
    }
    if config.destination_path.is_empty() {
        return Err("Missing roms destination path.".into());
    }
    Ok(())
}

/// True when the machine name falls outside the configured subset range.
pub(crate) fn name_in_subset_excluded(config: &ExportConfig, file_prefix: &str) -> bool {
    (!config.subset_start.is_empty() && file_prefix < config.subset_start.as_str())
        || (!config.subset_end.is_empty() && file_prefix > config.subset_end.as_str())
}

/// Copy an archive or directory. Simulation performs no copy and returns `Ok(())`.
pub(crate) fn copy_source_entry(
    path: &Path,
    destination: &Path,
    config: &ExportConfig,
) -> io::Result<()> {
    if config.simulation {
        return Ok(());
    }

    let result = if path.is_dir() {
        copy_dir_recursive(path, destination)
    } else {
        crate::core::copying::copy_file(path, destination)
    };
    result.map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "Failed to copy {} -> {}: {error}",
                path.display(),
                destination.display()
            ),
        )
    })
}
