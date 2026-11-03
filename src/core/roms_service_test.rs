use std::fs;
use std::path::Path;

use crate::core::args::Args;
use crate::core::roms_service::{parse, RomsExt};
use crate::models::roms::ChdStatus::{BadDump, Good, NoDump};
use crate::models::roms::RomCategory::{Bios, NotWorking, System, Working};
use crate::models::roms::{
    Chd, Feature, FeatureStatus, RomData, RomStatus, Roms, Status, UNKNOWN_CATEGORY,
};
use crate::{parse_categories, read_categories, read_mame_xml};

#[test]
fn should_properly_classify_roms() {
    // Older listxml/CatVer sample from when this repository was created.
    // The expected build is the attribute in that file, not a supported version.
    let source = Path::new("tests/resources/listxml_0244.xml");
    let rom_categories = read_categories(Path::new("tests/resources/catver_0244.ini")).unwrap();
    let contents = fs::read_to_string(source).unwrap();
    let doc = read_mame_xml(&contents).unwrap();
    let catalog = parse(&doc, &rom_categories, source)
        .unwrap()
        .categorize()
        .unwrap();

    assert_eq!(catalog.mame_build, "0.244 (mame0244)");
    assert_eq!(catalog.unknown_categories, vec!["area51".to_string()]);
    let roms = catalog.roms;

    let (working, not_working): (Roms, Roms) = roms
        .into_iter()
        .partition(|(_, rom)| matches!(rom.category, Working));

    assert_eq!(working.len(), 4);
    assert_eq!(not_working.len(), 7);

    // working roms

    let rom_005 = RomData {
        status: Some(RomStatus {
            driver: Status::Imperfect,
            emulation: Status::Good,
        }),
        is_bios: false,
        is_system: false,
        is_mechanical: false,
        features: vec![Feature {
            typ: "sound".to_string(),
            status: FeatureStatus::Imperfect,
        }],
        clone_of: None,
        rom_of: None,
        chd: Vec::new(),
        category: "Maze / Shooter Small".to_string(),
    };
    assert_eq!(working["005"].data, rom_005);
    assert!(matches!(working["005"].category, Working));

    let rom_elevatora = RomData {
        status: Some(RomStatus {
            driver: Status::Good,
            emulation: Status::Good,
        }),
        is_bios: false,
        is_system: false,
        is_mechanical: false,
        features: Vec::new(),
        clone_of: Some("elevator".to_string()),
        rom_of: Some("elevator".to_string()),
        chd: Vec::new(),
        category: "Platform / Shooter".to_string(),
    };
    assert_eq!(working["elevatora"].data, rom_elevatora);
    assert!(matches!(working["elevatora"].category, Working));

    let rom_robocop = RomData {
        status: Some(RomStatus {
            driver: Status::Imperfect,
            emulation: Status::Good,
        }),
        is_bios: false,
        is_system: false,
        is_mechanical: false,
        features: vec![Feature {
            typ: "sound".to_string(),
            status: FeatureStatus::Imperfect,
        }],
        clone_of: None,
        rom_of: None,
        chd: Vec::new(),
        category: "Platform / Shooter Scrolling".to_string(),
    };
    assert_eq!(working["robocop"].data, rom_robocop);
    assert!(matches!(working["robocop"].category, Working));

    let rom_area51 = RomData {
        status: Some(RomStatus {
            driver: Status::Good,
            emulation: Status::Good,
        }),
        is_bios: false,
        is_system: false,
        is_mechanical: false,
        features: Vec::new(),
        clone_of: None,
        rom_of: None,
        chd: vec![Chd {
            name: "area51".to_string(),
            status: Good,
            optional: false,
            merge: None,
            sha1: Some("3b303bc37e206a6d7339352c869f050d04186f11".to_string()),
        }],
        category: UNKNOWN_CATEGORY.to_string(),
    };
    assert_eq!(working["area51"].data, rom_area51);
    assert!(matches!(working["area51"].category, Working));

    // not working roms

    let rom_100lions = RomData {
        status: Some(RomStatus {
            driver: Status::Preliminary,
            emulation: Status::Preliminary,
        }),
        is_bios: false,
        is_system: false,
        is_mechanical: false,
        features: vec![Feature {
            typ: "sound".to_string(),
            status: FeatureStatus::Unemulated,
        }],
        clone_of: None,
        rom_of: Some("aristmk6".to_string()),
        chd: Vec::new(),
        category: "Slot Machine / Video Slot".to_string(),
    };
    assert_eq!(not_working["100lions"].data, rom_100lions);
    assert!(matches!(not_working["100lions"].category, NotWorking));

    let rom_aristmk6 = RomData {
        status: Some(RomStatus {
            driver: Status::Preliminary,
            emulation: Status::Preliminary,
        }),
        is_bios: true,
        is_system: true,
        is_mechanical: false,
        features: vec![Feature {
            typ: "sound".to_string(),
            status: FeatureStatus::Unemulated,
        }],
        clone_of: None,
        rom_of: None,
        chd: Vec::new(),
        category: "System / BIOS".to_string(),
    };
    assert_eq!(not_working["aristmk6"].data, rom_aristmk6);
    assert!(matches!(not_working["aristmk6"].category, Bios));

    let rom_a24play = RomData {
        status: None,
        is_bios: false,
        is_system: true,
        is_mechanical: false,
        features: Vec::new(),
        clone_of: None,
        rom_of: None,
        chd: Vec::new(),
        category: "System / Device".to_string(),
    };
    assert_eq!(not_working["a24play"].data, rom_a24play);
    assert!(matches!(not_working["a24play"].category, System));

    let rom_3dobios = RomData {
        status: Some(RomStatus {
            driver: Status::Preliminary,
            emulation: Status::Preliminary,
        }),
        is_bios: true,
        is_system: true,
        is_mechanical: false,
        features: vec![Feature {
            typ: "sound".to_string(),
            status: FeatureStatus::Unemulated,
        }],
        clone_of: None,
        rom_of: None,
        chd: Vec::new(),
        category: "System / BIOS".to_string(),
    };
    assert_eq!(not_working["3dobios"].data, rom_3dobios);
    assert!(matches!(not_working["3dobios"].category, Bios));

    let rom_sv801 = RomData {
        status: None,
        is_bios: false,
        is_system: true,
        is_mechanical: false,
        features: Vec::new(),
        clone_of: None,
        rom_of: None,
        chd: Vec::new(),
        category: "System / Device".to_string(),
    };
    assert_eq!(not_working["sv801"].data, rom_sv801);
    assert!(matches!(not_working["sv801"].category, System));

    let rom_99bottles = RomData {
        status: Some(RomStatus {
            driver: Status::Preliminary,
            emulation: Status::Preliminary,
        }),
        is_bios: false,
        is_system: false,
        is_mechanical: false,
        features: vec![Feature {
            typ: "sound".to_string(),
            status: FeatureStatus::Unemulated,
        }],
        clone_of: Some("gammagic".to_string()),
        rom_of: Some("gammagic".to_string()),
        chd: vec![Chd {
            name: "99bottles".to_string(),
            status: BadDump,
            optional: false,
            merge: None,
            sha1: Some("0b874178c8dd3cfc451deb53dc7936dc4ad5a04f".to_string()),
        }],
        category: "MultiGame / Compilation".to_string(),
    };
    assert_eq!(not_working["99bottles"].data, rom_99bottles);
    assert!(matches!(not_working["99bottles"].category, NotWorking));

    let rom_as_acp = RomData {
        status: Some(RomStatus {
            driver: Status::Preliminary,
            emulation: Status::Preliminary,
        }),
        is_bios: false,
        is_system: false,
        is_mechanical: true,
        features: vec![Feature {
            typ: "sound".to_string(),
            status: FeatureStatus::Unemulated,
        }],
        clone_of: None,
        rom_of: None,
        chd: Vec::new(),
        category: "Slot Machine / Reels".to_string(),
    };
    assert_eq!(not_working["as_acp"].data, rom_as_acp);
    assert!(matches!(not_working["as_acp"].category, NotWorking));
}

