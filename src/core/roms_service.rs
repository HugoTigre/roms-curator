use std::collections::HashSet;
use std::error::Error;
use std::fs::{read_dir, File};
use std::io::{BufRead, BufReader, Cursor};
use std::path::{Path, PathBuf};

use crate::cancel::{check_cancelled, CancelFlag};
use log::error;

use crate::core::config::{DestinationFolders, ExportConfig};
use crate::core::export::{check_export_paths, copy_source_entry, name_in_subset_excluded};
use crate::core::launch_ready::{export_game_names, export_game_set, game_exports};
use crate::models::dependencies::{DependencyKind, DependencySet};
use crate::models::disks::{DiskKind, DiskSet};
use crate::models::plan::{
    chd_pairs_in_sources, content_bytes, file_sha1, present_copies, source_has_chd, Availability,
    MediaRole, MetadataIdentity, Plan, PlannedGame, PlannedMedia, UnresolvedMedia, PLAN_VERSION,
};
use crate::models::report::{
    ExportMediaKind, MissingBiosEntry, MissingChdEntry, OperationStatus, Report, ReportDetailEntry,
};
use crate::models::roms::{Catalog, Roms, UnfilteredRoms};
use crate::models::roms::{Rom, RomCategory};
use crate::models::selection::{category_preference, machine_override, EmulationPolicy};
use crate::utils::{build_progress_bar, ProgressBarEx, ThrottledProgress};
use crate::RomCategories;

/// Plan reason for media routed to `rest` rather than required in `roms`.
pub const REST_MEDIA_REASON: &str = "Not required by a selected game";

/// True when a planned media row belongs in the `roms` support set for selected games.
pub fn planned_media_for_roms(media: &PlannedMedia) -> bool {
    media
        .reasons
        .iter()
        .any(|reason| reason != REST_MEDIA_REASON)
}

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

impl From<crate::core::listxml::MameListXml> for ParsedMachines {
    fn from(parsed: crate::core::listxml::MameListXml) -> Self {
        Self {
            mame_build: parsed.mame_build,
            machines: parsed.machines,
            unknown_categories: parsed.unknown_categories,
            source: parsed.source,
        }
    }
}

/// Parses MAME listxml from a readable byte stream (streaming; no full DOM).
pub fn parse_mame_xml<R: BufRead>(
    source: &Path,
    reader: R,
    categories: &RomCategories,
) -> Result<ParsedMachines, Box<dyn Error>> {
    Ok(crate::core::listxml::parse(source, reader, categories)?.into())
}

/// Parses MAME listxml from an on-disk file.
pub fn parse_mame_xml_file(
    source: &Path,
    categories: &RomCategories,
) -> Result<ParsedMachines, Box<dyn Error>> {
    let file = File::open(source)
        .map_err(|error| format!("Cannot read MAME XML {}: {error}", source.display()))?;
    parse_mame_xml(source, BufReader::new(file), categories)
}

/// Parses MAME listxml from a string (used in unit tests and small fixtures).
pub fn parse_mame_xml_from_str(
    source: &Path,
    contents: &str,
    categories: &RomCategories,
) -> Result<ParsedMachines, Box<dyn Error>> {
    parse_mame_xml(source, Cursor::new(contents.as_bytes()), categories)
}

impl Catalog {
    /// Builds a sorted JSON plan for this catalog and these arguments.
    ///
    /// The plan lists selected games and their reasons, the files the copy would
    /// place in `roms` and `rest`, sizes, and destination conflicts. It does not
    /// create output directories, write `selected.txt`, or copy files.
    ///
    /// Call [`ExportConfig::validate`] before this; path checks are not repeated here.
    pub fn plan(&self, args: &ExportConfig) -> Result<crate::models::plan::Plan, Box<dyn Error>> {
        build_plan(self, args, None, None)
    }

