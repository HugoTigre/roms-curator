use std::error::Error;
use std::fs::read_dir;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use log::error;
use roxmltree::{Document, Node};

use crate::core::args::Args;
use crate::models::destination_folders::DestinationFolders;
use crate::models::report::{Report, ReportDetailEntry};
use crate::models::roms::{
    Chd, ChdStatus, Feature, FeatureStatus, Rom, RomCategory, RomData, RomDataExt, RomStatus, Roms,
    Status, UnfilteredRoms, EXCLUDED_CATEGORIES, SPECIAL_CASES_DEMOTE, SPECIAL_CASES_PROMOTE,
};
use crate::utils::{build_progress_bar, copy_dir_recursive, ProgressBarEx};
use crate::RomCategories;

pub fn parse(doc: Document, categories: RomCategories) -> Result<UnfilteredRoms, Box<dyn Error>> {
    let mut roms = UnfilteredRoms::new();

    // go through mame xml doc to build categorized collection with all roms
    for node in doc.descendants() {
        if node.is_element() && node.tag_name().name() == "machine" {
            // found rom

            let name =
                match node.attribute("name") {
                    Some(value) => value.to_string(),
                    None => return Err(
                        "Machine with no name!!! Probably something wrong with file, aborting..."
                            .into(),
                    ),
                };

            let is_bios = is_bios(node);
            let is_system = is_system(node, &name, &categories);

            let clone_of = node.attribute("cloneof").map(|value| value.to_string());

            let is_mechanical = match node.attribute("ismechanical") {
                Some(value) => value == "yes",
                _ => false,
            };

            let rom_of = node.attribute("romof").map(|value| value.to_string());

            let status = extract_status(node);

            let features = extract_features(node);

            let chd = extract_chd(node);

            let category = match categories.get(&name) {
                Some(v) => v.to_string(),
                _ => "".to_string(),
            };

            roms.insert(
                name,
                RomData {
                    status,
                    is_bios,
                    is_system,
                    is_mechanical,
                    features,
                    clone_of,
                    rom_of,
                    chd,
                    category,
                },
            );
        }
    }

    Ok(roms)
}

pub trait UnfilteredRomsExt {
    fn categorize_roms(self) -> Result<Roms, Box<dyn Error>>;
}

impl UnfilteredRomsExt for UnfilteredRoms {
    fn categorize_roms(self) -> Result<Roms, Box<dyn Error>> {
        let mut roms = Roms::new();

        // First split roms into categories
        self.into_iter().for_each(|(name, data)| {
            let categorized_rom = if data.is_bios {
                (name, data.to_bios_rom())
            } else if data.is_system {
                (name, data.to_system_rom())
            } else {
                let status = data.status.clone().unwrap();

                if data.is_mechanical
                    || matches!(status.driver, Status::Preliminary)
                    || SPECIAL_CASES_DEMOTE.contains(&name)
                    || check_features_status(&name, &data.features)
                {
                    (name, data.to_not_working_rom())
                } else {
                    match status.emulation {
                        Status::Imperfect | Status::Good => (name, data.to_working_rom()),
                        _ => (name, data.to_not_working_rom()),
                    }
                }
            };

            roms.insert(categorized_rom.0.clone(), categorized_rom.1);
        });

        // Re-assign categories based on rom dependencies
        let to_demote = check_roms_dependency(&roms);

        to_demote.iter().for_each(|name| {
            roms.remove(name).and_then(|rom| {
                if matches!(rom.category, RomCategory::Working) {
                    roms.insert(
                        name.clone(),
                        Rom {
                            data: rom.data,
                            category: RomCategory::NotWorking,
                        },
                    )
                } else {
                    None
                }
            });
        });

        Ok(roms)
    }
}

