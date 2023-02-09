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

use std::error::Error;
use std::fs;
use std::collections::HashMap;
use log::{info};
use roxmltree::Document;
use crate::core::roms_service::{UnfilteredRomsExt, parse};
use crate::models::config::Config;
use crate::models::roms::Roms;

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
/// use roms_curator::models::config::Config;
///
/// # let config = Config::default();
/// let config = Config::new().build(env::args()).unwrap_or_else(|err| {
///     error!("Problem parsing arguments: {err}");
///     process::exit(1);
/// });
///
/// let roms = roms_curator::run(&config).unwrap_or_else(|err| {
///     error!("Application error: {err}");
///     process::exit(1);
/// });
/// ```
///
/// @return A `HashMap<String, Rom>` collection with all ROMs categorized.
///
pub fn run(config: &Config) -> Result<Roms, Box<dyn Error>> {
    info!("* Reading mame database and copying files can last a few minutes, please be patient. *");

    info!("Reading {} document...", &config.catver_path);
    let rom_categories = build_category_list(config.catver_path.clone())?;

    info!("Reading {} document...", &config.mame_xml_path);
    let contents = fs::read_to_string(config.mame_xml_path.clone())?;
    let doc = read_mame_xml(&contents)?;

    info!("Categorizing roms...");
    let unfiltered_roms = parse(doc, rom_categories)?;
    let roms = unfiltered_roms.categorize_roms()?;

    Ok(roms)
}

fn build_category_list(file_path: String) -> Result<RomCategories, Box<dyn Error>> {
    let category_contents = fs::read_to_string(file_path)?;

    let mut rom_category: HashMap<String, String> = HashMap::new();

    for line in category_contents.lines() {
        if line == "[VerAdded]" { break; };
        if line.is_empty() { continue; };

        let mut name_and_category = line.split('=');
        let name = name_and_category.next().unwrap_or_default();
        let category = name_and_category.next().unwrap_or_default();

        if name.is_empty() || category.is_empty() { continue; };

        rom_category.insert(name.to_string(), category.to_string());
    }

    Ok(rom_category)
}

fn read_mame_xml(contents: &str) -> Result<Document, Box<dyn Error>> {
    let opt = roxmltree::ParsingOptions { allow_dtd: true, nodes_limit: u32::MAX };
    let doc = Document::parse_with_options(contents, opt)?;
    Ok(doc)
}