    /// Same as [`plan`], but reports coarse steps through `on_status` while scanning sources.
    ///
    /// `cancel` is polled before the scan, before each source directory, before the
    /// destination-conflict check, and again after the MAME XML and CatVer hashes,
    /// immediately before the plan is returned.
    pub fn plan_with_progress(
        &self,
        args: &ExportConfig,
        on_status: Option<&mut dyn FnMut(&str)>,
        cancel: Option<&CancelFlag>,
    ) -> Result<crate::models::plan::Plan, Box<dyn Error>> {
        build_plan(self, args, on_status, cancel)
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
    /// [`copy_all_to_rest`] applies the range only and places every copied file in `rest`.
    pub fn copy_roms(self, config: &ExportConfig) -> Result<Report, Box<dyn Error>> {
        config.validate()?;
        let plan = self.plan(config)?;
        self.copy_with_plan_progress(config, &plan, None, None)
    }

    /// Rechecks `plan` and copies. Use this for a plan just written or loaded
    /// from `--plan-path`, so the copy does not build a second plan.
    ///
    /// The plan's sources, destination, subset, report path, metadata paths,
    /// and emulation policy must match this catalog and these arguments.
    /// Paths are compared after resolution. The copy still follows `config`.
    ///
    /// When `cancel` is set and becomes cancelled, copying stops. Files already
    /// copied stay on disk; `selected.txt` is not published.
    pub fn copy_with_plan(
        self,
        config: &ExportConfig,
        plan: &crate::models::plan::Plan,
        cancel: Option<&CancelFlag>,
    ) -> Result<Report, Box<dyn Error>> {
        self.copy_with_plan_progress(config, plan, cancel, None)
    }

    /// Like [`copy_with_plan`], but reports coarse copy progress through `on_copy_progress`
    /// when provided (`pos` and `len` count source directory entries, including skips).
    pub fn copy_with_plan_progress(
        self,
        config: &ExportConfig,
        plan: &crate::models::plan::Plan,
        cancel: Option<&CancelFlag>,
        on_copy_progress: Option<&mut dyn FnMut(u64, u64)>,
    ) -> Result<Report, Box<dyn Error>> {
        require_same_run(&self, config, plan)?;
        plan.recheck()?;
        if let Some(conflict) = plan.conflicts.first() {
            return Err(conflict.clone().into());
        }
        let export_games = export_game_set(&self, config)?;
        let retain = retained_archives(&self.dependencies, config, &export_games);
        let disks = required_disk_machines(&self.disks, config, &export_games);
        let missing_chds = missing_chds_for_report(&self, config)?;
        let export_names = export_game_names(&self, config)?;
        copy_retaining(
            self.roms,
            config,
            &retain,
            Some(&disks),
            missing_chds,
            export_names,
            cancel,
            on_copy_progress,
        )
    }
}

/// Copies every source entry into `rest` without dependency resolution or `selected.txt`.
///
/// [`Catalog::copy_roms`] places required archives and CHD directories in `roms`
/// and everything else this run keeps in `rest`.
pub fn copy_all_to_rest(roms: Roms, config: &ExportConfig) -> Result<Report, Box<dyn Error>> {
    copy_retaining(
        roms,
        config,
        &HashSet::new(),
        None,
        Vec::new(),
        Vec::new(),
        None,
        None,
    )
}

fn categorize_machines(
    machines: UnfilteredRoms,
    source: &Path,
    policy: EmulationPolicy,
) -> Result<Roms, Box<dyn Error>> {
    let mut roms = Roms::new();

    for (name, data) in machines {
        let categorized_rom = if data.is_bios {
            (name, data.into_bios())
        } else if data.is_system {
            (name, data.into_system())
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
                (name, data.into_not_working())
            } else {
                (name, data.into_working())
            }
        };

        roms.insert(categorized_rom.0.clone(), categorized_rom.1);
    }

    Ok(roms)
}

