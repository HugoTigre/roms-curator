//! A reusable copy plan.
//!
//! Version 2 is JSON. Field order is the struct order below, and every list
//! the builder sorts is sorted again on read only by the builder. Two builds
//! from the same catalog, arguments, and file bytes produce the same document.
//! [`Plan::recheck`] compares those file bytes, source sizes, and destination
//! conflicts with the disk before a later copy.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use tempfile::Builder;

use crate::core::args::Args;
use crate::core::copying::destination_conflicts;
use crate::core::paths::visit;

/// Schema version written by this crate. [`Plan::read`] rejects any other value.
pub const PLAN_VERSION: u32 = 2;

/// Sorted plan for one catalog and one set of arguments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub version: u32,
    pub metadata: MetadataIdentity,
    /// Source directories, in the order they were given.
    pub source_paths: Vec<String>,
    pub destination_path: String,
    /// `selected.txt` beside `roms` and `rest`. One exact selected machine name per line.
    pub selected_names_path: String,
    /// Empty when no report was requested. Conflict checks use this path.
    pub report_path: String,
    pub subset_start: String,
    pub subset_end: String,
    /// Selected games inside the name range, sorted by machine.
    pub games: Vec<PlannedGame>,
    /// Archives and disk directories the copy would use, sorted by machine and role.
    pub media: Vec<PlannedMedia>,
    /// Dependency and disk references that do not resolve, plus cycles.
    pub unresolved: Vec<UnresolvedMedia>,
    /// Destination clashes. Empty when the copy can start.
    /// Both the build and recheck walk present media after `media` is sorted by
    /// machine, role, and path, so the source order inside a clash stays stable.
    pub conflicts: Vec<String>,
}

/// MAME and CatVer files this plan was built from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataIdentity {
    pub mame_build: String,
    /// `playable` or `strict`.
    pub emulation_policy: String,
    pub mame_xml_path: String,
    /// SHA-1 of the XML file. Empty when the path is empty.
    pub mame_xml_sha1: String,
    pub catver_path: String,
    /// SHA-1 of the CatVer file. Empty when the path is empty.
    pub catver_sha1: String,
}

/// One selected game that this run would keep.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedGame {
    pub machine: String,
    /// CatVer category. `Unknown` when the file has no entry.
    pub category: String,
    /// Copy-folder category, such as `Working`.
    pub folder: String,
    /// Why the emulation policy selected this machine.
    pub reason: String,
}

/// One archive or disk directory.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedMedia {
    pub machine: String,
    pub role: MediaRole,
    pub availability: Availability,
    /// Absolute or given source path. Absent when the file is not in the source.
    pub source: Option<String>,
    /// Destination path. Absent when there is no source file name to join.
    pub destination: Option<String>,
    /// Bytes that would be copied. Absent when the source is missing.
    pub bytes: Option<u64>,
    /// Why this file is in the plan, sorted.
    pub reasons: Vec<String>,
}

/// A reference the catalog could not resolve to a file.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnresolvedMedia {
    /// Selected game, or empty for a cycle.
    pub game: String,
    pub kind: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaRole {
    Archive,
    Disk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Present,
    Missing,
}

impl Plan {
    /// Write pretty JSON without replacing an existing file.
    pub fn write(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut bytes = serde_json::to_vec_pretty(self)?;
        bytes.push(b'\n');
        publish(path, &bytes)
    }

    /// Read a version-2 plan.
    pub fn read(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let file = File::open(path)
            .map_err(|error| format!("Cannot read plan {}: {error}", path.display()))?;
        let plan: Plan = serde_json::from_reader(file)
            .map_err(|error| format!("Cannot parse plan {}: {error}", path.display()))?;
        if plan.version != PLAN_VERSION {
            return Err(format!(
                "Unsupported plan version {} in {}",
                plan.version,
                path.display()
            )
            .into());
        }
        Ok(plan)
    }

    /// Compare recorded metadata, source sizes, and destination conflicts with the disk.
    ///
    /// Returns an error when a hashed metadata file changed, a present file
    /// changed size or disappeared, a missing file appeared, or the conflict
    /// list is different. This does not copy.
    pub fn recheck(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut problems = Vec::new();
        recheck_hash(
            &mut problems,
            "MAME XML",
            &self.metadata.mame_xml_path,
            &self.metadata.mame_xml_sha1,
        )?;
        recheck_hash(
            &mut problems,
            "CatVer",
            &self.metadata.catver_path,
            &self.metadata.catver_sha1,
        )?;

        let sources = source_index(&self.source_paths)?;
        for item in &self.media {
            match item.availability {
                Availability::Present => {
                    let Some(source) = item.source.as_deref() else {
                        problems.push(format!(
                            "Present {} {} has no source path",
                            item.role_name(),
                            item.machine
                        ));
                        continue;
                    };
                    let path = Path::new(source);
                    if !path.exists() {
                        problems.push(format!("Source disappeared: {source}"));
                        continue;
                    }
                    match content_bytes(path) {
                        Ok(bytes) if Some(bytes) == item.bytes => {}
                        Ok(bytes) => problems.push(format!(
                            "Size changed for {source}: plan has {} bytes, disk has {bytes}",
                            item.bytes.unwrap_or(0)
                        )),
                        Err(error) => problems.push(format!("Cannot read {source}: {error}")),
                    }
                }
                Availability::Missing => {
                    if let Some(path) = sources.get(&(item.machine.clone(), item.role)) {
                        problems.push(format!(
                            "Missing {} {} is now at {}",
                            item.role_name(),
                            item.machine,
                            path.display()
                        ));
                    }
                }
            }
        }

        let copies = present_copies(&self.media);
        let folders = Args {
            destination_path: self.destination_path.clone(),
            ..Args::default()
        }
        .build_destination_folders_path();
        let directories = [folders.roms.as_path(), folders.rest.as_path()];
        let names = Path::new(&self.selected_names_path);
        let conflicts = destination_conflicts(&copies, &directories, &self.report_path, names)?;
        if conflicts != self.conflicts {
            problems.push(format!(
                "Destination conflicts changed: {}",
                conflicts.join(" | ")
            ));
        }

        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems.join("; ").into())
        }
    }
}

