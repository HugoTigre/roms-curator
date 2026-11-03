use std::error::Error;
use std::fs::read_dir;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use log::error;
use roxmltree::{Document, Node};

use crate::core::args::Args;
use crate::models::destination_folders::DestinationFolders;
use crate::models::report::{OperationStatus, Report, ReportDetailEntry};
use crate::models::roms::{
    Catalog, Chd, ChdStatus, Feature, FeatureStatus, Rom, RomCategory, RomData, RomDataExt,
    RomStatus, Roms, Status, UnfilteredRoms, UNKNOWN_CATEGORY,
};
use crate::models::selection::{category_preference, machine_override, EmulationPolicy};
use crate::utils::{build_progress_bar, copy_dir_recursive, ProgressBarEx};
use crate::RomCategories;

#[derive(Debug)]
pub struct ParsedMachines {
    pub mame_build: String,
    pub machines: UnfilteredRoms,
    pub unknown_categories: Vec<String>,
    source: PathBuf,
}

impl ParsedMachines {
    /// Categorize with the playable policy: good or imperfect driver and emulation.
    pub fn categorize(self) -> Result<Catalog, Box<dyn Error>> {
        self.categorize_with(EmulationPolicy::Playable)
    }

    /// Categorize with an explicit emulation policy.
    ///
    /// Category preferences and machine-type rules still apply. Feature flags do not.
    pub fn categorize_with(self, policy: EmulationPolicy) -> Result<Catalog, Box<dyn Error>> {
        Ok(Catalog {
            mame_build: self.mame_build,
            roms: categorize_machines(self.machines, &self.source, policy)?,
            unknown_categories: self.unknown_categories,
        })
    }
}

impl Catalog {
    pub fn copy_roms(self, args: &Args) -> Result<Report, Box<dyn Error>> {
        self.roms.copy_roms(args)
    }
}

pub fn parse(
    doc: &Document,
    categories: &RomCategories,
    source: &Path,
) -> Result<ParsedMachines, Box<dyn Error>> {
    let mame_build = mame_build(doc, source)?.to_string();
    let mut machines = UnfilteredRoms::new();
    let mut unknown_categories = Vec::new();

    for node in doc.descendants() {
        if !(node.is_element() && node.tag_name().name() == "machine") {
            continue;
        }

        let name = node
            .attribute("name")
            .filter(|name| !name.is_empty())
            .ok_or_else(|| {
                format!(
                    "{}: machine is missing required attribute 'name'",
                    source.display()
                )
            })?;
        if machines.contains_key(name) {
            return Err(format!(
                "Machine '{name}' in {}: duplicate machine name",
                source.display()
            )
            .into());
        }

        let category = match categories.get(name) {
            Some(category) => category.clone(),
            None => {
                unknown_categories.push(name.to_string());
                UNKNOWN_CATEGORY.to_string()
            }
        };
        machines.insert(
            name.to_string(),
            RomData {
                status: extract_status(node, name, source)?,
                is_bios: is_bios(node),
                is_system: is_system(node, name, categories),
                is_mechanical: node.attribute("ismechanical") == Some("yes"),
                features: extract_features(node, name, source)?,
                clone_of: node.attribute("cloneof").map(str::to_string),
                rom_of: node.attribute("romof").map(str::to_string),
                chd: extract_disks(node, name, source)?,
                category,
            },
        );
    }

    unknown_categories.sort();
    Ok(ParsedMachines {
        mame_build,
        machines,
        unknown_categories,
        source: source.to_path_buf(),
    })
}