pub trait RomsExt {
    ///
    /// Copies the selected ROMs to the destination directory in [`Args`].
    ///
    /// Conflicting destination names (including case-only differences), existing
    /// selected files or CHD directories, and report paths inside category
    /// directories abort the run before category directories are created or ROMs
    /// are copied. Files are never overwritten and CHD directories are not merged.
    /// Later copy failures retain files already completed; there is no rollback
    /// of the whole collection or guarantee of durability after power loss.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::process;
    /// # use log::error;
    /// use roms_curator::core::args::build_args;
    /// use roms_curator::core::roms_service::RomsExt;
    /// use roms_curator::utils::build_progress_bar;
    ///
    /// let args = build_args().unwrap_or_else(|err| {
    ///     error!("Application error: {err}");
    ///     process::exit(1);
    /// });
    ///
    /// let roms = roms_curator::run(&args).unwrap_or_else(|err| {
    ///     error!("Needs a proper Config");
    ///     process::exit(1);
    /// });
    ///
    /// let report = roms.copy_roms(&args).unwrap_or_else(|err| {
    ///     error!("Application error: {err}");
    ///     process::exit(1);
    /// });
    /// ```
    ///
    /// @return A [Report](Report) of all that was and/or was not copied.
    ///
    fn copy_roms(self, args: &Args) -> Result<Report, Box<dyn Error>>;
    fn check_paths(args: &Args) -> Result<bool, &'static str>;
    fn get_destination_folder(rom: &Rom, destination_folders: &DestinationFolders) -> PathBuf;
    fn should_move(rom: &Rom, args: &Args) -> bool;
    fn copy_rom(path: &Path, destination: &Path, args: &Args) -> bool;
    fn is_excluded(args: &Args, file_prefix: &str) -> bool;
}

impl RomsExt for Roms {
    fn copy_roms(self, args: &Args) -> Result<Report, Box<dyn Error>> {
        Self::check_paths(args)?;
        crate::core::paths::validate(args)?;

        let destination_paths = args.build_destination_folders_path();
        let directories = [
            destination_paths.working.as_path(),
            destination_paths.other.as_path(),
            destination_paths.chd_working.as_path(),
            destination_paths.chd_other.as_path(),
        ];
        let mut sources = Vec::new();
        let mut copies = Vec::new();
        for source in &args.source_path {
            let mut entries = read_dir(source)?
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<Result<Vec<_>, _>>()?;
            entries.sort();
            for path in &entries {
                let prefix = path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| format!("Unsupported ROM name: {}", path.display()))?;
                if Self::is_excluded(args, prefix) {
                    continue;
                }
                if let Some(rom) = self.get(&prefix.to_ascii_lowercase()) {
                    if Self::should_move(rom, args) {
                        let destination = Self::get_destination_folder(rom, &destination_paths)
                            .join(path.file_name().ok_or("Missing ROM filename")?);
                        copies.push((path.clone(), destination));
                    }
                }
            }
            sources.push((source, entries));
        }
        crate::core::copying::check_targets(&copies, &directories, &args.report_path)?;
        for directory in directories {
            std::fs::create_dir_all(directory)?;
        }

        let mut total_working = 0;
        let mut total_other = 0;
        let mut something_failed = false;
        let mut report = Report::new();

        for (source_path, entries) in sources {
            let progress_bar = if args.progress {
                Some(build_progress_bar())
            } else {
                None
            };

            progress_bar.println(format!("Copying from source: {}", source_path).as_str());

            let nr_of_files = entries.len();
            if let Some(pb) = progress_bar.as_ref() {
                pb.set_length(nr_of_files as u64);
            }

            for path in entries {
                let (file_prefix, file_name) = (
                    path.file_stem().unwrap().to_str().unwrap(),
                    path.file_name().unwrap(),
                );

                progress_bar.inc();

                if Self::is_excluded(args, file_prefix) {
                    continue;
                }

                if let Some(rom) = self.get(&file_prefix.to_ascii_lowercase()) {
                    if Self::should_move(rom, args) {
                        let destination =
                            Self::get_destination_folder(rom, &destination_paths).join(file_name);

                        let moved = Self::copy_rom(&path, &destination, args);
                        if !moved {
                            something_failed = true
                        };

                        let report_detail_entry = ReportDetailEntry {
                            rom_name: file_name.to_string_lossy().into_owned(),
                            moved,
                            is_chd: !rom.data.chd.is_empty(),
                        };

                        match &rom.category {
                            RomCategory::Working => {
                                total_working += 1;
                                report.add_rom_working(report_detail_entry)
                            }
                            _ => {
                                total_other += 1;
                                report.add_rom_other(report_detail_entry)
                            }
                        };
                    }
                } else {
                    let report_detail_entry = ReportDetailEntry {
                        rom_name: file_name.to_string_lossy().into_owned(),
                        moved: false,  // doesn't matter here
                        is_chd: false, // doesn't matter here
                    };
                    report.add_ignored_rom(report_detail_entry);
                }
            }

            if let Some(pb) = progress_bar {
                pb.finish();
            }
        }

