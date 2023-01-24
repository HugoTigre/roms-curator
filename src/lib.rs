//! # roms-curator
//!
//! `roms-curator`, at the moment, is a utility to help and sort mame roms.
//! It separates bios/working/not-working/other roms into subfolders, so that it can,
//! more easily, be added to frontends without filling the collection with
//! stuff you don't want.

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

pub fn run(config: &Config) -> Result<Roms, Box<dyn Error>> {
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