fn categorize_machines(
    machines: UnfilteredRoms,
    source: &Path,
    policy: EmulationPolicy,
) -> Result<Roms, Box<dyn Error>> {
    let mut roms = Roms::new();

    for (name, data) in machines {
        let categorized_rom = if data.is_bios {
            (name, data.to_bios_rom())
        } else if data.is_system {
            (name, data.to_system_rom())
        } else {
            let status = data.status.clone().ok_or_else(|| {
                format!(
                    "Machine '{name}' in {}: missing required driver status and emulation",
                    source.display()
                )
            })?;

            // Mechanical machines and emulation policy are separate from CatVer
            // preferences. Feature flags are ignored here.
            if data.is_mechanical || !policy.accepts(&status) {
                (name, data.to_not_working_rom())
            } else {
                (name, data.to_working_rom())
            }
        };

        roms.insert(categorized_rom.0.clone(), categorized_rom.1);
    }

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
    /// Individual copy failures are recorded in the returned report: check
    /// [`Report::all_ok`] and each failed entry's error. Preflight/setup failures
    /// return `Err` before the copy loop starts.
    /// Simulation returns `Planned` entries without creating output directories
    /// or copying ROMs. Only a separately requested report may be written.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::process;
    /// # use log::error;
    /// use roms_curator::core::args::build_args;
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
    ///
    /// if !args.report_path.is_empty() {
    ///     report.to_file(&args.report_path).unwrap_or_else(|err| {
    ///         error!("Error creating report: {err}");
    ///         process::exit(1);
    ///     });
    /// }
    /// if !report.all_ok {
    ///     error!("One or more ROMs failed to copy.");
    ///     process::exit(1);
    /// }
    /// ```
    ///
    /// @return A [Report](Report) of all that was and/or was not copied.
    ///
    fn copy_roms(self, args: &Args) -> Result<Report, Box<dyn Error>>;
    fn check_paths(args: &Args) -> Result<bool, &'static str>;
    fn get_destination_folder(rom: &Rom, destination_folders: &DestinationFolders) -> PathBuf;
    fn should_move(rom: &Rom, args: &Args) -> bool;
    /// Copy an archive or directory, returning source/destination paths and the
    /// cause on failure. Simulation performs no copy and returns `Ok(())`.
    fn copy_rom(path: &Path, destination: &Path, args: &Args) -> io::Result<()>;
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
            let mut entries = read_dir(source)
                .map_err(|error| format!("Cannot read source directory {source}: {error}"))?
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| {
                    format!("Cannot read entry in source directory {source}: {error}")
                })?;
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
        if !args.simulation {
            for directory in directories {
                std::fs::create_dir_all(directory).map_err(|error| {
                    format!(
                        "Cannot create output directory {}: {error}",
                        directory.display()
                    )
                })?;
            }
        }

        let mut total_working = 0;
        let mut total_other = 0;
        let mut something_failed = false;
        let mut report = Report {
            simulation: args.simulation,
            ..Report::default()
        };

        for (source_path, entries) in sources {
            let progress_bar = if args.progress {
                Some(build_progress_bar())
            } else {
                None
            };

            let operation = if args.simulation {
                "Planning"
            } else {
                "Copying"
            };
            progress_bar.println(format!("{operation} from source: {source_path}").as_str());

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

                        let copy_error = Self::copy_rom(&path, &destination, args)
                            .err()
                            .map(|error| error.to_string());
                        let status = if copy_error.is_some() {
                            OperationStatus::Failed
                        } else if args.simulation {
                            OperationStatus::Planned
                        } else {
                            OperationStatus::Copied
                        };
                        if let Some(error) = &copy_error {
                            error!("{error}");
                            something_failed = true;
                        }

                        let report_detail_entry = ReportDetailEntry {
                            rom_name: file_name.to_string_lossy().into_owned(),
                            status,
                            is_chd: !rom.data.chd.is_empty(),
                            error: copy_error,
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
                        status: OperationStatus::Ignored,
                        is_chd: false, // doesn't matter here
                        error: None,
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
            .all_ok(!something_failed);

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

    fn copy_rom(path: &Path, destination: &Path, args: &Args) -> io::Result<()> {
        if args.simulation {
            return Ok(());
        };

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
                    ChdStatus::BadDump | ChdStatus::NoDump => demote_working.push(name.clone()),
                    ChdStatus::Good => (),
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

/// Records the XML `build` attribute. The value is not checked against a supported version.
fn mame_build<'a>(doc: &'a Document, source: &Path) -> Result<&'a str, Box<dyn Error>> {
    let root = doc.root_element();
    if root.tag_name().name() != "mame" {
        return Err(format!("{}: root element must be 'mame'", source.display()).into());
    }
    root.attribute("build")
        .filter(|build| !build.is_empty())
        .ok_or_else(|| {
            format!(
                "{}: mame element is missing required attribute 'build'",
                source.display()
            )
            .into()
        })
}

fn worse_feature_status(own: FeatureStatus, overall: FeatureStatus) -> FeatureStatus {
    match (own, overall) {
        (FeatureStatus::Unemulated, _) | (_, FeatureStatus::Unemulated) => {
            FeatureStatus::Unemulated
        }
        _ => FeatureStatus::Imperfect,
    }
}

fn machine_error(source: &Path, machine: &str, detail: String) -> Box<dyn Error> {
    format!("Machine '{machine}' in {}: {detail}", source.display()).into()
}

fn parse_enum<T: FromStr>(
    source: &Path,
    machine: &str,
    attribute: &str,
    value: &str,
) -> Result<T, Box<dyn Error>> {
    T::from_str(value)
        .map_err(|_| machine_error(source, machine, format!("unknown {attribute} '{value}'")))
}

fn extract_status(
    node: Node,
    machine: &str,
    source: &Path,
) -> Result<Option<RomStatus>, Box<dyn Error>> {
    let mut status = None;
    for child in node.children() {
        if child.tag_name().name() != "driver" {
            continue;
        }
        let driver = required_attr(child, source, machine, "driver", "status")?;
        let emulation = required_attr(child, source, machine, "driver", "emulation")?;
        status = Some(RomStatus {
            driver: parse_enum(source, machine, "driver status", driver)?,
            emulation: parse_enum(source, machine, "driver emulation", emulation)?,
        });
    }
    Ok(status)
}

fn extract_features(
    node: Node,
    machine: &str,
    source: &Path,
) -> Result<Vec<Feature>, Box<dyn Error>> {
    let mut features = Vec::new();
    for child in node.children() {
        if child.tag_name().name() != "feature" {
            continue;
        }
        let typ = required_attr(child, source, machine, "feature", "type")?;
        let own = child.attribute("status").filter(|value| !value.is_empty());
        let overall = child.attribute("overall").filter(|value| !value.is_empty());
        // MAME's own flag is `status`. `overall` includes subdevices and can be worse.
        let status = match (own, overall) {
            (None, None) => {
                return Err(machine_error(
                    source,
                    machine,
                    "feature is missing required attribute 'status'".to_string(),
                ));
            }
            (Some(value), None) => parse_enum(source, machine, "feature status", value)?,
            (None, Some(value)) => parse_enum(source, machine, "feature overall", value)?,
            (Some(own), Some(overall)) => {
                let own = parse_enum(source, machine, "feature status", own)?;
                let overall = parse_enum(source, machine, "feature overall", overall)?;
                worse_feature_status(own, overall)
            }
        };
        features.push(Feature {
            typ: typ.to_string(),
            status,
        });
    }
    Ok(features)
}

fn extract_disks(node: Node, machine: &str, source: &Path) -> Result<Vec<Chd>, Box<dyn Error>> {
    let mut disks = Vec::new();
    for child in node.children() {
        if child.tag_name().name() != "disk" {
            continue;
        }
        // Each disk is parsed from its own attributes. A missing status is the
        // MAME default, Good, and must not reuse the previous disk's status.
        let name = required_attr(child, source, machine, "disk", "name")?;
        let status = match child.attribute("status") {
            None => ChdStatus::Good,
            Some(value) => parse_enum(source, machine, "disk status", value)?,
        };
        let optional = match child.attribute("optional") {
            None => false,
            Some(value) if value.eq_ignore_ascii_case("yes") => true,
            Some(value) if value.eq_ignore_ascii_case("no") => false,
            Some(value) => {
                return Err(machine_error(
                    source,
                    machine,
                    format!("disk '{name}' has unknown optional value '{value}'"),
                ));
            }
        };
        disks.push(Chd {
            name: name.to_string(),
            status,
            optional,
            merge: child
                .attribute("merge")
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            sha1: child
                .attribute("sha1")
                .filter(|value| !value.is_empty())
                .map(str::to_string),
        });
    }
    Ok(disks)
}

fn required_attr<'input>(
    node: Node<'input, 'input>,
    source: &Path,
    machine: &str,
    element: &str,
    attribute: &str,
) -> Result<&'input str, Box<dyn Error>> {
    node.attribute(attribute)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            machine_error(
                source,
                machine,
                format!("{element} is missing required attribute '{attribute}'"),
            )
        })
}

fn is_system(node: Node, name: &str, categories: &RomCategories) -> bool {
    if is_device(node) || machine_override(name).is_some() {
        return true;
    }
    // A missing category stays Unknown. It is not guessed to be a system.
    categories
        .get(name)
        .is_some_and(|category| category_preference(category).is_some())
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