        report
            .total_working(total_working)
            .total_other(total_other)
            .all_ok(!something_failed)
            .build();

        Ok(report)
    }

    fn check_paths(args: &Args) -> Result<bool, &'static str> {
        if args.source_path.is_empty() {
            return Err("Missing roms source path.");
        }
        if args.destination_path.is_empty() {
            return Err("Missing roms destination path.");
        }
        Ok(true)
    }

    fn get_destination_folder(rom: &Rom, destination_folders: &DestinationFolders) -> PathBuf {
        let is_chd = !rom.data.chd.is_empty();

        match &rom.category {
            RomCategory::Working => {
                if is_chd {
                    destination_folders.chd_working.to_owned()
                } else {
                    destination_folders.working.to_owned()
                }
            }
            _ => {
                if is_chd {
                    destination_folders.chd_other.to_owned()
                } else {
                    destination_folders.other.to_owned()
                }
            }
        }
    }

    fn should_move(rom: &Rom, args: &Args) -> bool {
        if !args.ignore_not_working_chd {
            return true;
        }

        let is_chd = !rom.data.chd.is_empty();

        if is_chd && !matches!(&rom.category, RomCategory::Working) {
            return false;
        }

        true
    }

    fn copy_rom(path: &Path, destination: &Path, args: &Args) -> bool {
        if args.simulation {
            return true;
        };

        if path.is_dir() {
            if let Some(err) = copy_dir_recursive(path, destination).err() {
                error!("Error copying {:?}: {}", path, err);
                return false;
            }
        } else if let Some(err) = crate::core::copying::copy_file(path, destination).err() {
            error!(
                "Error copying {} -> {}: {}",
                path.display(),
                destination.display(),
                err
            );
            return false;
        }

        true
    }

    fn is_excluded(args: &Args, file_prefix: &str) -> bool {
        (!args.subset_start.is_empty() && file_prefix < args.subset_start.as_str())
            || (!args.subset_end.is_empty() && file_prefix > args.subset_end.as_str())
    }
}

/// Checks the dependencies of ROMs to determine which working ROMs should be demoted.
///
/// This function iterates through all working ROMs and checks their dependencies.
/// A working ROM may be demoted if:
/// - It depends on a non-working parent ROM
/// - It has a CHD with a bad dump or no dump status
///
/// # Returns
/// A vector of ROM names that should be demoted from the working category.
fn check_roms_dependency(roms: &Roms) -> Vec<String> {
    let mut demote_working: Vec<String> = Vec::new();

    roms.iter()
        .filter(|(_, rom)| matches!(rom.category, RomCategory::Working))
        .for_each(|(name, rom)| {
            if rom.data.rom_of.is_some()
                && !(rom.data.clone_of.is_some() && rom.data.rom_of.eq(&rom.data.clone_of))
            {
                let demote = should_demote_rom(rom.data.rom_of.clone().unwrap(), roms);
                if demote {
                    demote_working.push(name.clone());
                }
            } else if !rom.data.chd.is_empty() {
                rom.data.chd.iter().for_each(|chd| match chd.status {
                    ChdStatus::BadDump => demote_working.push(name.clone()),
                    ChdStatus::NoDump => demote_working.push(name.clone()),
                    _ => (),
                })
            }
        });

    demote_working
}

