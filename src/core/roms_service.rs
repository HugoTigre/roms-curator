use std::collections::HashSet;
use std::error::Error;
use std::fs::read_dir;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use log::error;
use roxmltree::{Document, Node};

use crate::core::args::Args;
use crate::models::dependencies::{DependencyKind, DependencySet};
use crate::models::destination_folders::DestinationFolders;
use crate::models::disks::{DiskKind, DiskSet};
use crate::models::plan::{
    content_bytes, file_sha1, present_copies, Availability, MediaRole, MetadataIdentity, Plan,
    PlannedGame, PlannedMedia, UnresolvedMedia, PLAN_VERSION,
};
use crate::models::report::{OperationStatus, Report, ReportDetailEntry};
use crate::models::roms::{
    Catalog, Chd, ChdStatus, Feature, FeatureStatus, Rom, RomData, RomDataExt, RomFile, RomStatus,
    Roms, UnfilteredRoms, UNKNOWN_CATEGORY,
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
        let roms = categorize_machines(self.machines, &self.source, policy)?;
        let mut selected: Vec<String> = roms
            .iter()
            .filter(|(_, rom)| rom.selected)
            .map(|(name, _)| name.clone())
            .collect();
        selected.sort();
        let dependencies = crate::models::dependencies::resolve(&roms, &selected);
        let disks = crate::models::disks::resolve(&roms, &selected);
        Ok(Catalog {
            mame_build: self.mame_build,
            roms,
            selected,
            dependencies,
            disks,
            unknown_categories: self.unknown_categories,
            policy,
        })
    }
}

impl Catalog {
    /// Builds a sorted JSON plan for this catalog and these arguments.
    ///
    /// The plan lists selected games and their reasons, the files the copy would
    /// place in `roms` and `rest`, sizes, and destination conflicts. It does not
    /// create output directories, write `selected.txt`, or copy files.
    pub fn plan(&self, args: &Args) -> Result<crate::models::plan::Plan, Box<dyn Error>> {
        build_plan(self, args)
    }

    /// Copies the source into `roms` and `rest`, and writes `selected.txt` when every copy succeeds.
    ///
    /// If that file cannot be published, the call still returns the report with
    /// `all_ok` false and `names_error` set. Copied files stay.
    ///
    /// Builds one plan, rechecks that plan against the disk, then copies.
    /// `roms` receives every merged archive and CHD directory a selected game needs.
    /// `rest` receives every other source file this run keeps, so a full export can
    /// replace the original collection. A name-range subset still leaves stems
    /// outside the range in the source, except required archives and disk directories.
    /// [`RomsExt::copy_roms`] applies the range only and places every copied file in `rest`.
    pub fn copy_roms(self, args: &Args) -> Result<Report, Box<dyn Error>> {
        let plan = self.plan(args)?;
        self.copy_with_plan(args, &plan)
    }

