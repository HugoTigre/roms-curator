use crate::models::config::Config;

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
        simulation: false,
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
        source_path: vec!["./".to_string(),"target/".to_string(),"target".to_string()],
        destination_path: "/".to_string(),
        report_path: "".to_string(),
        ignore_not_working_chd: false,
        simulation: false,
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
        "--simulation=true".to_string(),
    ];

    let result = Config::new()
        .build(args.into_iter());

    assert!(matches!(result, Err("Simulation mode needs REPORT_PATH location.")));
}