#[test]
fn should_exclude_subsets() {
    // subset_start only
    let mut args = Args::new();
    args.subset_start = "r".to_string();

    assert!(Roms::is_excluded(&args, "area51"));
    assert!(!Roms::is_excluded(&args, "robocop"));

    // subset_end only
    let mut args = Args::new();
    args.subset_end = "sv0".to_string();

    assert!(!Roms::is_excluded(&args, "robocop"));
    assert!(Roms::is_excluded(&args, "sv801"));

    // subset_start and subset_end
    let mut args = Args::new();
    args.subset_start = "az".to_string();
    args.subset_end = "sv0".to_string();

    assert!(Roms::is_excluded(&args, "as_acp"));
    assert!(!Roms::is_excluded(&args, "elevatora"));
    assert!(!Roms::is_excluded(&args, "robocop"));
    assert!(Roms::is_excluded(&args, "sv801"));
}

#[test]
fn parses_disk_defaults_and_reports_unknown_categories() {
    let source = Path::new("tests/resources/listxml_metadata.xml");
    let categories = read_categories(Path::new("tests/resources/catver_sections.ini")).unwrap();
    assert_eq!(
        categories.get("parent").map(String::as_str),
        Some("Shooter / Gallery")
    );
    assert_eq!(
        categories.get("spaced").map(String::as_str),
        Some("Puzzle / Maze")
    );
    assert!(!categories.contains_key("ignored"));
    assert!(!categories.contains_key("RootFolderIcon"));

    let contents = fs::read_to_string(source).unwrap();
    let doc = read_mame_xml(&contents).unwrap();
    let catalog = parse(&doc, &categories, source)
        .unwrap()
        .categorize()
        .unwrap();

    // Copied from the fixture. This is not a supported-version check.
    assert_eq!(catalog.mame_build, "0.289 (mame0289)");
    assert_eq!(catalog.unknown_categories, vec!["nocat".to_string()]);

    let parent = &catalog.roms["parent"];
    assert_eq!(parent.data.category, "Shooter / Gallery");
    assert!(matches!(parent.category, NotWorking));
    assert_eq!(
        parent.data.chd,
        vec![
            Chd {
                name: "shared".to_string(),
                status: BadDump,
                optional: false,
                merge: None,
                sha1: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()),
            },
            Chd {
                name: "childdisk".to_string(),
                status: Good,
                optional: true,
                merge: Some("shared".to_string()),
                sha1: Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string()),
            },
        ]
    );

    assert_eq!(
        catalog.roms["clone"].data.features,
        vec![Feature {
            typ: "sound".to_string(),
            status: FeatureStatus::Unemulated,
        }]
    );
    let nodump = &catalog.roms["nodump"];
    assert_eq!(
        nodump.data.chd,
        vec![Chd {
            name: "missing".to_string(),
            status: NoDump,
            optional: false,
            merge: None,
            sha1: Some("cccccccccccccccccccccccccccccccccccccccc".to_string()),
        }]
    );
    assert!(matches!(nodump.category, NotWorking));

    let nocat = &catalog.roms["nocat"];
    assert_eq!(nocat.data.category, UNKNOWN_CATEGORY);
    assert!(!nocat.data.is_system);
    assert!(matches!(nocat.category, Working));
    assert_eq!(catalog.roms["spaced"].data.category, "Puzzle / Maze");
}

