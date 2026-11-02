//! Read-only preflight for collection paths. Filesystem changes during a copy
//! are outside this check; callers must keep the trees stable during execution.

use std::collections::{hash_map::DefaultHasher, HashSet};
use std::error::Error;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};

use same_file::Handle;

use super::args::Args;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

// Canonicalize the existing ancestor without creating a missing output suffix.
// Let the filesystem resolve links and '..' together, rather than normalizing
// lexically (which can change the meaning of a path through a link).
fn resolve(path: &Path) -> Result<PathBuf> {
    match fs::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            match fs::symlink_metadata(path) {
                Ok(_) => return Err(error.into()), // dangling link
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
            }
            let name = path
                .file_name()
                .ok_or_else(|| format!("Cannot resolve output path: {}", path.display()))?;
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            Ok(resolve(parent)?.join(name))
        }
        Err(error) => Err(error.into()),
    }
}

fn identity(path: &Path) -> Result<u64> {
    // Drop each handle immediately instead of keeping an open file per ROM.
    // A hash collision only causes a conservative rejection, never a missed alias.
    let mut hasher = DefaultHasher::new();
    Handle::from_path(path)?.hash(&mut hasher);
    Ok(hasher.finish())
}

fn contains(existing: &Path, path: &Path) -> Result<bool> {
    let handle = Handle::from_path(existing)?;
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(_) => {
                if handle == Handle::from_path(ancestor)? {
                    return Ok(true);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(false)
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Includes directory junctions and other reparse points.
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn visit(path: &Path, check: &mut impl FnMut(&Path) -> Result<()>) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if is_link(&metadata) {
        return Err(format!(
            "Linked entries inside source/output trees are not supported: {}",
            path.display()
        )
        .into());
    }
    if !metadata.is_dir() && !metadata.is_file() {
        return Err(format!("Unsupported filesystem entry: {}", path.display()).into());
    }
    check(path)?;
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            visit(&entry?.path(), check)?;
        }
    }
    Ok(())
}

pub(crate) fn validate(args: &Args) -> Result<()> {
    if args.source_path.is_empty() || args.destination_path.is_empty() {
        return Err("Source and destination paths are required.".into());
    }
    let destination = resolve(Path::new(&args.destination_path))?;
    if destination.exists() && !destination.is_dir() {
        return Err("Destination path needs to be a directory.".into());
    }
    let mut source_ids = HashSet::new();
    for source in &args.source_path {
        let source = fs::canonicalize(source)?;
        if !source.is_dir() {
            return Err(format!("Source must be a directory: {}", source.display()).into());
        }
        if contains(&source, &destination)?
            || (destination.exists() && contains(&destination, &source)?)
        {
            return Err(
                "[source-path] and [destination-path] cannot be the same or overlap.".into(),
            );
        }
        if !args.report_path.is_empty()
            && contains(&source, &resolve(Path::new(&args.report_path))?)?
        {
            return Err("Report path must be outside every source directory.".into());
        }
        visit(&source, &mut |path| {
            source_ids.insert(identity(path)?);
            Ok(())
        })?;
    }
    let mut reject_alias = |path: &Path| {
        if source_ids.contains(&identity(path)?) {
            return Err(format!("Output aliases a source entry: {}", path.display()).into());
        }
        Ok(())
    };
    if destination.exists() {
        visit(&destination, &mut reject_alias)?;
    }
    if !args.report_path.is_empty() {
        let report = resolve(Path::new(&args.report_path))?;
        if report.exists() {
            if !report.is_file() {
                return Err("Report path must be a regular file.".into());
            }
            reject_alias(&report)?;
        }
    }
    Ok(())
}