pub(crate) fn present_copies(media: &[PlannedMedia]) -> Vec<(PathBuf, PathBuf)> {
    media
        .iter()
        .filter(|item| item.availability == Availability::Present)
        .filter_map(|item| {
            let source = item.source.as_ref()?;
            let destination = item.destination.as_ref()?;
            Some((PathBuf::from(source), PathBuf::from(destination)))
        })
        .collect()
}

impl PlannedMedia {
    fn role_name(&self) -> &'static str {
        match self.role {
            MediaRole::Archive => "archive",
            MediaRole::Disk => "disk",
        }
    }
}

pub(crate) fn content_bytes(path: &Path) -> Result<u64, Box<dyn std::error::Error>> {
    let mut total = 0u64;
    visit(path, &mut |entry| {
        let metadata = fs::symlink_metadata(entry)?;
        if metadata.is_file() {
            total += metadata.len();
        }
        Ok(())
    })?;
    Ok(total)
}

pub(crate) fn file_sha1(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    if path.is_empty() {
        return Ok(String::new());
    }
    let mut file = File::open(path).map_err(|error| format!("Cannot read {path}: {error}"))?;
    let mut hasher = Sha1::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Cannot read {path}: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = hasher.finalize();
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn recheck_hash(
    problems: &mut Vec<String>,
    label: &str,
    path: &str,
    recorded: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if path.is_empty() {
        if !recorded.is_empty() {
            problems.push(format!("{label} hash was recorded without a path"));
        }
        return Ok(());
    }
    let sha = file_sha1(path)?;
    if sha != recorded {
        problems.push(format!("{label} changed: {path}"));
    }
    Ok(())
}

fn source_index(
    roots: &[String],
) -> Result<HashMap<(String, MediaRole), PathBuf>, Box<dyn std::error::Error>> {
    let mut index = HashMap::new();
    for root in roots {
        let entries = fs::read_dir(root)
            .map_err(|error| format!("Cannot read source directory {root}: {error}"))?;
        for entry in entries {
            let path = entry
                .map_err(|error| format!("Cannot read entry in source directory {root}: {error}"))?
                .path();
            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let stem = stem.to_ascii_lowercase();
            if fills(MediaRole::Disk, &path) {
                index
                    .entry((stem.clone(), MediaRole::Disk))
                    .or_insert_with(|| path.clone());
            }
            if fills(MediaRole::Archive, &path) {
                index
                    .entry((stem, MediaRole::Archive))
                    .or_insert_with(|| path.clone());
            }
        }
    }
    Ok(index)
}

fn fills(role: MediaRole, path: &Path) -> bool {
    let chd_file = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("chd"));
    match role {
        MediaRole::Disk => path.is_dir() || chd_file,
        MediaRole::Archive => !path.is_dir() && !chd_file,
    }
}

fn publish(path: &str, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    match fs::symlink_metadata(path) {
        Ok(_) => return Err(format!("Cannot create plan {path}: file already exists").into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(format!("Cannot create plan {path}: {error}").into()),
    }
    let parent = Path::new(path)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = Builder::new()
        .prefix(".roms-curator-plan-")
        .tempfile_in(parent)
        .map_err(|error| format!("Cannot create plan {path}: {error}"))?;
    temporary
        .write_all(bytes)
        .map_err(|error| format!("Cannot write plan {path}: {error}"))?;
    temporary
        .persist_noclobber(path)
        .map_err(|error| format!("Cannot publish plan {path}: {}", error.error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_rejects_a_version_1_plan() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("plan.json");
        let document = r#"{
            "version": 1,
            "metadata": {
                "mame_build": "0",
                "emulation_policy": "playable",
                "mame_xml_path": "",
                "mame_xml_sha1": "",
                "catver_path": "",
                "catver_sha1": ""
            },
            "source_paths": [],
            "destination_path": "destination",
            "selected_names_path": "destination/selected.txt",
            "report_path": "",
            "subset_start": "",
            "subset_end": "",
            "games": [],
            "media": [],
            "unresolved": [],
            "conflicts": []
        }"#;
        fs::write(&path, document).unwrap();
        let error = Plan::read(&path).unwrap_err().to_string();
        assert!(error.contains("Unsupported plan version 1"), "{error}");
    }
}
