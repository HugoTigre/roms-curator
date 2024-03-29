use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::iter::Filter;
use std::path::Path;
use std::string::ToString;

use roms_curator::core::roms_service::RomsExt;
use roms_curator::models::roms::RomCategory::Working;
use roms_curator::models::roms::Roms;

use crate::utils::{CATEGORIZED_CHD_OTHER_FOLDER_NAME, CATEGORIZED_CHD_WORKING_FOLDER_NAME, CATEGORIZED_OTHER_FOLDER_NAME, CATEGORIZED_ROMS_FOLDER_NAME, CATEGORIZED_WORKING_FOLDER_NAME, clean_up, get_files_from_folder, get_test_tag, run_expensive_tests, set_up, TARGET_FOLDER, WORKING_ARCADE_LIST_PATH};

mod utils;

#[test]
fn should_build_set_with_no_errors() {
    let tag = get_test_tag();
    set_up(&tag);

    let args = utils::build_args(
        &tag, false, String::new(), String::new(),
    );

    let result = roms_curator::run(&args);

    let success = match result {
        Ok(..) => true,
        Err(err) => {
            println!("{err:?}");
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

    let args = utils::build_args(
        &tag, false, String::new(), String::new(),
    );

    let results = roms_curator::run(&args).unwrap();

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
    let not_included_in_working_arcade_len = working_names.difference(&working_arcade).count();
    // roms in good set but not in `working_arcade` set
    let included_in_working_arcade_len = working_arcade.difference(&working_names).count();

    // debug_roms_set_diff(included_in_working_arcade, not_included_in_working_arcade);

    // difference between sets was manually validated
    let notincluded_in_working_expected = if run_expensive_tests() { 9 } else { 0 };
    assert_eq!(not_included_in_working_arcade_len, notincluded_in_working_expected);

    // These include roms that:
    // - don't actually work
    // - are mechanical and/or casino games
    // - ...
    let included_in_working_expected = if run_expensive_tests() { 1621 } else { 11796 };
    assert_eq!(included_in_working_arcade_len, included_in_working_expected);

    clean_up(&tag);
}

#[test]
fn should_copy_files_to_destination_folder_and_create_report() {
    let tag = get_test_tag();
    set_up(&tag);

    let args = utils::build_args(
        &tag, false, String::new(), String::new(),
    );

    let results = roms_curator::run(&args).unwrap();

    let report = results.copy_roms(&args).expect("Error copying roms");

    assert_eq!(report.total_working, 5);
    assert_eq!(report.total_other, 7);

    let test_folder = Path::new(TARGET_FOLDER).join(&tag);

    let path = test_folder.join(CATEGORIZED_ROMS_FOLDER_NAME).join(CATEGORIZED_WORKING_FOLDER_NAME);
    let mut working_roms = get_files_from_folder(path.to_str().unwrap());
    let mut expected = vec!(
        "005.zip".to_string(),
        "elevatora.zip".to_string(),
        "robocop.zip".to_string(),
    );
    working_roms.sort();
    expected.sort();
    assert_eq!(working_roms, expected);

    let path = test_folder.join(CATEGORIZED_ROMS_FOLDER_NAME).join(CATEGORIZED_OTHER_FOLDER_NAME);
    let mut other_roms = get_files_from_folder(path.to_str().unwrap());
    let mut expected = vec!(
        "100lions.zip".to_string(),
        "3dobios.zip".to_string(), // bios
        "a24play.zip".to_string(), // system
        "aristmk6.zip".to_string(),
        "as_acp.zip".to_string(),
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

    let path = test_folder.join(CATEGORIZED_ROMS_FOLDER_NAME).join(CATEGORIZED_CHD_WORKING_FOLDER_NAME);
    let mut chd_working_roms = get_files_from_folder(path.to_str().unwrap());
    let mut expected = vec!(
        "Area51".to_string(), // chd
        "area51.zip".to_string(), // rom that depends on chd
    );
    chd_working_roms.sort();
    expected.sort();
    assert_eq!(chd_working_roms, expected);

    assert!(matches!(report.to_file(args.report_path.as_str()), Ok(true)));

    clean_up(&tag);
}

#[test]
fn simulation_should_generate_report_but_not_copy_roms() {
    let tag = get_test_tag();
    set_up(&tag);

    let args = utils::build_args(
        &tag, true, String::new(), String::new(),
    );

    let results = roms_curator::run(&args).unwrap();

    let report = results.copy_roms(&args).expect("Error copying roms");

    assert_eq!(report.total_working, 5);
    assert_eq!(report.total_other, 7);

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

    let path = test_folder.join(CATEGORIZED_ROMS_FOLDER_NAME).join(CATEGORIZED_CHD_WORKING_FOLDER_NAME);
    let chd_working_roms = get_files_from_folder(path.to_str().unwrap());
    assert!(chd_working_roms.is_empty());

    assert!(matches!(report.to_file(args.report_path.as_str()), Ok(true)));

    clean_up(&tag);
}

#[test]
fn should_copy_files_to_destination_folder_excluding_subsets() {
    let tag = get_test_tag();
    set_up(&tag);

    let args = utils::build_args(
        &tag, false, "r".to_string(), "sa".to_string(),
    );

    let results = roms_curator::run(&args).unwrap();

    let report = results.copy_roms(&args).expect("Error copying roms");

    assert_eq!(report.total_working, 1);
    assert_eq!(report.total_other, 0);

    let test_folder = Path::new(TARGET_FOLDER).join(&tag);

    let path = test_folder.join(CATEGORIZED_ROMS_FOLDER_NAME).join(CATEGORIZED_WORKING_FOLDER_NAME);
    let mut working_roms = get_files_from_folder(path.to_str().unwrap());
    let mut expected = vec!(
        "robocop.zip".to_string(),
    );
    working_roms.sort();
    expected.sort();
    assert_eq!(working_roms, expected);

    let expected: Vec<String> = Vec::new();

    let path = test_folder.join(CATEGORIZED_ROMS_FOLDER_NAME).join(CATEGORIZED_OTHER_FOLDER_NAME);
    let other_roms = get_files_from_folder(path.to_str().unwrap());
    assert_eq!(other_roms, expected);

    let path = test_folder.join(CATEGORIZED_ROMS_FOLDER_NAME).join(CATEGORIZED_CHD_OTHER_FOLDER_NAME);
    let chd_other_roms = get_files_from_folder(path.to_str().unwrap());
    assert_eq!(chd_other_roms, expected);

    let path = test_folder.join(CATEGORIZED_ROMS_FOLDER_NAME).join(CATEGORIZED_CHD_WORKING_FOLDER_NAME);
    let chd_working_roms = get_files_from_folder(path.to_str().unwrap());
    assert_eq!(chd_working_roms, expected);

    assert!(matches!(report.to_file(args.report_path.as_str()), Ok(true)));

    clean_up(&tag);
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
