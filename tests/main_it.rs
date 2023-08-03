use std::fs::File;
use std::path::Path;
use assert_cmd::Command;
use predicates::prelude::predicate;
use crate::utils::{CATEGORIZED_ROMS_FOLDER_NAME, CATEGORY_LIST_FILE_NAME_SMALL_SET, get_test_tag, MAME_XML_FILE_NAME_SMALL_SET, ROMS_SOURCE_PATH, set_up, TARGET_FOLDER};

mod utils;

#[test]
fn should_run_successfully() {
    let tag = get_test_tag();
    set_up(&tag);

    let mut cmd = Command::cargo_bin("roms-curator").unwrap();

    let test_folder = Path::new(TARGET_FOLDER).join(&tag);
    let destination_path = test_folder.join(CATEGORIZED_ROMS_FOLDER_NAME).to_str().unwrap().to_string();
    let report_path = test_folder.join("report.md").to_str().unwrap().to_string();

    let assert = cmd
        .arg(format!("--mame-xml-path={}", MAME_XML_FILE_NAME_SMALL_SET))
        .arg(format!("--catver-path={}", CATEGORY_LIST_FILE_NAME_SMALL_SET))
        .arg(format!("--source-path={}", ROMS_SOURCE_PATH))
        .arg(format!("--destination-path={}", destination_path))
        .arg(format!("--report-path={}", report_path))
        .arg("--simulation=true")
        .arg("--progress=false")
        .assert();

    assert.success()
        .stdout(predicate::str::contains("Categorizing roms"))
        .stdout(predicate::str::contains("Copying from source"));
}

#[test]
fn should_validate_missing_mame_xml_path() {
    set_up(&get_test_tag());
    let mut cmd = Command::cargo_bin("roms-curator").unwrap();

    // should show help
    let assert = cmd.assert();

    assert.stderr(predicate::str::contains(
        "Helper utility to manage Sets of ROMs. Currently only works for MAME ROMs"
    ));

    // should warn that file needs to be a .xml
    let assert = cmd.arg("--mame-xml-path=some-invalid-path").assert();

    assert.stderr(predicate::str::contains(
        "File needs to be a XML file, for ex, mame.xml."
    ));

    // should warn that file was not found
    let mut cmd = Command::cargo_bin("roms-curator").unwrap();
    let assert = cmd.arg("--mame-xml-path=some-valid-path.xml").assert();

    assert.stderr(predicate::str::contains(
        "File not found."
    ));
}

#[test]
fn should_validate_missing_catver_path() {
    let tag = get_test_tag();
    set_up(&tag);
    let mut cmd = Command::cargo_bin("roms-curator").unwrap();

    let (mame_xml_file, _) = create_mame_and_catver_files_to_bypass_file_not_found_error(&tag);

    let assert = cmd.arg(format!("--mame-xml-path={}", mame_xml_file)).assert();

    // should want about the other missing mandatory arguments
    assert
        .stderr(predicate::str::contains("the following required arguments were not provided:"))
        .stderr(predicate::str::contains("--catver-path <catver_path>"))
        .stderr(predicate::str::contains("--source-path <source_path>"))
        .stderr(predicate::str::contains("--destination-path <destination_path>"));

    let assert = cmd.arg("--catver-path=some-invalid-path").assert();

    assert.stderr(predicate::str::contains(
        "File needs to be a ini file, for ex, catver.ini"
    ));
}

#[test]
fn should_parse_source_paths() {
    let tag = get_test_tag();
    set_up(&tag);
    let mut cmd = Command::cargo_bin("roms-curator").unwrap();

    let (mame_xml_file, carver_init_file) =
        create_mame_and_catver_files_to_bypass_file_not_found_error(&tag);

    let fs_sp = std::path::MAIN_SEPARATOR;

    let assert = cmd
        .arg(format!("--mame-xml-path={}", mame_xml_file))
        .arg(format!("--catver-path={}", carver_init_file))
        .arg(format!("--source-path=.{},target{},target", fs_sp, fs_sp))
        .arg(format!("--destination-path={}", fs_sp))
        .assert();

    let expected = if fs_sp == '/' {
        format!("source_path: [\".{}\", \"target{}\", \"target\"]", fs_sp, fs_sp)
    } else {
        format!("source_path: [\".{}{}\", \"target{}{}\", \"target\"]", fs_sp, fs_sp, fs_sp, fs_sp)
    };

    assert.stdout(predicate::str::contains(expected));
}

#[test]
fn source_path_and_destination_path_cannot_be_equal() {
    let tag = get_test_tag();
    set_up(&tag);
    let mut cmd = Command::cargo_bin("roms-curator").unwrap();

    let (mame_xml_file, carver_init_file) =
        create_mame_and_catver_files_to_bypass_file_not_found_error(&tag);

    let assert = cmd
        .arg(format!("--mame-xml-path={}", mame_xml_file))
        .arg(format!("--catver-path={}", carver_init_file))
        .arg("--source-path=target/")
        .arg("--destination-path=target/")
        .assert();

    assert.stdout(predicate::str::contains(
        "[source-path] and [destination-path] cannot be the same"
    ));
}

#[test]
fn simulation_mode_needs_report_path() {
    let tag = get_test_tag();
    set_up(&tag);
    let mut cmd = Command::cargo_bin("roms-curator").unwrap();

    let (mame_xml_file, carver_init_file) =
        create_mame_and_catver_files_to_bypass_file_not_found_error(&tag);

    let assert = cmd
        .arg(format!("--mame-xml-path={}", mame_xml_file))
        .arg(format!("--catver-path={}", carver_init_file))
        .arg("--source-path=target/")
        .arg("--destination-path=/")
        .arg("--simulation=true")
        .assert();

    assert.stdout(predicate::str::contains(
        "Simulation mode requires a report file"
    ));
}

fn create_mame_and_catver_files_to_bypass_file_not_found_error(test_tag: &str) -> (String, String) {
    let test_folder = Path::new(TARGET_FOLDER).join(test_tag);
    let mame_xml_file = test_folder.join("some-valid-path.xml").to_str().unwrap().to_string();
    let catver_init_file = test_folder.join("some-valid-path.ini").to_str().unwrap().to_string();
    File::create(&mame_xml_file).unwrap();
    File::create(&catver_init_file).unwrap();
    (mame_xml_file, catver_init_file)
}
