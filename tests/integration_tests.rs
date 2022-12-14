use std::collections::HashSet;
use std::fs::{File, read_dir};
use std::io::{BufRead, BufReader};
use std::iter::Filter;
use std::path::Path;
use std::string::ToString;

use rand::{distributions::Alphanumeric, Rng};

use roms_curator::core::roms_service::RomsExt;
use roms_curator::models::config::Config;
use roms_curator::models::roms::RomCategory::Working;
use roms_curator::models::roms::Roms;

use crate::utils::{clean_up, set_up};

mod utils;

static TARGET_FOLDER: &str = "target/tests/";
static MAME_XML_FILE_NAME: &str = "mame-roms.xml";
static CATEGORY_LIST_FILE_NAME: &str = "catver.ini";
static WORKING_ARCADE_LIST_PATH: &str = "tests/resources/working_arcade_0244.ini";
static ROMS_SOURCE_PATH: &str = "tests/resources/merged_roms/";
static CATEGORIZED_ROMS_FOLDER_NAME: &str = "categorized_roms";
static CATEGORIZED_WORKING_FOLDER_NAME: &str = "working";
static CATEGORIZED_OTHER_FOLDER_NAME: &str = "other";
static CATEGORIZED_CHD_OTHER_FOLDER_NAME: &str = "chd_other";

#[test]
fn should_build_set_with_no_errors() {
    let tag = get_test_tag();
    set_up(&tag);

    let config = build_config(&tag, false);

    let result = roms_curator::run(&config);

    let success = match result {
        Ok(..) => true,
        Err(err) => {
            println!("{:?}", err);
            false
        }
    };

    assert!(success);

    clean_up(&tag);
}

#[test]
fn should_separate_all_working_roms() {
    let tag = get_test_tag();
    set_up(&tag);

    let config = build_config(&tag, false);

    let results = roms_curator::run_debug(&config).unwrap();

    // get working from results
    let working: Roms = Filter::collect(
        results.into_iter()
            .filter(|(_, rom)| matches!(rom.category, Working))
    );
    let working_names: HashSet<String> = working.keys().cloned().collect();

    // get working from mame ini files
    let working_arcade: HashSet<String> = BufReader::new(
        File::open(WORKING_ARCADE_LIST_PATH).expect("File not found")
    )
        .lines().skip(8)
        .map(|line| line.expect("Could not parse line"))
        .collect();

    // roms NOT in working set but in `working_arcade` set
    let included_in_working_arcade: Vec<_> = working_arcade.difference(&working_names).collect();
    // roms in good set but not in `working_arcade` set
    let not_included_in_working_arcade: Vec<_> = working_names.difference(&working_arcade).collect();

    let not_included_in_working_arcade_len = not_included_in_working_arcade.len();
    let included_in_working_arcade_len = included_in_working_arcade.len();

    // debug_roms_set_diff(included_in_working_arcade, not_included_in_working_arcade);

    // difference between sets was manually validated
    assert_eq!(not_included_in_working_arcade_len, 9);
    // These include roms that:
    // - don't actually work
    // - are mechanical and/or casino games
    // - ...
    assert_eq!(included_in_working_arcade_len, 1621);

    clean_up(&tag);
}

#[test]
fn should_copy_files_to_destination_folder_and_create_report() {
    let tag = get_test_tag();
    set_up(&tag);

    let config = build_config(&tag, false);

    let results = roms_curator::run_debug(&config).unwrap();

    let report = results.copy_roms(&config).expect("Error copying roms");

    assert_eq!(report.total_working, 2);
    assert_eq!(report.total_other, 4);

    // validate roms are in the correct place
    fn get_files_from_folder(path: &str) -> Vec<String> {
        let mut roms: Vec<String> = Vec::new();
        for entry in read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            let file_name = path.file_name().unwrap().to_str().unwrap();
            roms.push(file_name.to_string());
        }
        roms
    }

    let test_folder = Path::new(TARGET_FOLDER).join(&tag);

    let path = test_folder.join(CATEGORIZED_ROMS_FOLDER_NAME).join(CATEGORIZED_WORKING_FOLDER_NAME);
    let mut working_roms = get_files_from_folder(path.to_str().unwrap());
    let mut expected = vec!(
        "elevatora.zip".to_string(),
        "robocop.zip".to_string(),
    );
    working_roms.sort();
    expected.sort();
    assert_eq!(working_roms, expected);

    let path = test_folder.join(CATEGORIZED_ROMS_FOLDER_NAME).join(CATEGORIZED_OTHER_FOLDER_NAME);
    let mut other_roms = get_files_from_folder(path.to_str().unwrap());
    let mut expected = vec!(
        "a24play.zip".to_string(), // system
        "3dobios.zip".to_string(), // bios
        "sv801.zip".to_string(), // device
    );
    other_roms.sort();
    expected.sort();
    assert_eq!(other_roms, expected);

    let path = test_folder.join(CATEGORIZED_ROMS_FOLDER_NAME).join(CATEGORIZED_CHD_OTHER_FOLDER_NAME);
    let mut chd_other_roms = get_files_from_folder(path.to_str().unwrap());
    let mut expected = vec!(
        "99bottles.zip".to_string(), // chd
    );
    chd_other_roms.sort();
    expected.sort();
    assert_eq!(chd_other_roms, expected);

    assert!(matches!(report.to_file(config.report_path.as_str()), Ok(true)));

    clean_up(&tag);
}

