use crate::utils::{
    get_test_tag, set_up, CATEGORIZED_ROMS_FOLDER_NAME, CATVER_SAMPLE, MAME_XML_SAMPLE,
    ROMS_SOURCE_PATH, TARGET_FOLDER,
};
use assert_cmd::Command;
use predicates::prelude::predicate;
use std::fs::File;
use std::path::Path;

mod utils;

#[test]
fn should_run_successfully() {
    let tag = get_test_tag();
    set_up(&tag);

    let mut cmd = Command::cargo_bin("roms-curator").unwrap();

    let test_folder = Path::new(TARGET_FOLDER).join(&tag);
    let destination_path = test_folder
        .join(CATEGORIZED_ROMS_FOLDER_NAME)
        .to_str()
        .unwrap()
        .to_string();
    let report_path = test_folder.join("report.md").to_str().unwrap().to_string();

    let assert = cmd
        .arg(format!("--mame-xml-path={}", MAME_XML_SAMPLE))
        .arg(format!("--catver-path={}", CATVER_SAMPLE))
        .arg(format!("--source-path={}", ROMS_SOURCE_PATH))
        .arg(format!("--destination-path={}", destination_path))
        .arg(format!("--report-path={}", report_path))
        .arg("--simulation=true")
        .arg("--progress=false")
        .assert();

    assert
        .success()
        .stdout(predicate::str::contains("Categorizing roms"))
        .stdout(predicate::str::contains("Planning from source"));
}

#[test]
fn should_run_with_precached_mame_version() {
    let tag = get_test_tag();
    set_up(&tag);

    let test_folder = Path::new(TARGET_FOLDER).join(&tag);
    let cache_root = test_folder.join("metadata-cache");
    let cache_dir = cache_root.join("mame-0.289");
    std::fs::create_dir_all(&cache_dir).unwrap();
    std::fs::write(
        cache_dir.join("mame0289.xml"),
        r#"<?xml version="1.0"?><mame build="0.289 (mame0289)"><machine name="game"><driver status="good" emulation="good"/></machine></mame>"#,
    )
    .unwrap();
    std::fs::write(
        cache_dir.join("catver.ini"),
        ";; catver.ini 0.289 / test / MAME 0.289 ;;\n[Category]\ngame=Shooter\n",
    )
    .unwrap();

    let destination_path = test_folder
        .join(CATEGORIZED_ROMS_FOLDER_NAME)
        .to_str()
        .unwrap()
        .to_string();
    let report_path = test_folder.join("report.md").to_str().unwrap().to_string();

    Command::cargo_bin("roms-curator")
        .unwrap()
        .arg("--mame-version=0.289")
        .arg(format!("--metadata-cache-dir={}", cache_root.display()))
        .arg(format!("--source-path={}", ROMS_SOURCE_PATH))
        .arg(format!("--destination-path={}", destination_path))
        .arg(format!("--report-path={}", report_path))
        .arg("--simulation=true")
        .arg("--progress=false")
        .assert()
        .success()
        .stdout(predicate::str::contains("Categorizing roms"));
}

#[test]
fn should_validate_missing_mame_xml_path() {
    set_up(&get_test_tag());
    let mut cmd = Command::cargo_bin("roms-curator").unwrap();

    // should show help
    let assert = cmd.assert();

    assert.stderr(predicate::str::contains(
        "Helper utility to manage Sets of ROMs. Currently only works for MAME ROMs",
    ));

    // should warn that file needs to be a .xml
    let assert = cmd.arg("--mame-xml-path=some-invalid-path").assert();

    assert.stderr(predicate::str::contains(
        "File needs to be a XML file, for ex, mame.xml.",
    ));

    // should warn that file was not found
    let mut cmd = Command::cargo_bin("roms-curator").unwrap();
    let assert = cmd.arg("--mame-xml-path=some-valid-path.xml").assert();

    assert.stderr(predicate::str::contains("File not found."));
}

#[test]
fn should_validate_missing_catver_path() {
    let tag = get_test_tag();
    set_up(&tag);
    let mut cmd = Command::cargo_bin("roms-curator").unwrap();

    let (mame_xml_file, _) = create_mame_and_catver_files_to_bypass_file_not_found_error(&tag);

    let assert = cmd
        .arg(format!("--mame-xml-path={}", mame_xml_file))
        .assert();

    // should want about the other missing mandatory arguments
    assert
        .stderr(predicate::str::contains(
            "the following required arguments were not provided:",
        ))
        .stderr(predicate::str::contains("--catver-path <catver_path>"))
        .stderr(predicate::str::contains("--source-path <source_path>"))
        .stderr(predicate::str::contains(
            "--destination-path <destination_path>",
        ));

    let assert = cmd.arg("--catver-path=some-invalid-path").assert();

    assert.stderr(predicate::str::contains(
        "File needs to be a ini file, for ex, catver.ini",
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
    let test_folder = Path::new(TARGET_FOLDER).join(&tag).canonicalize().unwrap();
    std::fs::create_dir(test_folder.join("first")).unwrap();
    std::fs::create_dir(test_folder.join("second")).unwrap();
    cmd.current_dir(&test_folder);
    let mame_xml_file = Path::new(&mame_xml_file).canonicalize().unwrap();
    let carver_init_file = Path::new(&carver_init_file).canonicalize().unwrap();

    let assert = cmd
        .arg(format!("--mame-xml-path={}", mame_xml_file.display()))
        .arg(format!("--catver-path={}", carver_init_file.display()))
        .arg(format!(
            "--source-path=first{},second{},second",
            fs_sp, fs_sp
        ))
        .arg("--destination-path=output")
        .assert();

    let expected = if fs_sp == '/' {
        format!(
            "source_path: [\"first{}\", \"second{}\", \"second\"]",
            fs_sp, fs_sp
        )
    } else {
        format!(
            "source_path: [\"first{}{}\", \"second{}{}\", \"second\"]",
            fs_sp, fs_sp, fs_sp, fs_sp
        )
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
        "[source-path] and [destination-path] cannot be the same",
    ));
}

#[test]
fn simulation_without_a_report_does_not_create_output() {
    let tag = get_test_tag();
    set_up(&tag);
    let mut cmd = Command::cargo_bin("roms-curator").unwrap();

    let output = Path::new(TARGET_FOLDER)
        .join(&tag)
        .join("simulation-output");

    let assert = cmd
        .arg(format!("--mame-xml-path={}", MAME_XML_SAMPLE))
        .arg(format!("--catver-path={}", CATVER_SAMPLE))
        .arg(format!("--source-path={}", ROMS_SOURCE_PATH))
        .arg(format!("--destination-path={}", output.display()))
        .arg("--simulation=true")
        .assert();

    assert
        .success()
        .stdout(predicate::str::contains("entries Planned"));
    assert!(!output.exists());
}

fn create_mame_and_catver_files_to_bypass_file_not_found_error(test_tag: &str) -> (String, String) {
    let test_folder = Path::new(TARGET_FOLDER).join(test_tag);
    let mame_xml_file = test_folder
        .join("some-valid-path.xml")
        .to_str()
        .unwrap()
        .to_string();
    let catver_init_file = test_folder
        .join("some-valid-path.ini")
        .to_str()
        .unwrap()
        .to_string();
    File::create(&mame_xml_file).unwrap();
    File::create(&catver_init_file).unwrap();
    (mame_xml_file, catver_init_file)
}
