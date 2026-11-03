//! # roms-curator
//!
//! `roms-curator`, at the moment, is a utility to help and sort MAME ROMs Sets.
//! For example to create a working only ROM Set. It separates bios/working/not-working/other
//! roms into sub-folders, so that it can, more easily, be added to frontends without filling
//! the collection with stuff you don't want.
//!
//! See [documentation](https://github.com/HugoTigre/roms-curator) for more details
//! on how to use this as a command-line Application or as a Library.
//!

use crate::core::args::Args;
use crate::core::roms_service::parse;
use crate::models::roms::Catalog;
use crate::utils::{build_progress_bar, ProgressBarEx};
use roxmltree::Document;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::Path;

pub mod core;
pub mod models;
pub mod utils;

type RomCategories = HashMap<String, String>;

///
/// Reads both MAME ROM database and MAME support file
/// from [Config](Config) `mame_xml_path`
/// and `catver_ini_path` and creates a in memory ROM collection
/// with all roms categorized according to the version of the
/// files provided. This collection can then be used to copy
/// only the intended roms creating a new curated ROM collection.
/// See [copy_roms](core::roms_service::RomsExt::copy_roms).
///
/// # Examples
///
/// ```no_run
/// # use std::{env, process};
/// # use log::error;
/// use roms_curator::core::args::build_args;
///
/// // build args from command line arguments
/// let args = build_args().unwrap_or_else(|err| {
///     error!("Application error: {err}");
///     process::exit(1);
///  });
///
/// let roms = roms_curator::run(&args).unwrap_or_else(|err| {
///     error!("Application error: {err}");
///     process::exit(1);
/// });
/// ```
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
/// that fall outside the range.
///
pub fn run(args: &Args) -> Result<Catalog, Box<dyn Error>> {
    let progress_bar = if args.progress {
        Some(build_progress_bar())
    } else {
        None
    };
    progress_bar.set_length(3);

    progress_bar.println(
        "* Reading mame database and copying files can last a few minutes, please be patient. *",
    );

    let catver_path = Path::new(&args.catver_path);
    let xml_path = Path::new(&args.mame_xml_path);
    progress_bar.println(format!("Reading {} document...", catver_path.display()).as_str());
    let rom_categories = read_categories(catver_path)?;
    progress_bar.inc();

    progress_bar.println(format!("Reading {} document...", xml_path.display()).as_str());
    let contents = fs::read_to_string(xml_path)
        .map_err(|error| format!("Cannot read MAME XML {}: {error}", xml_path.display()))?;
    let doc = read_mame_xml(&contents)
        .map_err(|error| format!("Cannot parse MAME XML {}: {error}", xml_path.display()))?;
    progress_bar.inc();

    progress_bar.println("Categorizing roms...");
    let catalog = parse(&doc, &rom_categories, xml_path)?.categorize()?;
    progress_bar.println(
        format!(
            "MAME build {}: {} machines, {} without a CatVer category.",
            catalog.mame_build,
            catalog.roms.len(),
            catalog.unknown_categories.len()
        )
        .as_str(),
    );
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

fn read_mame_xml(contents: &str) -> Result<Document<'_>, Box<dyn Error>> {
    let opt = roxmltree::ParsingOptions {
        allow_dtd: true,
        nodes_limit: u32::MAX,
    };
    let doc = Document::parse_with_options(contents, opt)?;
    Ok(doc)
}