    /// Rechecks `plan` and copies. Use this for a plan just written or loaded
    /// from `--plan-path`, so the copy does not build a second plan.
    ///
    /// The plan's sources, destination, subset, report path, metadata paths,
    /// and emulation policy must match this catalog and these arguments.
    /// Paths are compared after resolution. The copy still follows `args`.
    pub fn copy_with_plan(
        self,
        args: &Args,
        plan: &crate::models::plan::Plan,
    ) -> Result<Report, Box<dyn Error>> {
        require_same_run(&self, args, plan)?;
        plan.recheck()?;
        if let Some(conflict) = plan.conflicts.first() {
            return Err(conflict.clone().into());
        }
        let retain = retained_archives(&self.dependencies, args);
        let disks = required_disk_machines(&self.disks, args);
        copy_retaining(self.roms, args, &retain, Some(&disks))
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
                devices: extract_devices(node, name, source)?,
                roms: extract_roms(node, name, source)?,
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

    Ok(roms)
}

pub trait RomsExt {
    ///
    /// Copies source entries into `rest` and writes `selected.txt` when every copy succeeds.
    ///
    /// This path has no dependency resolution, so copied files go to `rest`.
    /// [`Catalog::copy_roms`] places required archives and CHD directories in `roms`
    /// and everything else this run keeps in `rest`. Conflicting destination names
    /// (including case-only differences), existing files or CHD directories, an
    /// existing `selected.txt`, and report paths inside those directories abort
    /// the run before the directories are created or ROMs are copied. Files are
    /// never overwritten and CHD directories are not merged.
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
    fn should_move(rom: &Rom, args: &Args) -> bool;
    /// Copy an archive or directory, returning source/destination paths and the
    /// cause on failure. Simulation performs no copy and returns `Ok(())`.
    fn copy_rom(path: &Path, destination: &Path, args: &Args) -> io::Result<()>;
    fn is_excluded(args: &Args, file_prefix: &str) -> bool;
}

fn require_same_run(catalog: &Catalog, args: &Args, plan: &Plan) -> Result<(), Box<dyn Error>> {
    let mut problems = Vec::new();
    let policy = match catalog.policy {
        EmulationPolicy::Playable => "playable",
        EmulationPolicy::Strict => "strict",
    };
    if plan.metadata.emulation_policy != policy {
        problems.push(format!(
            "emulation policy is {}, catalog is {policy}",
            plan.metadata.emulation_policy
        ));
    }
    if plan.subset_start != args.subset_start || plan.subset_end != args.subset_end {
        problems.push(format!(
            "subset is '{}'..'{}', arguments are '{}'..'{}'",
            plan.subset_start, plan.subset_end, args.subset_start, args.subset_end
        ));
    }
    same_path(
        &mut problems,
        "destination",
        &plan.destination_path,
        &args.destination_path,
    )?;
    same_path(
        &mut problems,
        "selected names",
        &plan.selected_names_path,
        &args.selected_names_path().display().to_string(),
    )?;
    same_path(
        &mut problems,
        "report path",
        &plan.report_path,
        &args.report_path,
    )?;
    same_path(
        &mut problems,
        "MAME XML",
        &plan.metadata.mame_xml_path,
        &args.mame_xml_path,
    )?;
    same_path(
        &mut problems,
        "CatVer",
        &plan.metadata.catver_path,
        &args.catver_path,
    )?;
    if plan.source_paths.len() != args.source_path.len() {
        problems.push(format!(
            "source list has {} directories, arguments have {}",
            plan.source_paths.len(),
            args.source_path.len()
        ));
    } else {
        for (planned, given) in plan.source_paths.iter().zip(&args.source_path) {
            same_path(&mut problems, "source", planned, given)?;
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Plan does not match this catalog and arguments: {}",
            problems.join("; ")
        )
        .into())
    }
}

fn same_path(
    problems: &mut Vec<String>,
    label: &str,
    planned: &str,
    given: &str,
) -> Result<(), Box<dyn Error>> {
    if planned.is_empty() || given.is_empty() {
        if planned != given {
            problems.push(format!("{label} is set on only one side"));
        }
        return Ok(());
    }
    let planned = crate::core::paths::resolve(Path::new(planned))?;
    let given = crate::core::paths::resolve(Path::new(given))?;
    if planned != given {
        problems.push(format!(
            "{label} {} does not match {}",
            planned.display(),
            given.display()
        ));
    }
    Ok(())
}

