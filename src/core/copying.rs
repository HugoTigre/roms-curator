//! File-level safety. Preflight assumes stable trees, as does path validation.

use std::collections::HashMap;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use tempfile::{Builder, TempPath};

use super::paths::{resolve, visit};

// MAME names are ASCII. Reject case-only clashes even on case-sensitive hosts
// so a collection prepared there can also be copied to Windows.
fn key(path: &Path) -> PathBuf {
    PathBuf::from(path.as_os_str().to_ascii_lowercase())
}

pub(crate) fn check_targets(
    copies: &[(PathBuf, PathBuf)],
    directories: &[&Path],
    report_path: &str,
) -> Result<(), Box<dyn Error>> {
    let conflicts = destination_conflicts(copies, directories, report_path)?;
    if let Some(conflict) = conflicts.first() {
        return Err(conflict.clone().into());
    }
    Ok(())
}

pub(crate) fn destination_conflicts(
    copies: &[(PathBuf, PathBuf)],
    directories: &[&Path],
    report_path: &str,
) -> Result<Vec<String>, Box<dyn Error>> {
    let mut conflicts = Vec::new();
    let report = if report_path.is_empty() {
        None
    } else {
        Some(key(&resolve(Path::new(report_path))?))
    };
    let mut resolved_directories = HashMap::new();
    for directory in directories {
        let resolved = resolve(directory)?;
        match fs::metadata(directory) {
            Ok(metadata) if !metadata.is_dir() => {
                return Err(format!(
                    "Output directory is blocked by a file: {}",
                    directory.display()
                )
                .into());
            }
            Ok(_) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
        if !resolved.exists() {
            if let Some(parent) = resolved.parent() {
                match fs::read_dir(parent) {
                    Ok(entries) => {
                        for entry in entries {
                            let entry = entry?;
                            if Some(entry.file_name().to_ascii_lowercase())
                                == resolved.file_name().map(|name| name.to_ascii_lowercase())
                            {
                                return Err(format!(
                                    "Destination directory has a case-only conflict: {} and {}",
                                    resolved.display(),
                                    entry.path().display()
                                )
                                .into());
                            }
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                    Err(error) => return Err(error.into()),
                }
            }
        }
        if let Some(report) = &report {
            let directory_key = key(&resolved);
            if directory_key.starts_with(report) || report.starts_with(&directory_key) {
                return Err(format!(
                    "Report path conflicts with output directory: {}",
                    directory.display()
                )
                .into());
            }
        }
        resolved_directories.insert(directory.to_path_buf(), resolved);
    }

    let mut targets = HashMap::new();
    // Read each existing category directory once, rather than once per ROM.
    let mut existing: HashMap<PathBuf, HashMap<OsString, PathBuf>> = HashMap::new();
    for (source, destination) in copies {
        // Resolve each category once, rather than canonicalizing a missing path
        // and its ancestors for every archive in a complete collection.
        let parent = resolved_directories
            .get(destination.parent().ok_or("Missing destination parent")?)
            .ok_or("Destination is outside the category directories")?;
        let destination = parent.join(destination.file_name().ok_or("Missing destination name")?);
        let parent = parent.as_path();
        if !existing.contains_key(parent) {
            let mut names = HashMap::new();
            match fs::read_dir(parent) {
                Ok(entries) => {
                    for entry in entries {
                        let entry = entry?;
                        names.insert(entry.file_name().to_ascii_lowercase(), entry.path());
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
            }
            existing.insert(parent.to_path_buf(), names);
        }
        if let Some(path) = existing[parent].get(
            &destination
                .file_name()
                .ok_or("Missing destination name")?
                .to_ascii_lowercase(),
        ) {
            conflicts.push(format!(
                "Destination already exists; refusing replacement: {} (source: {})",
                path.display(),
                source.display()
            ));
            continue;
        }

        visit(source, &mut |path| {
            let target = destination.join(path.strip_prefix(source)?);
            let target_key = key(&target);
            if let Some(previous) = targets.insert(target_key.clone(), path.to_path_buf()) {
                conflicts.push(format!(
                    "Conflicting destination {} from sources {} and {}",
                    target.display(),
                    previous.display(),
                    path.display()
                ));
            }
            // Reports are already excluded from every category directory, so
            // no per-entry report comparison or metadata lookup is needed.
            Ok(())
        })?;
    }
    Ok(conflicts)
}

/// Copy through a new staging path on the destination filesystem, then publish
/// without replacing anything. Normal failures remove staging files. Abrupt
/// termination may leave staging files; this is not a power-loss durability guarantee.
pub(crate) fn copy_file(source: &Path, destination: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Source must be a regular file",
        ));
    }
    stage_and_publish(destination, |temporary| {
        // A nonexistent target allows APFS cloning. fs::copy also preserves
        // source permissions and uses the platform's optimized copy operation.
        fs::copy(source, temporary).map(|_| ())
    })?;
    #[cfg(windows)]
    if metadata.permissions().readonly() {
        // tempfile clears file attributes before moving on Windows. Restore
        // read-only only after publication; writable sources need no extra call.
        fs::set_permissions(destination, metadata.permissions())?;
    }
    Ok(())
}

fn stage_and_publish(
    destination: &Path,
    copy: impl FnOnce(&Path) -> io::Result<()>,
) -> io::Result<()> {
    match fs::symlink_metadata(destination) {
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("Destination already exists: {}", destination.display()),
            ))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        Err(error) => return Err(error),
    }
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    // A private sibling directory reserves the staging location while leaving
    // the file itself nonexistent. It also owns cleanup after a partial copy.
    let staging = Builder::new().prefix(".roms-curator-").tempdir_in(parent)?;
    let temporary = TempPath::try_from_path(staging.path().join("rom"))?;
    copy(&temporary)?;
    // No per-file fsync: completing the copy before publication prevents a
    // partial final file, without forcing every ROM to stable storage in turn.
    // Unlike rename(), this never replaces an entry created after preflight.
    temporary
        .persist_noclobber(destination)
        .map_err(|error| error.error)?;
    Ok(())
}