fn require_same_run(
    catalog: &Catalog,
    config: &ExportConfig,
    plan: &Plan,
) -> Result<(), Box<dyn Error>> {
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
    if plan.subset_start != config.subset_start || plan.subset_end != config.subset_end {
        problems.push(format!(
            "subset is '{}'..'{}', arguments are '{}'..'{}'",
            plan.subset_start, plan.subset_end, config.subset_start, config.subset_end
        ));
    }
    same_path(
        &mut problems,
        "destination",
        &plan.destination_path,
        &config.destination_path,
    )?;
    same_path(
        &mut problems,
        "selected names",
        &plan.selected_names_path,
        &config.selected_names_path().display().to_string(),
    )?;
    same_path(
        &mut problems,
        "report path",
        &plan.report_path,
        &config.report_path,
    )?;
    same_path(
        &mut problems,
        "MAME XML",
        &plan.metadata.mame_xml_path,
        &config.mame_xml_path,
    )?;
    same_path(
        &mut problems,
        "CatVer",
        &plan.metadata.catver_path,
        &config.catver_path,
    )?;
    if plan.source_paths.len() != config.source_path.len() {
        problems.push(format!(
            "source list has {} directories, arguments have {}",
            plan.source_paths.len(),
            config.source_path.len()
        ));
    } else {
        for (planned, given) in plan.source_paths.iter().zip(&config.source_path) {
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

fn build_plan(
    catalog: &Catalog,
    args: &ExportConfig,
    mut on_status: Option<&mut dyn FnMut(&str)>,
    cancel: Option<&CancelFlag>,
) -> Result<Plan, Box<dyn Error>> {
    let mut status = |message: &str| {
        if let Some(callback) = on_status.as_deref_mut() {
            callback(message);
        }
    };

    status("Scanning source collection for export plan…");
    check_cancelled(cancel)?;
    let export_games = export_game_set(catalog, args)?;
    let export_names = export_game_names(catalog, args)?;
    let retain = retained_archives(&catalog.dependencies, args, &export_games);
    let disk_machines = required_disk_machines(&catalog.disks, args, &export_games);
    let folders = args.destination_folders();
    let mut media = Vec::new();
    let mut seen_archives = HashSet::new();
    let mut seen_disks = HashSet::new();

    for source in &args.source_path {
        status(&format!("Scanning {} for media…", source));
        check_cancelled(cancel)?;
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
                    media_reasons(&key, role, catalog, args, &export_games)
                } else {
                    vec![REST_MEDIA_REASON.to_string()]
                },
            });
        }
    }

    for archive in required_archive_names(&catalog.dependencies, args, &export_games) {
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
            reasons: media_reasons(&archive, MediaRole::Archive, catalog, args, &export_games),
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
            reasons: media_reasons(&machine, MediaRole::Disk, catalog, args, &export_games),
        });
    }
    media.sort();

    status("Checking destination conflicts…");
    check_cancelled(cancel)?;
    let directories = [folders.roms.as_path(), folders.rest.as_path()];
    let names_path = args.selected_names_path();
    let conflicts = crate::core::copying::destination_conflicts(
        &present_copies(&media),
        &directories,
        &args.report_path,
        &names_path,
    )?;

    let mut games = Vec::new();
    for name in &export_names {
        let rom = &catalog.roms[name];
        games.push(PlannedGame {
            machine: name.clone(),
            category: rom.data.category.clone(),
            folder: rom.category.to_string(),
            reason: catalog.policy.effect().to_string(),
        });
    }

    let mame_xml_sha1 = file_sha1(&args.mame_xml_path)?;
    let catver_sha1 = file_sha1(&args.catver_path)?;
    check_cancelled(cancel)?;

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
            mame_xml_sha1,
            catver_path: args.catver_path.clone(),
            catver_sha1,
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