fn build_plan(catalog: &Catalog, args: &Args) -> Result<Plan, Box<dyn Error>> {
    crate::core::paths::validate(args)?;
    let retain = retained_archives(&catalog.dependencies, args);
    let disk_machines = required_disk_machines(&catalog.disks, args);
    let folders = args.build_destination_folders_path();
    let mut media = Vec::new();
    let mut seen_archives = HashSet::new();
    let mut seen_disks = HashSet::new();

    for source in &args.source_path {
        let mut entries = read_dir(source)
            .map_err(|error| format!("Cannot read source directory {source}: {error}"))?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Cannot read entry in source directory {source}: {error}"))?;
        entries.sort();
        for path in entries {
            let prefix = path
                .file_stem()
                .and_then(|name| name.to_str())
                .ok_or_else(|| format!("Unsupported ROM name: {}", path.display()))?;
            if plan_source_entry(
                &path,
                prefix,
                &catalog.roms,
                args,
                &retain,
                Some(&disk_machines),
            ) != EntryPlan::Copy
            {
                continue;
            }
            let key = prefix.to_ascii_lowercase();
            let machine_has_disks = catalog
                .roms
                .get(&key)
                .is_some_and(|rom| !rom.data.chd.is_empty());
            let disk = is_disk_media(&path, machine_has_disks);
            let required = file_is_required(disk, &key, &retain, Some(&disk_machines));
            let destination = export_directory(required, &folders).join(
                path.file_name()
                    .ok_or_else(|| format!("Missing ROM filename: {}", path.display()))?,
            );
            let role = if disk {
                MediaRole::Disk
            } else {
                MediaRole::Archive
            };
            if disk {
                seen_disks.insert(key.clone());
            } else {
                seen_archives.insert(key.clone());
            }
            let bytes = content_bytes(&path)?;
            media.push(PlannedMedia {
                machine: key.clone(),
                role,
                availability: Availability::Present,
                source: Some(path.display().to_string()),
                destination: Some(destination.display().to_string()),
                bytes: Some(bytes),
                reasons: if required {
                    media_reasons(&key, role, catalog, args)
                } else {
                    vec!["Not required by a selected game".to_string()]
                },
            });
        }
    }

    for archive in required_archive_names(&catalog.dependencies, args) {
        if seen_archives.contains(&archive) {
            continue;
        }
        media.push(PlannedMedia {
            machine: archive.clone(),
            role: MediaRole::Archive,
            availability: Availability::Missing,
            source: None,
            destination: None,
            bytes: None,
            reasons: media_reasons(&archive, MediaRole::Archive, catalog, args),
        });
    }
    let mut disks = disk_machines.into_iter().collect::<Vec<_>>();
    disks.sort();
    for machine in disks {
        if seen_disks.contains(&machine) {
            continue;
        }
        media.push(PlannedMedia {
            machine: machine.clone(),
            role: MediaRole::Disk,
            availability: Availability::Missing,
            source: None,
            destination: None,
            bytes: None,
            reasons: media_reasons(&machine, MediaRole::Disk, catalog, args),
        });
    }
    media.sort();

    let directories = [folders.roms.as_path(), folders.rest.as_path()];
    let names_path = args.selected_names_path();
    let conflicts = crate::core::copying::destination_conflicts(
        &present_copies(&media),
        &directories,
        &args.report_path,
        &names_path,
    )?;

    let mut games = Vec::new();
    for name in &catalog.selected {
        if Roms::is_excluded(args, name) {
            continue;
        }
        let rom = &catalog.roms[name];
        games.push(PlannedGame {
            machine: name.clone(),
            category: rom.data.category.clone(),
            folder: rom.category.to_string(),
            reason: catalog.policy.effect().to_string(),
        });
    }

    Ok(Plan {
        version: PLAN_VERSION,
        metadata: MetadataIdentity {
            mame_build: catalog.mame_build.clone(),
            emulation_policy: match catalog.policy {
                EmulationPolicy::Playable => "playable",
                EmulationPolicy::Strict => "strict",
            }
            .to_string(),
            mame_xml_path: args.mame_xml_path.clone(),
            mame_xml_sha1: file_sha1(&args.mame_xml_path)?,
            catver_path: args.catver_path.clone(),
            catver_sha1: file_sha1(&args.catver_path)?,
        },
        source_paths: args.source_path.clone(),
        destination_path: args.destination_path.clone(),
        selected_names_path: args.selected_names_path().display().to_string(),
        report_path: args.report_path.clone(),
        subset_start: args.subset_start.clone(),
        subset_end: args.subset_end.clone(),
        games,
        media,
        unresolved: unresolved_media(catalog, args),
        conflicts,
    })
}