#[test]
fn records_any_mame_build_string() {
    let categories =
        parse_categories("[Category]\ngame=Shooter\n", Path::new("catver.ini")).unwrap();
    let doc = read_mame_xml(
        r#"<mame build="0.300 (mame0300)"><machine name="game"><driver status="good" emulation="good"/></machine></mame>"#,
    )
    .unwrap();
    let catalog = parse(&doc, &categories, Path::new("list.xml"))
        .unwrap()
        .categorize()
        .unwrap();
    assert_eq!(catalog.mame_build, "0.300 (mame0300)");
    assert!(catalog.unknown_categories.is_empty());
}

#[test]
fn malformed_metadata_names_the_machine_attribute_and_source() {
    let source = Path::new("fixture.xml");
    let categories =
        parse_categories("[Category]\nparent=Shooter\n", Path::new("fixture.ini")).unwrap();

    let missing_name = read_mame_xml(
        r#"<mame build="fixture"><machine><driver status="good" emulation="good"/></machine></mame>"#,
    )
    .unwrap();
    let error = parse(&missing_name, &categories, source)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("fixture.xml") && error.contains("missing required attribute 'name'"),
        "{error}"
    );

    let inherited = read_mame_xml(
        r#"<mame build="fixture"><machine name="parent"><disk status="baddump"/><disk name="second"/><driver status="good" emulation="good"/></machine></mame>"#,
    )
    .unwrap();
    let error = parse(&inherited, &categories, source)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("Machine 'parent'")
            && error.contains("fixture.xml")
            && error.contains("disk")
            && error.contains("'name'"),
        "{error}"
    );

    let unknown_status = read_mame_xml(
        r#"<mame build="fixture"><machine name="parent"><disk name="shared" status="weird"/><driver status="good" emulation="good"/></machine></mame>"#,
    )
    .unwrap();
    let error = parse(&unknown_status, &categories, source)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("Machine 'parent'") && error.contains("unknown disk status 'weird'"),
        "{error}"
    );

    let missing_driver = read_mame_xml(
        r#"<mame build="fixture"><machine name="parent"><description>Parent</description></machine></mame>"#,
    )
    .unwrap();
    let error = parse(&missing_driver, &categories, source)
        .unwrap()
        .categorize()
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("Machine 'parent'")
            && error.contains("fixture.xml")
            && error.contains("missing required driver status"),
        "{error}"
    );

    let good_feature = read_mame_xml(
        r#"<mame build="fixture"><machine name="parent"><feature type="sound" status="good"/><driver status="good" emulation="good"/></machine></mame>"#,
    )
    .unwrap();
    let error = parse(&good_feature, &categories, source)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("Machine 'parent'") && error.contains("unknown feature status 'good'"),
        "{error}"
    );

    let category_error = parse_categories("parent=Shooter\n", Path::new("notes.ini")).unwrap_err();
    assert!(
        category_error.to_string().contains("notes.ini")
            && category_error.to_string().contains("missing [Category]"),
        "{category_error}"
    );

    let entry_error = parse_categories("[Category]\nparent\n", Path::new("notes.ini")).unwrap_err();
    assert!(
        entry_error.to_string().contains("notes.ini:2")
            && entry_error.to_string().contains("expected name=category"),
        "{entry_error}"
    );
}