fn required_archive_names(
    dependencies: &DependencySet,
    args: &ExportConfig,
    export_games: &HashSet<String>,
) -> Vec<String> {
    let mut names = dependencies
        .edges
        .iter()
        .filter(|edge| game_exports(&edge.game, export_games, args))
        .map(|edge| edge.archive.to_ascii_lowercase())
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

fn media_reasons(
    machine: &str,
    role: MediaRole,
    catalog: &Catalog,
    args: &ExportConfig,
    export_games: &HashSet<String>,
) -> Vec<String> {
    let mut reasons = Vec::new();
    if role == MediaRole::Archive && export_games.contains(&machine.to_ascii_lowercase()) {
        reasons.push(format!("Selected game {machine}"));
    }
    if role == MediaRole::Archive {
        for edge in &catalog.dependencies.edges {
            if edge.archive.eq_ignore_ascii_case(machine)
                && game_exports(&edge.game, export_games, args)
            {
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
            if edge.machine.eq_ignore_ascii_case(machine)
                && game_exports(&edge.game, export_games, args)
            {
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

fn unresolved_media(catalog: &Catalog, args: &ExportConfig) -> Vec<UnresolvedMedia> {
    let mut unresolved = Vec::new();
    for missing in &catalog.dependencies.missing {
        if name_in_subset_excluded(args, &missing.game) {
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
        if name_in_subset_excluded(args, &missing.game) {
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

pub use crate::core::launch_ready::held_back_games;

/// Arcade-style machines left off the selected list, with a short reason.
pub fn excluded_games_for_review(catalog: &Catalog, args: &ExportConfig) -> Vec<PlannedGame> {
    let mut games = Vec::new();
    for (name, rom) in &catalog.roms {
        if rom.selected {
            continue;
        }
        if matches!(rom.category, RomCategory::Bios | RomCategory::System) {
            continue;
        }
        if name_in_subset_excluded(args, name) {
            continue;
        }
        if !matches!(
            rom.category,
            RomCategory::NotWorking | RomCategory::UnCategorized
        ) {
            continue;
        }
        games.push(PlannedGame {
            machine: name.clone(),
            category: rom.data.category.clone(),
            folder: rom.category.to_string(),
            reason: exclusion_reason(name, rom, catalog.policy),
        });
    }
    games.sort_by(|left, right| left.machine.cmp(&right.machine));
    games
}

fn exclusion_reason(name: &str, rom: &Rom, policy: EmulationPolicy) -> String {
    if let Some(override_rule) = machine_override(name) {
        return format!("{} {}", override_rule.effect, override_rule.reason);
    }
    if rom.data.is_mechanical {
        return "Mechanical machine; not on the arcade game list.".into();
    }
    if let Some(preference) = category_preference(&rom.data.category) {
        return format!("{} {}", preference.effect, preference.reason);
    }
    if let Some(status) = &rom.data.status {
        if !policy.accepts(status) {
            return format!(
                "Driver is {:?}, emulation is {:?}; not accepted under the current selection policy.",
                status.driver, status.emulation
            );
        }
    }
    "Not on the selected arcade list.".into()
}

fn policy_selected_game(catalog: &Catalog, config: &ExportConfig, game: &str) -> bool {
    !name_in_subset_excluded(config, game) && catalog.selected.iter().any(|name| name == game)
}

/// ROM zip stems for `.zip` archives found under the source roots.
fn archives_in_sources(roots: &[String]) -> Result<HashSet<String>, Box<dyn Error>> {
    use std::fs::read_dir;
    let mut stems = HashSet::new();
    for root in roots {
        for entry in read_dir(root)
            .map_err(|error| format!("Cannot read source directory {root}: {error}"))?
        {
            let path = entry
                .map_err(|error| format!("Cannot read entry in source directory {root}: {error}"))?
                .path();
            if !path.is_file() {
                continue;
            }
            if path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_none_or(|ext| !ext.eq_ignore_ascii_case("zip"))
            {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|name| name.to_str()) {
                stems.insert(stem.to_ascii_lowercase());
            }
        }
    }
    Ok(stems)
}

fn archive_is_bios(catalog: &Catalog, archive: &str) -> bool {
    catalog
        .roms
        .get(archive)
        .is_some_and(|rom| rom.data.is_bios)
}

fn missing_bios_title(catalog: &Catalog, archive: &str) -> String {
    catalog
        .roms
        .get(archive)
        .map(|rom| {
            if rom.data.category.is_empty() {
                "BIOS image".to_string()
            } else {
                rom.data.category.clone()
            }
        })
        .unwrap_or_else(|| "BIOS image".to_string())
}

/// BIOS ROM zips policy-selected games need that are not present in the source.
pub fn missing_bios_for_report(
    catalog: &Catalog,
    args: &ExportConfig,
) -> Result<Vec<MissingBiosEntry>, Box<dyn Error>> {
    use std::collections::HashMap;
    let present = archives_in_sources(&args.source_path)?;
    let mut by_archive: HashMap<String, (String, HashSet<String>)> = HashMap::new();
    for edge in &catalog.dependencies.edges {
        if !policy_selected_game(catalog, args, &edge.game) {
            continue;
        }
        let archive = edge.archive.to_ascii_lowercase();
        if present.contains(&archive) || !archive_is_bios(catalog, &archive) {
            continue;
        }
        let title = missing_bios_title(catalog, &archive);
        by_archive
            .entry(archive)
            .or_insert_with(|| (title, HashSet::new()))
            .1
            .insert(edge.game.clone());
    }
    let mut entries = by_archive
        .into_iter()
        .map(|(archive, (title, games))| {
            let mut needed_by: Vec<_> = games.into_iter().collect();
            needed_by.sort();
            MissingBiosEntry {
                archive,
                title,
                needed_by,
            }
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.archive.cmp(&right.archive));
    Ok(entries)
}

/// Count of `.chd` files indexed under all configured source roots (ROM + optional CHD folder).
pub fn indexed_chd_file_count(args: &ExportConfig) -> Result<usize, Box<dyn Error>> {
    Ok(chd_pairs_in_sources(&args.source_path)?.len())
}

/// CHD files policy-selected games need that are not present in the source.
///
/// Includes games held back for incomplete dependencies so the plan preview matches
/// [`held_back_games`], not only launch-ready export names.
pub fn missing_chds_for_report(
    catalog: &Catalog,
    args: &ExportConfig,
) -> Result<Vec<MissingChdEntry>, Box<dyn Error>> {
    let present = chd_pairs_in_sources(&args.source_path)?;
    let mut entries = Vec::new();
    for missing in &catalog.disks.missing {
        if !policy_selected_game(catalog, args, &missing.game) {
            continue;
        }
        if source_has_chd(&present, &missing.machine, &missing.name) {
            continue;
        }
        entries.push(MissingChdEntry {
            game: missing.game.clone(),
            machine: missing.machine.clone(),
            disk: missing.name.clone(),
            reason: format!("Unresolved {}", missing_disk_kind(missing.kind)),
        });
    }
    for edge in &catalog.disks.edges {
        if !policy_selected_game(catalog, args, &edge.game) {
            continue;
        }
        if source_has_chd(&present, &edge.machine, &edge.name) {
            continue;
        }
        entries.push(MissingChdEntry {
            game: edge.game.clone(),
            machine: edge.machine.clone(),
            disk: edge.name.clone(),
            reason: required_disk_kind(edge.kind),
        });
    }
    entries.sort();
    entries.dedup();
    Ok(entries)
}

fn required_disk_kind(kind: DiskKind) -> String {
    match kind {
        DiskKind::Own => "Own disk".to_string(),
        DiskKind::Shared => "Shared disk".to_string(),
        DiskKind::DeltaParent => "Delta parent disk".to_string(),
        DiskKind::Device => "Device disk".to_string(),
    }
}

fn missing_disk_kind(kind: DiskKind) -> &'static str {
    match kind {
        DiskKind::Own => "own disk",
        DiskKind::Shared => "shared disk",
        DiskKind::DeltaParent => "delta parent",
        DiskKind::Device => "device disk",
    }
}

fn copy_retaining(
    roms: Roms,
    config: &ExportConfig,
    retain: &HashSet<String>,
    disk_machines: Option<&HashSet<String>>,
    missing_chds: Vec<MissingChdEntry>,
    export_names: Vec<String>,
    cancel: Option<&CancelFlag>,
    mut on_copy_progress: Option<&mut dyn FnMut(u64, u64)>,
) -> Result<Report, Box<dyn Error>> {
    check_export_paths(config)?;
    crate::core::paths::validate(config)?;

    let destination_paths = config.destination_folders();
    let names_path = config.selected_names_path();
    let directories = [
        destination_paths.roms.as_path(),
        destination_paths.rest.as_path(),
    ];
    let mut sources = Vec::new();
    let mut copies = Vec::new();
    for source in &config.source_path {
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
            if plan_source_entry(path, prefix, &roms, config, retain, disk_machines)
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
    let total_entries: u64 = sources
        .iter()
        .map(|(_, entries)| entries.len() as u64)
        .sum();
    let mut copy_progress = ThrottledProgress::new();
    crate::core::copying::check_targets(&copies, &directories, &config.report_path, &names_path)?;
    if !config.simulation {
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
        simulation: config.simulation,
        missing_chds,
        ..Report::default()
    };

    let mut entries_seen: u64 = 0;

    for (source_path, entries) in sources {
        let progress_bar = if config.progress {
            Some(build_progress_bar())
        } else {
            None
        };

        let operation = if config.simulation {
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
            check_cancelled(cancel)?;
            entries_seen += 1;
            if let Some(callback) = on_copy_progress.as_deref_mut() {
                copy_progress.maybe_emit(entries_seen, total_entries, callback);
            }
            let (file_prefix, file_name) = (
                path.file_stem().unwrap().to_str().unwrap(),
                path.file_name().unwrap(),
            );

            progress_bar.inc();

            match plan_source_entry(&path, file_prefix, &roms, config, retain, disk_machines) {
                EntryPlan::Skip => continue,
                EntryPlan::Copy => {
                    let key = file_prefix.to_ascii_lowercase();
                    let machine_has_disks =
                        roms.get(&key).is_some_and(|rom| !rom.data.chd.is_empty());
                    let disk = is_disk_media(&path, machine_has_disks);
                    let required = file_is_required(disk, &key, retain, disk_machines);
                    let destination =
                        export_directory(required, &destination_paths).join(file_name);

                    let copy_error = copy_source_entry(&path, &destination, config)
                        .err()
                        .map(|error| error.to_string());
                    let status = if copy_error.is_some() {
                        OperationStatus::Failed
                    } else if config.simulation {
                        OperationStatus::Planned
                    } else {
                        OperationStatus::Copied
                    };
                    if let Some(error) = &copy_error {
                        error!("{error}");
                        something_failed = true;
                    }

                    let media_kind = if disk {
                        ExportMediaKind::DiskDirectory
                    } else if path.is_dir() {
                        ExportMediaKind::OtherFile
                    } else {
                        ExportMediaKind::Archive
                    };

                    let report_detail_entry = ReportDetailEntry {
                        rom_name: file_name.to_string_lossy().into_owned(),
                        status,
                        media_kind,
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

    check_cancelled(cancel)?;

    if !config.simulation && !something_failed {
        if let Err(error) = write_selected_names(&export_names, &names_path) {
            error!("{error}");
            something_failed = true;
            report.names_error = Some(error.to_string());
        }
    }

    report
        .total_working(total_working)
        .total_other(total_other)
        .all_ok(!something_failed);
    report.selected_games = export_names.len();

    Ok(report)
}

/// Machine names whose CHD directory a selected game needs.
///
/// The range limits selected games. A parent directory that stores a required
/// shared or delta disk stays in `roms` even when its name falls outside the range.
/// Other disk directories go to `rest` when this run copies them.
pub(crate) fn required_disk_machines(
    disks: &DiskSet,
    args: &ExportConfig,
    export_games: &HashSet<String>,
) -> HashSet<String> {
    // The flag remains accepted so existing commands parse. It does not choose a folder.
    let _retired = args.ignore_not_working_chd;
    disks
        .edges
        .iter()
        .filter(|edge| game_exports(&edge.game, export_games, args))
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
    args: &ExportConfig,
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

fn write_selected_names(export_names: &[String], path: &Path) -> Result<(), Box<dyn Error>> {
    let mut names = export_names.to_vec();
    names.sort();
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
pub(crate) fn retained_archives(
    dependencies: &DependencySet,
    args: &ExportConfig,
    export_games: &HashSet<String>,
) -> HashSet<String> {
    dependencies
        .edges
        .iter()
        .filter(|edge| game_exports(&edge.game, export_games, args))
        .map(|edge| edge.archive.to_ascii_lowercase())
        .collect()
}

pub(crate) fn subset_excludes(
    config: &ExportConfig,
    file_prefix: &str,
    retain: &HashSet<String>,
) -> bool {
    name_in_subset_excluded(config, file_prefix)
        && !retain.contains(&file_prefix.to_ascii_lowercase())
}