fn required_archive_names(dependencies: &DependencySet, args: &Args) -> Vec<String> {
    let mut names = dependencies
        .edges
        .iter()
        .filter(|edge| !Roms::is_excluded(args, &edge.game))
        .map(|edge| edge.archive.to_ascii_lowercase())
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

fn media_reasons(machine: &str, role: MediaRole, catalog: &Catalog, args: &Args) -> Vec<String> {
    let mut reasons = Vec::new();
    if role == MediaRole::Archive
        && catalog
            .selected
            .iter()
            .any(|game| game.eq_ignore_ascii_case(machine) && !Roms::is_excluded(args, game))
    {
        reasons.push(format!("Selected game {machine}"));
    }
    if role == MediaRole::Archive {
        for edge in &catalog.dependencies.edges {
            if edge.archive.eq_ignore_ascii_case(machine) && !Roms::is_excluded(args, &edge.game) {
                let why = match edge.kind {
                    DependencyKind::Own => "Own archive",
                    DependencyKind::Parent => "Parent archive",
                    DependencyKind::RomParent => "ROM parent archive",
                    DependencyKind::Device => "Device archive",
                };
                reasons.push(format!("{why} for {}", edge.game));
            }
        }
    }
    if role == MediaRole::Disk {
        for edge in &catalog.disks.edges {
            if edge.machine.eq_ignore_ascii_case(machine) && !Roms::is_excluded(args, &edge.game) {
                let why = match edge.kind {
                    DiskKind::Own => "Own disk",
                    DiskKind::Shared => "Shared disk",
                    DiskKind::DeltaParent => "Delta parent disk",
                    DiskKind::Device => "Device disk",
                };
                reasons.push(format!("{why} {} for {}", edge.name, edge.game));
            }
        }
    }
    if reasons.is_empty() {
        if let Some(rom) = catalog.roms.get(machine) {
            reasons.push(format!("Category {}", rom.category));
        }
    }
    reasons.sort();
    reasons.dedup();
    reasons
}

fn unresolved_media(catalog: &Catalog, args: &Args) -> Vec<UnresolvedMedia> {
    let mut unresolved = Vec::new();
    for missing in &catalog.dependencies.missing {
        if Roms::is_excluded(args, &missing.game) {
            continue;
        }
        let kind = match missing.kind {
            DependencyKind::Own => "own-archive",
            DependencyKind::Parent => "parent-archive",
            DependencyKind::RomParent => "rom-parent-archive",
            DependencyKind::Device => "device-archive",
        };
        unresolved.push(UnresolvedMedia {
            game: missing.game.clone(),
            kind: kind.to_string(),
            name: missing.name.clone(),
        });
    }
    for cycle in &catalog.dependencies.cycles {
        unresolved.push(UnresolvedMedia {
            game: String::new(),
            kind: "dependency-cycle".to_string(),
            name: cycle.join(" -> "),
        });
    }
    for missing in &catalog.disks.missing {
        if Roms::is_excluded(args, &missing.game) {
            continue;
        }
        let kind = match missing.kind {
            DiskKind::Own => "own-disk",
            DiskKind::Shared => "shared-disk",
            DiskKind::DeltaParent => "delta-parent",
            DiskKind::Device => "device-disk",
        };
        unresolved.push(UnresolvedMedia {
            game: missing.game.clone(),
            kind: kind.to_string(),
            name: missing.name.clone(),
        });
    }
    for cycle in &catalog.disks.cycles {
        unresolved.push(UnresolvedMedia {
            game: String::new(),
            kind: "disk-cycle".to_string(),
            name: cycle.join(" -> "),
        });
    }
    unresolved.sort();
    unresolved
}

fn copy_retaining(
    roms: Roms,
    args: &Args,
    retain: &HashSet<String>,
    disk_machines: Option<&HashSet<String>>,
) -> Result<Report, Box<dyn Error>> {
    Roms::check_paths(args)?;
    crate::core::paths::validate(args)?;

    let destination_paths = args.build_destination_folders_path();
    let names_path = args.selected_names_path();
    let directories = [
        destination_paths.roms.as_path(),
        destination_paths.rest.as_path(),
    ];
    let mut sources = Vec::new();
    let mut copies = Vec::new();
    for source in &args.source_path {
        let mut entries = read_dir(source)
            .map_err(|error| format!("Cannot read source directory {source}: {error}"))?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Cannot read entry in source directory {source}: {error}"))?;
        entries.sort();
        for path in &entries {
            let prefix = path
                .file_stem()
                .and_then(|name| name.to_str())
                .ok_or_else(|| format!("Unsupported ROM name: {}", path.display()))?;
            if plan_source_entry(path, prefix, &roms, args, retain, disk_machines)
                == EntryPlan::Copy
            {
                let destination = export_destination(
                    path,
                    prefix,
                    &roms,
                    retain,
                    disk_machines,
                    &destination_paths,
                )?;
                copies.push((path.clone(), destination));
            }
        }
        sources.push((source, entries));
    }
    crate::core::copying::check_targets(&copies, &directories, &args.report_path, &names_path)?;
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

            match plan_source_entry(&path, file_prefix, &roms, args, retain, disk_machines) {
                EntryPlan::Skip => continue,
                EntryPlan::Copy => {
                    let key = file_prefix.to_ascii_lowercase();
                    let machine_has_disks =
                        roms.get(&key).is_some_and(|rom| !rom.data.chd.is_empty());
                    let disk = is_disk_media(&path, machine_has_disks);
                    let required = file_is_required(disk, &key, retain, disk_machines);
                    let destination =
                        export_directory(required, &destination_paths).join(file_name);

                    let copy_error = Roms::copy_rom(&path, &destination, args)
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
                        is_chd: disk,
                        error: copy_error,
                    };

                    if required {
                        total_working += 1;
                        report.add_rom_working(report_detail_entry);
                    } else {
                        total_other += 1;
                        report.add_rom_other(report_detail_entry);
                    }
                }
            }
        }

        if let Some(pb) = progress_bar {
            pb.finish();
        }
    }

    if !args.simulation && !something_failed {
        if let Err(error) = write_selected_names(&roms, args, &names_path) {
            error!("{error}");
            something_failed = true;
            report.names_error = Some(error.to_string());
        }
    }

    report
        .total_working(total_working)
        .total_other(total_other)
        .all_ok(!something_failed);

    Ok(report)
}