fn should_demote_rom(rom_of: String, roms: &Roms) -> bool {
    let found = roms.get(rom_of.as_str());
    if let Some(rom) = found {
        match rom.category {
            RomCategory::Working => {
                if rom.data.rom_of.is_some() {
                    should_demote_rom(rom.data.rom_of.clone().unwrap(), roms);
                }
            }
            RomCategory::System | RomCategory::NotWorking => return true,
            RomCategory::Bios => match &rom.data.status {
                Some(status) => {
                    if status.driver == Status::Preliminary {
                        return true;
                    }
                    if status.emulation == Status::Preliminary {
                        return true;
                    }
                }
                None => {
                    return false;
                }
            },
            _ => (),
        };
    };

    false
}

///
/// @return true if contains bad feature status, false otherwise.
///
fn check_features_status(name: &String, features: &[Feature]) -> bool {
    if SPECIAL_CASES_PROMOTE.contains(name) {
        return false;
    }

    let mut invalid: u32 = 0;
    features.iter().for_each(|feature| {
        if feature.typ == "sound" || feature.typ == "graphics" {
            invalid += 1;
        }
    });

    invalid > 1
}

fn extract_status(node: Node) -> Option<RomStatus> {
    let mut driver_status: &str = "";
    let mut emulation_status: &str = "";

    for machine_node in node.children() {
        if machine_node.tag_name().name() == "driver" {
            if let Some(status) = machine_node.attribute("status") {
                driver_status = status
            }
            if let Some(status) = machine_node.attribute("emulation") {
                emulation_status = status
            }
        }
    }

    if driver_status.is_empty() || emulation_status.is_empty() {
        None
    } else {
        let rom_status = RomStatus {
            driver: Status::from_str(driver_status).unwrap(),
            emulation: Status::from_str(emulation_status).unwrap(),
        };
        Some(rom_status)
    }
}

fn extract_features(node: Node) -> Vec<Feature> {
    let mut feature_type: &str = "";
    let mut feature_status: &str = "";
    let mut features: Vec<Feature> = Vec::new();

    for machine_node in node.children() {
        if machine_node.tag_name().name() == "feature" {
            if let Some(typ) = machine_node.attribute("type") {
                feature_type = typ
            }
            if let Some(status) = machine_node.attribute("status") {
                feature_status = status
            } else if let Some(overall) = machine_node.attribute("overall") {
                feature_status = overall
            }
            let feature = Feature {
                typ: feature_type.to_string(),
                status: FeatureStatus::from_str(feature_status).unwrap(),
            };
            features.push(feature)
        }
    }

    features
}

fn extract_chd(node: Node) -> Vec<Chd> {
    let mut chd_status: &str = "";
    let mut chd_name: &str = "";
    let mut chd_vec: Vec<Chd> = Vec::new();

    for machine_node in node.children() {
        if machine_node.tag_name().name() == "disk" {
            if let Some(name) = machine_node.attribute("name") {
                chd_name = name
            }
            if let Some(status) = machine_node.attribute("status") {
                chd_status = status
            }
            let chd = Chd {
                name: chd_name.to_string(),
                status: ChdStatus::from_str(chd_status).unwrap_or(ChdStatus::NoStatus),
            };
            chd_vec.push(chd);
        }
    }

    chd_vec
}

fn is_system(node: Node, name: &str, categories: &RomCategories) -> bool {
    if is_device(node) {
        return true;
    };

    let mut has_device = false;
    let mut requires_chd = false;

    {
        match categories.get(name) {
            Some(v) => EXCLUDED_CATEGORIES.iter().any(|cat| v.contains(cat)),
            _ => {
                // couldn't match category, determine by having device (void if entry is chd)
                for machine_node in node.children() {
                    if machine_node.tag_name().name() == "disk" {
                        requires_chd = true;
                    }
                    if machine_node.tag_name().name() == "device" {
                        has_device = true;
                    }
                }
                has_device && !requires_chd
            }
        }
    }
}

fn is_bios(node: Node) -> bool {
    match node.attribute("isbios") {
        Some(value) => value == "yes",
        _ => false,
    }
}

fn is_device(node: Node) -> bool {
    match node.attribute("isdevice") {
        Some(value) => value == "yes",
        _ => false,
    }
}
