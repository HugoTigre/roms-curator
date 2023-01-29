use std::path::Path;
use crate::models::config::Config;
use crate::utils::{create_dir, remove_dir};

#[test]
fn should_validate_missing_mame_xml_path() {
    let result = Config::new()
        .build(vec!["roms-curator".to_string()].into_iter());

    assert!(matches!(result, Err("Missing mame_xml_path.")));

    let args = vec![
        "roms-curator".to_string(),
        "--mame_xml_path=some-invalid-path".to_string(),
    ];

    let result = Config::new()
        .build(args.into_iter());

    assert!(matches!(result, Err("File needs to be a XML file, for ex, mame.xml.")));
}

#[test]
fn should_validate_missing_catver_path() {
    let args = vec![
        "roms-curator".to_string(),
        "--mame_xml_path=some-valid-path.xml".to_string(),
    ];

    let result = Config::new()
        .build(args.into_iter());

    assert!(matches!(result, Err("Missing catver_path.")));

    let args = vec![
        "roms-curator".to_string(),
        "--mame_xml_path=some-valid-path.xml".to_string(),
        "--catver_path=some-invalid-path".to_string(),
    ];

    let result = Config::new()
        .build(args.into_iter());

    assert!(matches!(result, Err("File needs to be a ini file, for ex, catver.ini.")));
}

#[test]
fn should_set_defaults() {
    let args = vec![
        "roms-curator".to_string(),
        "--mame_xml_path=some-valid-path.xml".to_string(),
        "--catver_path=some-valid-path.ini".to_string(),
        "--source_path=/".to_string(),
        "--destination_path=./".to_string(),
    ];

    let result = Config::new()
        .build(args.into_iter());

    let _config = Config {
        mame_xml_path: "self.mame_xml_path".to_string(),
        catver_path: "some-valid-path.ini".to_string(),
        source_path: vec!["/".to_string()],
        destination_path: "/".to_string(),
        report_path: "".to_string(),
        ignore_not_working_chd: false,
        simulate: false,
    };

    assert!(matches!(result, Ok(_config)));
}

#[test]
fn should_parse_source_paths() {
    let args = vec![
        "roms-curator".to_string(),
        "--mame_xml_path=some-valid-path.xml".to_string(),
        "--catver_path=some-valid-path.ini".to_string(),
        "--source_path=./,target/,target".to_string(),
        "--destination_path=/".to_string(),
    ];

    let result = Config::new()
        .build(args.into_iter());

    let _config = Config {
        mame_xml_path: "some-valid-path.xml".to_string(),
        catver_path: "some-valid-path.ini".to_string(),
        source_path: vec!["./".to_string(), "target/".to_string(), "target".to_string()],
        destination_path: "/".to_string(),
        report_path: "".to_string(),
        ignore_not_working_chd: false,
        simulate: false,
    };

    assert_eq!(result.unwrap(), _config);
}

#[test]
fn source_path_and_destination_path_cannot_be_equal() {
    let args = vec![
        "roms-curator".to_string(),
        "--mame_xml_path=some-valid-path.xml".to_string(),
        "--catver_path=some-valid-path.ini".to_string(),
        "--source_path=target/".to_string(),
        "--destination_path=target/".to_string(),
    ];

    let result = Config::new()
        .build(args.into_iter());

    assert!(matches!(result, Err("SOURCE_PATH and DESTINATION_PATH cannot be the same.")));
}

#[test]
fn simulation_mode_needs_report_path() {
    let args = vec![
        "roms-curator".to_string(),
        "--mame_xml_path=some-valid-path.xml".to_string(),
        "--catver_path=some-valid-path.ini".to_string(),
        "--source_path=target/".to_string(),
        "--destination_path=/".to_string(),
        "--simulate=true".to_string(),
    ];

    let result = Config::new()
        .build(args.into_iter());

    assert!(matches!(result, Err("Simulation mode needs REPORT_PATH location.")));
}

#[test]
fn should_support_windows_paths() {
    let roms_folder = Path::new("target/tests/roms");
    remove_dir(roms_folder);
    let source_folder_1 = roms_folder.join("MAME 0.244 ROMs (merged)");
    let source_folder_2 = roms_folder.join("MAME 0.243 CHDs (merged)");
    let destination_folder_2 = roms_folder.join("roms-curated");

    create_dir(roms_folder);
    create_dir(&source_folder_1);
    create_dir(&source_folder_2);
    create_dir(&destination_folder_2);

    let args = vec![
        "roms-curator".to_string(),
        "--mame_xml_path=C:\\Users\\Ellie\\Desktop\\roms-curator\\mame-roms.xml".to_string(),
        "--catver_path=C:\\Users\\Ellie\\Desktop\\roms-curator\\catver.ini".to_string(),
        "--source_path=target\\tests\\roms\\MAME 0.244 ROMs (merged),target\\tests\\roms\\MAME 0.243 CHDs (merged)".to_string(),
        "--destination_path=target\\tests\\roms\\roms-curated".to_string(),
        "--report_path=target\\tests\\roms\\report.md".to_string(),
        "--simulate=true".to_string(),
    ];

    let result = Config::new()
        .build(args.into_iter());

    let fs_sp = std::path::MAIN_SEPARATOR;

    let _config = Config {
        mame_xml_path: format!("{}{}{}{}{}{}{}{}{}{}{}", "C:", fs_sp, "Users", fs_sp, "Ellie", fs_sp, "Desktop", fs_sp, "roms-curator", fs_sp, "mame-roms.xml"),
        catver_path: format!("{}{}{}{}{}{}{}{}{}{}{}", "C:", fs_sp, "Users", fs_sp, "Ellie", fs_sp, "Desktop", fs_sp, "roms-curator", fs_sp, "catver.ini"),
        source_path: vec![
            format!("{}{}{}{}{}{}{}", "target", fs_sp, "tests", fs_sp, "roms", fs_sp, "MAME 0.244 ROMs (merged)"),
            format!("{}{}{}{}{}{}{}", "target", fs_sp, "tests", fs_sp, "roms", fs_sp, "MAME 0.243 CHDs (merged)"),
        ],
        destination_path: format!("{}{}{}{}{}{}{}", "target", fs_sp, "tests", fs_sp, "roms", fs_sp, "roms-curated"),
        report_path: format!("{}{}{}{}{}{}{}", "target", fs_sp, "tests", fs_sp, "roms", fs_sp, "report.md"),
        ignore_not_working_chd: false,
        simulate: true,
    };

    remove_dir(roms_folder);

    assert_eq!(result.unwrap(), _config);
}
