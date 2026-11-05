//! # roms-curator
//!
//! `roms-curator` exports a merged MAME collection into two folders.
//! `roms` holds every file a selected game needs. `rest` holds every other source
//! file, so the original collection can be removed. `selected.txt` lists the
//! selected machine names for a frontend.
//!
//! See [documentation](https://github.com/HugoTigre/roms-curator) for more details
//! on how to use this as a command-line Application or as a Library.
//!

use crate::core::roms_service::parse_mame_xml_file;
use crate::models::roms::Catalog;
use crate::models::selection::EmulationPolicy;
use crate::utils::{build_progress_bar, ProgressBarEx};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::Path;

pub mod cancel;
pub mod core;
pub mod models;
pub mod utils;

pub use crate::cancel::CancelFlag;
pub use crate::core::config::ExportConfig;
pub use crate::core::form_check::{check_export_form, ExportFormIssues};
pub use crate::core::metadata_fetch::{
    default_metadata_cache_dir, ensure_metadata, parse_mame_version, MameMetadataPaths, MameVersion,
};
pub use crate::core::roms_service::{planned_media_for_roms, REST_MEDIA_REASON};

type RomCategories = HashMap<String, String>;

///
/// Reads both MAME ROM database and MAME support file
/// from [Config](Config) `mame_xml_path`
/// and `catver_ini_path` and creates a in memory ROM collection
/// with all roms categorized according to the version of the
/// files provided. This collection can then be used to copy
/// only the intended roms creating a new curated ROM collection.
/// See [`Catalog::copy_roms`].
///
/// # Examples
///
/// ```no_run
/// # use std::{env, process};
/// # use log::error;
/// use roms_curator::{ExportConfig, run};
///
/// let config = ExportConfig {
///     mame_xml_path: "mame.xml".into(),
///     catver_path: "catver.ini".into(),
///     ..ExportConfig::default()
/// };
///
/// let catalog = run(&config).unwrap_or_else(|err| {
///     error!("Application error: {err}");
///     process::exit(1);
/// });
/// ```
///
/// Call [`ExportConfig::validate`] before [`Catalog::plan`] or [`Catalog::copy_roms`];
/// it requires source and destination paths in addition to metadata paths.
///
/// The returned catalog includes the MAME build and the names of machines that
/// have no CatVer category. Those machines use the category `Unknown`.
///
/// The default selection is the playable policy: driver status and emulation
/// are each good or imperfect. Imperfect or unemulated sound and graphics
/// features stay selected. Preliminary machines, mechanical machines, BIOS,
/// devices, and non-arcade CatVer groups do not. `catalog.selected` is that
/// visible game list. BIOS, devices, and other unselected machines stay in
/// `catalog.roms`, and a clone stays selected when its parent does not.
/// Parent, device, ROM, and disk relationships are kept on each machine.
/// `catalog.dependencies` names the merged archives those selected games need:
/// a clone requires its parent archive, and BIOS and device archives stay separate.
/// Missing names and cycles are listed there. `catalog.disks` names the CHD
/// files those games need, including one shared ancestor copy and the parent's
/// same-named non-optional disk when a clone disk may be a delta. A differently
/// named or optional parent disk is missing when that parent has disks. A parent
/// with no disks is not missing. Optional disks are omitted. Unresolved
/// disk references are `missing` and do not change the game's category.
/// A name-range subset still copies required archives and disk directories
/// that fall outside the range, and leaves other out-of-range names in the
/// source. `Catalog::plan` returns a version-2 plan of that copy: selection
/// reasons, `roms` and `rest` paths and sizes, missing media, and destination
/// conflicts. `Plan::write` or `--plan-path` saves it. `copy_with_plan` rechecks
/// the plan it is given, then copies and writes `selected.txt` when every copy succeeds.
///
pub fn run(config: &ExportConfig) -> Result<Catalog, Box<dyn Error>> {
    run_with_progress(config, None::<&mut dyn FnMut(&str)>, None)
}

/// Same as [`run`], but invokes `on_status` with human-readable step labels when
/// provided (for example a GUI status line while `progress` is false).
///
/// `cancel` is polled after the opening status line, after the CatVer file is
/// read, and after the MAME XML is parsed, before categorization. Metadata
/// download is not part of this function.
pub fn run_with_progress(
    config: &ExportConfig,
    mut on_status: Option<&mut dyn FnMut(&str)>,
    cancel: Option<&CancelFlag>,
) -> Result<Catalog, Box<dyn Error>> {
    use crate::cancel::check_cancelled;
    let progress_bar = if config.progress {
        Some(build_progress_bar())
    } else {
        None
    };
    progress_bar.set_length(3);

    let catalog_load_message = if on_status.is_some() {
        "Loading MAME catalog…"
    } else {
        "* Reading mame database and copying files can last a few minutes, please be patient. *"
    };

    let mut status = |message: &str| {
        progress_bar.println(message);
        if let Some(callback) = on_status.as_deref_mut() {
            callback(message);
        }
    };

    status(catalog_load_message);
    check_cancelled(cancel)?;

    let catver_path = Path::new(&config.catver_path);
    let xml_path = Path::new(&config.mame_xml_path);
    status(&format!("Reading {} document...", catver_path.display()));
    let rom_categories = read_categories(catver_path)?;
    progress_bar.inc();
    check_cancelled(cancel)?;

    status(&format!("Reading {} document...", xml_path.display()));
    let parsed = parse_mame_xml_file(xml_path, &rom_categories)?;
    progress_bar.inc();
    check_cancelled(cancel)?;

    status("Categorizing roms...");
    let catalog = if config.strict_emulation {
        parsed.categorize_with(EmulationPolicy::Strict)?
    } else {
        parsed.categorize()?
    };
    status(&format!(
        "MAME build {}: {} machines, {} without a CatVer category.",
        catalog.mame_build,
        catalog.roms.len(),
        catalog.unknown_categories.len()
    ));
    progress_bar.inc();

    if let Some(pb) = progress_bar {
        pb.finish();
    }

    Ok(catalog)
}

pub(crate) fn read_categories(path: &Path) -> Result<RomCategories, Box<dyn Error>> {
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("Cannot read CatVer categories {}: {error}", path.display()))?;
    parse_categories(&contents, path)
}

pub(crate) fn parse_categories(
    contents: &str,
    path: &Path,
) -> Result<RomCategories, Box<dyn Error>> {
    let mut categories = HashMap::new();
    let mut in_category = false;
    let mut saw_category = false;

    for (index, raw) in contents.lines().enumerate() {
        let line_no = index + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if let Some(section) = section_name(line) {
            in_category = section == "Category";
            if in_category {
                saw_category = true;
            }
            continue;
        }
        if !in_category {
            continue;
        }
        let Some((name, category)) = line.split_once('=') else {
            return Err(format!(
                "{}:{line_no}: expected name=category in [Category]",
                path.display()
            )
            .into());
        };
        let name = name.trim();
        let category = category.trim();
        if name.is_empty() || category.is_empty() {
            return Err(format!(
                "{}:{line_no}: category entry needs a name and a category",
                path.display()
            )
            .into());
        }
        categories.insert(name.to_string(), category.to_string());
    }

    if !saw_category {
        return Err(format!("{}: missing [Category] section", path.display()).into());
    }
    Ok(categories)
}

fn section_name(line: &str) -> Option<&str> {
    let line = line.trim();
    line.strip_prefix('[')
        .and_then(|line| line.strip_suffix(']'))
        .map(str::trim)
        .filter(|section| !section.is_empty())
}