pub(crate) fn copy_directory(source: &Path, destination: &Path) -> io::Result<()> {
    if !fs::symlink_metadata(source)?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Source must be a directory",
        ));
    }
    // Reject a pre-existing directory as well: no implicit CHD directory merge.
    fs::create_dir(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        let result = if entry.file_type()?.is_dir() {
            copy_directory(&entry.path(), &target)
        } else {
            copy_file(&entry.path(), &target)
        };
        result.map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "{} -> {}: {error}",
                    entry.path().display(),
                    target.display()
                ),
            )
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn preserves_windows_readonly_and_writable_source_permissions() {
        for readonly in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let source = directory.path().join("source.zip");
            let destination = directory.path().join("output.zip");
            fs::write(&source, b"source ROM").unwrap();
            let mut permissions = fs::metadata(&source).unwrap().permissions();
            permissions.set_readonly(readonly);
            fs::set_permissions(&source, permissions).unwrap();

            let result = copy_file(&source, &destination);
            let source_readonly = fs::metadata(&source).map(|m| m.permissions().readonly());
            let output_readonly = fs::metadata(&destination).map(|m| m.permissions().readonly());

            // Clear attributes before assertions so TempDir can clean up on
            // Windows even if the copy or a permission assertion fails.
            for path in [&source, &destination] {
                if let Ok(metadata) = fs::metadata(path) {
                    let mut permissions = metadata.permissions();
                    permissions.set_readonly(false);
                    fs::set_permissions(path, permissions).unwrap();
                }
            }
            result.unwrap();
            assert_eq!(source_readonly.unwrap(), readonly);
            assert_eq!(output_readonly.unwrap(), readonly);
            assert_eq!(fs::read(&source).unwrap(), b"source ROM");
            assert_eq!(fs::read(&destination).unwrap(), b"source ROM");
            assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
        }
    }

    #[test]
    fn partial_read_failure_leaves_no_final_or_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let result = stage_and_publish(&directory.path().join("rom.zip"), |temporary| {
            assert!(!temporary.exists());
            fs::write(temporary, b"partial")?;
            Err(io::Error::other("injected read failure"))
        });
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("injected read failure"));
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[test]
    fn destination_created_during_copy_is_preserved_and_temporary_file_removed() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("rom.zip");
        let result = stage_and_publish(&destination, |temporary| {
            fs::write(temporary, b"new")?;
            fs::write(&destination, b"keep existing")
        });
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&destination).unwrap(), b"keep existing");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn failed_publication_to_a_directory_cleans_up_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("rom.zip");
        let result = stage_and_publish(&destination, |temporary| {
            fs::write(temporary, b"new")?;
            fs::create_dir(&destination)
        });
        assert!(result.is_err());
        assert!(destination.is_dir());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}