/// Machine names whose CHD directory a selected game needs.
///
/// The range limits selected games. A parent directory that stores a required
/// shared or delta disk stays in `roms` even when its name falls outside the range.
/// Other disk directories go to `rest` when this run copies them.
pub(crate) fn required_disk_machines(disks: &DiskSet, args: &Args) -> HashSet<String> {
    // The flag remains accepted so existing commands parse. It does not choose a folder.
    let _retired = args.ignore_not_working_chd;
    disks
        .edges
        .iter()
        .filter(|edge| !Roms::is_excluded(args, &edge.game))
        .map(|edge| edge.machine.to_ascii_lowercase())
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EntryPlan {
    Copy,
    Skip,
}

fn plan_source_entry(
    path: &Path,
    prefix: &str,
    roms: &Roms,
    args: &Args,
    retain: &HashSet<String>,
    disk_machines: Option<&HashSet<String>>,
) -> EntryPlan {
    let key = prefix.to_ascii_lowercase();
    let machine_has_disks = roms.get(&key).is_some_and(|rom| !rom.data.chd.is_empty());
    let disk = is_disk_media(path, machine_has_disks);
    // A required file stays even when its name falls outside the range. Every
    // other name outside the range stays in the source.
    if !file_is_required(disk, &key, retain, disk_machines) && subset_excludes(args, prefix, retain)
    {
        EntryPlan::Skip
    } else {
        EntryPlan::Copy
    }
}

fn file_is_required(
    disk: bool,
    key: &str,
    retain: &HashSet<String>,
    disk_machines: Option<&HashSet<String>>,
) -> bool {
    if disk {
        disk_machines.is_some_and(|required| required.contains(key))
    } else {
        retain.contains(key)
    }
}

fn export_directory(required: bool, folders: &DestinationFolders) -> PathBuf {
    if required {
        folders.roms.clone()
    } else {
        folders.rest.clone()
    }
}

fn export_destination(
    path: &Path,
    prefix: &str,
    roms: &Roms,
    retain: &HashSet<String>,
    disk_machines: Option<&HashSet<String>>,
    folders: &DestinationFolders,
) -> Result<PathBuf, Box<dyn Error>> {
    let key = prefix.to_ascii_lowercase();
    let machine_has_disks = roms.get(&key).is_some_and(|rom| !rom.data.chd.is_empty());
    let required = file_is_required(
        is_disk_media(path, machine_has_disks),
        &key,
        retain,
        disk_machines,
    );
    let file_name = path
        .file_name()
        .ok_or_else(|| format!("Missing ROM filename: {}", path.display()))?;
    Ok(export_directory(required, folders).join(file_name))
}

fn write_selected_names(roms: &Roms, args: &Args, path: &Path) -> Result<(), Box<dyn Error>> {
    let mut names: Vec<&str> = roms
        .iter()
        .filter(|(name, rom)| rom.selected && !Roms::is_excluded(args, name))
        .map(|(name, _)| name.as_str())
        .collect();
    names.sort_unstable();
    let mut bytes = names.join("\n");
    if !bytes.is_empty() {
        bytes.push('\n');
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::Builder::new()
        .prefix(".roms-curator-names-")
        .tempfile_in(parent)
        .map_err(|error| format!("Cannot create {}: {error}", path.display()))?;
    std::io::Write::write_all(temporary.as_file_mut(), bytes.as_bytes())
        .map_err(|error| format!("Cannot write {}: {error}", path.display()))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| format!("Cannot write {}: {error}", path.display()))?;
    temporary
        .persist_noclobber(path)
        .map_err(|error| format!("Cannot publish {}: {}", path.display(), error.error))?;
    Ok(())
}

fn is_disk_media(path: &Path, machine_has_disks: bool) -> bool {
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("chd"))
    {
        return true;
    }
    path.is_dir() && machine_has_disks
}