#[test]
fn simulation_should_generate_report_but_not_copy_roms() {
    let tag = get_test_tag();
    set_up(&tag);

    let config = build_config(&tag, true);

    let results = roms_curator::run_debug(&config).unwrap();

    let report = results.copy_roms(&config).expect("Error copying roms");

    assert_eq!(report.total_working, 2);
    assert_eq!(report.total_other, 4);

    let test_folder = Path::new(TARGET_FOLDER).join(&tag);

    let path = test_folder.join(CATEGORIZED_ROMS_FOLDER_NAME).join(CATEGORIZED_WORKING_FOLDER_NAME);
    let working_roms = get_files_from_folder(path.to_str().unwrap());
    assert!(working_roms.is_empty());

    let path = test_folder.join(CATEGORIZED_ROMS_FOLDER_NAME).join(CATEGORIZED_OTHER_FOLDER_NAME);
    let other_roms = get_files_from_folder(path.to_str().unwrap());
    assert!(other_roms.is_empty());

    let path = test_folder.join(CATEGORIZED_ROMS_FOLDER_NAME).join(CATEGORIZED_CHD_OTHER_FOLDER_NAME);
    let chd_other_roms = get_files_from_folder(path.to_str().unwrap());
    assert!(chd_other_roms.is_empty());

    assert!(matches!(report.to_file(config.report_path.as_str()), Ok(true)));

    clean_up(&tag);
}

fn get_files_from_folder(path: &str) -> Vec<String> {
    let mut roms: Vec<String> = Vec::new();
    for entry in read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        let file_name = path.file_name().unwrap().to_str().unwrap();
        roms.push(file_name.to_string());
    }
    roms
}

fn build_config(tag: &str, simulate: bool) -> Config {
    let test_folder = Path::new(TARGET_FOLDER).join(tag);
    let mame_xml_path = test_folder.join(MAME_XML_FILE_NAME).to_str().unwrap().to_string();
    let catver_path = test_folder.join(CATEGORY_LIST_FILE_NAME).to_str().unwrap().to_string();
    let destination_path = test_folder.join(CATEGORIZED_ROMS_FOLDER_NAME).to_str().unwrap().to_string();
    let report_path = test_folder.join("report.md").to_str().unwrap().to_string();
    let ignore_not_working_chd = false;
    let simulation = simulate;

    Config {
        mame_xml_path,
        catver_path,
        source_path: vec![ROMS_SOURCE_PATH.to_string()],
        destination_path,
        report_path,
        ignore_not_working_chd,
        simulation,
    }
}

#[allow(dead_code)]
fn debug_roms_set_diff(
    mut included_in_working_arcade: Vec<&String>,
    mut not_included_in_working_arcade: Vec<&String>,
) {
    not_included_in_working_arcade.sort();
    included_in_working_arcade.sort();
    println!("Roms found in one our collection but not in 'working_arcade.ini' collection");
    println!("Nr of roms: {:?}, roms: {:?}", not_included_in_working_arcade.len(), not_included_in_working_arcade);
    println!();
    println!("Roms NOT found in one our collection but found in 'working_arcade.ini' collection");
    println!("Nr of roms: {:?}, roms: {:?}", included_in_working_arcade.len(), included_in_working_arcade);
}

fn get_test_tag() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(4)
        .map(char::from)
        .collect()
}