/// Merged archive stems a selected game needs.
///
/// A clone contributes its parent archive, not its own name. A name-range subset
/// still copies these archives when the archive name falls outside the range.
/// With no range, this is every archive the full selected list needs; other
/// source files go to `rest`.
pub(crate) fn retained_archives(dependencies: &DependencySet, args: &Args) -> HashSet<String> {
    dependencies
        .edges
        .iter()
        .filter(|edge| !Roms::is_excluded(args, &edge.game))
        .map(|edge| edge.archive.to_ascii_lowercase())
        .collect()
}

pub(crate) fn subset_excludes(args: &Args, file_prefix: &str, retain: &HashSet<String>) -> bool {
    Roms::is_excluded(args, file_prefix) && !retain.contains(&file_prefix.to_ascii_lowercase())
}

impl RomsExt for Roms {
    fn copy_roms(self, args: &Args) -> Result<Report, Box<dyn Error>> {
        copy_retaining(self, args, &HashSet::new(), None)
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

    fn should_move(_rom: &Rom, args: &Args) -> bool {
        // Disk retention is decided from `Catalog.disks` in `copy_retaining`.
        // This flag no longer drops a CHD directory a selected game needs.
        let _retired = args.ignore_not_working_chd;
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
        let status = dump_status(child, source, machine, "disk status")?;
        let optional = optional_flag(child, source, machine, "disk", name)?;
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

fn extract_roms(node: Node, machine: &str, source: &Path) -> Result<Vec<RomFile>, Box<dyn Error>> {
    let mut roms = Vec::new();
    for child in node.children() {
        if child.tag_name().name() != "rom" {
            continue;
        }
        let name = required_attr(child, source, machine, "rom", "name")?;
        let size = match child.attribute("size") {
            None => None,
            Some(value) => Some(value.parse::<u64>().map_err(|_| {
                machine_error(
                    source,
                    machine,
                    format!("rom '{name}' has unknown size '{value}'"),
                )
            })?),
        };
        roms.push(RomFile {
            name: name.to_string(),
            size,
            crc: owned_attr(child, "crc"),
            sha1: owned_attr(child, "sha1"),
            merge: owned_attr(child, "merge"),
            bios: owned_attr(child, "bios"),
            status: dump_status(child, source, machine, "rom status")?,
            optional: optional_flag(child, source, machine, "rom", name)?,
        });
    }
    Ok(roms)
}

fn extract_devices(
    node: Node,
    machine: &str,
    source: &Path,
) -> Result<Vec<String>, Box<dyn Error>> {
    let mut devices = Vec::new();
    for child in node.children() {
        if child.tag_name().name() != "device_ref" {
            continue;
        }
        devices.push(required_attr(child, source, machine, "device_ref", "name")?.to_string());
    }
    Ok(devices)
}

fn owned_attr(node: Node, attribute: &str) -> Option<String> {
    node.attribute(attribute)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn dump_status(
    node: Node,
    source: &Path,
    machine: &str,
    attribute: &str,
) -> Result<ChdStatus, Box<dyn Error>> {
    match node.attribute("status") {
        None => Ok(ChdStatus::Good),
        Some(value) => parse_enum(source, machine, attribute, value),
    }
}

fn optional_flag(
    node: Node,
    source: &Path,
    machine: &str,
    element: &str,
    name: &str,
) -> Result<bool, Box<dyn Error>> {
    match node.attribute("optional") {
        None => Ok(false),
        Some(value) if value.eq_ignore_ascii_case("yes") => Ok(true),
        Some(value) if value.eq_ignore_ascii_case("no") => Ok(false),
        Some(value) => Err(machine_error(
            source,
            machine,
            format!("{element} '{name}' has unknown optional value '{value}'"),
        )),
    }
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
