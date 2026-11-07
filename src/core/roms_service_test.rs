use std::fs;
use std::path::Path;

use crate::core::config::ExportConfig;
use crate::core::export::name_in_subset_excluded;
use crate::core::roms_service::{
    copy_all_to_rest, missing_chds_for_report, parse_mame_xml_from_str, REST_MEDIA_REASON,
};
use crate::models::dependencies::DependencyKind::{Device, Own, Parent, RomParent};
use crate::models::disks::DiskKind;
use crate::models::plan::{Availability, MediaRole, Plan};
use crate::models::report::OperationStatus;
use crate::models::roms::ChdStatus::{BadDump, Good, NoDump};
use crate::models::roms::RomCategory::{Bios, NotWorking, System, Working};
use crate::models::roms::{
    Chd, Feature, FeatureStatus, RomData, RomFile, RomStatus, Roms, Status, UNKNOWN_CATEGORY,
};
use crate::models::selection::EmulationPolicy;
use crate::{parse_categories, read_categories};

#[test]
fn should_properly_classify_roms() {
    // Older listxml/CatVer sample from when this repository was created.
    // The expected build is the attribute in that file, not a supported version.
    let source = Path::new("tests/resources/listxml_sample.xml");
    let rom_categories = read_categories(Path::new("tests/resources/catver_sample.ini")).unwrap();
    let contents = fs::read_to_string(source).unwrap();
    let catalog = parse_mame_xml_from_str(source, &contents, &rom_categories)
        .unwrap()
        .categorize()
        .unwrap();

    assert_eq!(catalog.mame_build, "0.244 (mame0244)");
    assert_eq!(catalog.unknown_categories, vec!["area51".to_string()]);
    // These four pass the playable policy on their own. elevatora's parent is not in the file.
    assert_eq!(
        catalog.selected,
        vec![
            "005".to_string(),
            "area51".to_string(),
            "elevatora".to_string(),
            "robocop".to_string()
        ]
    );
    assert!(catalog.dependencies.missing.iter().any(|missing| {
        missing.game == "elevatora" && missing.name == "elevator" && missing.kind == Parent
    }));
    assert!(!catalog
        .dependencies
        .archives
        .iter()
        .any(|name| name == "elevatora" || name == "elevator"));
    for name in ["005", "area51", "robocop"] {
        assert!(
            catalog
                .dependencies
                .archives
                .iter()
                .any(|archive| archive == name),
            "{name}"
        );
    }
    let cpu = catalog.roms["005"]
        .data
        .roms
        .iter()
        .find(|rom| rom.name == "1346b.cpu-u25")
        .unwrap();
    assert_eq!(cpu.size, Some(2048));
    assert_eq!(
        cpu.sha1.as_deref(),
        Some("a257c556d31691068ed5c991f1fb2b51da4826db")
    );
    assert!(catalog.roms["005"]
        .data
        .devices
        .iter()
        .any(|device| device == "z80"));
    for (name, rom_count, device_count) in [
        ("005", 23, 8),
        ("100lions", 92, 6),
        ("3dobios", 4, 5),
        ("99bottles", 3, 15),
        ("a24play", 0, 0),
        ("area51", 5, 17),
        ("aristmk6", 90, 6),
        ("as_acp", 4, 6),
        ("elevatora", 21, 17),
        ("robocop", 27, 18),
        ("sv801", 0, 4),
    ] {
        assert_eq!(catalog.roms[name].data.roms.len(), rom_count, "{name}");
        assert_eq!(
            catalog.roms[name].data.devices.len(),
            device_count,
            "{name}"
        );
    }
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
        devices: Vec::new(),
        roms: Vec::new(),
        chd: Vec::new(),
        category: "Maze / Shooter Small".to_string(),
    };
    assert_eq!(classification(&working["005"].data), rom_005);
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
        devices: Vec::new(),
        roms: Vec::new(),
        chd: Vec::new(),
        category: "Platform / Shooter".to_string(),
    };
    assert_eq!(classification(&working["elevatora"].data), rom_elevatora);
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
        devices: Vec::new(),
        roms: Vec::new(),
        chd: Vec::new(),
        category: "Platform / Shooter Scrolling".to_string(),
    };
    assert_eq!(classification(&working["robocop"].data), rom_robocop);
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
        devices: Vec::new(),
        roms: Vec::new(),
        chd: vec![Chd {
            name: "area51".to_string(),
            status: Good,
            optional: false,
            merge: None,
            sha1: Some("3b303bc37e206a6d7339352c869f050d04186f11".to_string()),
        }],
        category: UNKNOWN_CATEGORY.to_string(),
    };
    assert_eq!(classification(&working["area51"].data), rom_area51);
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
        devices: Vec::new(),
        roms: Vec::new(),
        chd: Vec::new(),
        category: "Slot Machine / Video Slot".to_string(),
    };
    assert_eq!(classification(&not_working["100lions"].data), rom_100lions);
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
        devices: Vec::new(),
        roms: Vec::new(),
        chd: Vec::new(),
        category: "System / BIOS".to_string(),
    };
    assert_eq!(classification(&not_working["aristmk6"].data), rom_aristmk6);
    assert!(matches!(not_working["aristmk6"].category, Bios));

    let rom_a24play = RomData {
        status: None,
        is_bios: false,
        is_system: true,
        is_mechanical: false,
        features: Vec::new(),
        clone_of: None,
        rom_of: None,
        devices: Vec::new(),
        roms: Vec::new(),
        chd: Vec::new(),
        category: "System / Device".to_string(),
    };
    assert_eq!(classification(&not_working["a24play"].data), rom_a24play);
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
        devices: Vec::new(),
        roms: Vec::new(),
        chd: Vec::new(),
        category: "System / BIOS".to_string(),
    };
    assert_eq!(classification(&not_working["3dobios"].data), rom_3dobios);
    assert!(matches!(not_working["3dobios"].category, Bios));

    let rom_sv801 = RomData {
        status: None,
        is_bios: false,
        is_system: true,
        is_mechanical: false,
        features: Vec::new(),
        clone_of: None,
        rom_of: None,
        devices: Vec::new(),
        roms: Vec::new(),
        chd: Vec::new(),
        category: "System / Device".to_string(),
    };
    assert_eq!(classification(&not_working["sv801"].data), rom_sv801);
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
        devices: Vec::new(),
        roms: Vec::new(),
        chd: vec![Chd {
            name: "99bottles".to_string(),
            status: BadDump,
            optional: false,
            merge: None,
            sha1: Some("0b874178c8dd3cfc451deb53dc7936dc4ad5a04f".to_string()),
        }],
        category: "MultiGame / Compilation".to_string(),
    };
    assert_eq!(
        classification(&not_working["99bottles"].data),
        rom_99bottles
    );
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
        devices: Vec::new(),
        roms: Vec::new(),
        chd: Vec::new(),
        category: "Slot Machine / Reels".to_string(),
    };
    assert_eq!(classification(&not_working["as_acp"].data), rom_as_acp);
    assert!(matches!(not_working["as_acp"].category, NotWorking));
}

#[test]
fn should_exclude_subsets() {
    // subset_start only
    let mut args = ExportConfig::include_incomplete_dependencies();
    args.subset_start = "r".to_string();

    assert!(name_in_subset_excluded(&args, "area51"));
    assert!(!name_in_subset_excluded(&args, "robocop"));

    // subset_end only
    let mut args = ExportConfig::include_incomplete_dependencies();
    args.subset_end = "sv0".to_string();

    assert!(!name_in_subset_excluded(&args, "robocop"));
    assert!(name_in_subset_excluded(&args, "sv801"));

    // subset_start and subset_end
    let mut args = ExportConfig::include_incomplete_dependencies();
    args.subset_start = "az".to_string();
    args.subset_end = "sv0".to_string();

    assert!(name_in_subset_excluded(&args, "as_acp"));
    assert!(!name_in_subset_excluded(&args, "elevatora"));
    assert!(!name_in_subset_excluded(&args, "robocop"));
    assert!(name_in_subset_excluded(&args, "sv801"));
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
    let catalog = parse_mame_xml_from_str(source, &contents, &categories)
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
    assert!(matches!(nodump.category, Working));

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
    let catalog = parse_mame_xml_from_str(
        Path::new("list.xml"),
        r#"<mame build="0.300 (mame0300)"><machine name="game"><driver status="good" emulation="good"/></machine></mame>"#,
        &categories,
    )
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

    let missing_name = r#"<mame build="fixture"><machine><driver status="good" emulation="good"/></machine></mame>"#;
    let error = parse_mame_xml_from_str(source, missing_name, &categories)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("fixture.xml") && error.contains("missing required attribute 'name'"),
        "{error}"
    );

    let inherited = r#"<mame build="fixture"><machine name="parent"><disk status="baddump"/><disk name="second"/><driver status="good" emulation="good"/></machine></mame>"#;
    let error = parse_mame_xml_from_str(source, inherited, &categories)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("Machine 'parent'")
            && error.contains("fixture.xml")
            && error.contains("disk")
            && error.contains("'name'"),
        "{error}"
    );

    let unknown_status = r#"<mame build="fixture"><machine name="parent"><disk name="shared" status="weird"/><driver status="good" emulation="good"/></machine></mame>"#;
    let error = parse_mame_xml_from_str(source, unknown_status, &categories)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("Machine 'parent'") && error.contains("unknown disk status 'weird'"),
        "{error}"
    );

    let missing_driver = r#"<mame build="fixture"><machine name="parent"><description>Parent</description></machine></mame>"#;
    let error = parse_mame_xml_from_str(source, missing_driver, &categories)
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

    let good_feature = r#"<mame build="fixture"><machine name="parent"><feature type="sound" status="good"/><driver status="good" emulation="good"/></machine></mame>"#;
    let error = parse_mame_xml_from_str(source, good_feature, &categories)
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

    let missing_rom = r#"<mame build="fixture"><machine name="parent"><rom size="4"/><driver status="good" emulation="good"/></machine></mame>"#;
    let error = parse_mame_xml_from_str(source, missing_rom, &categories)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("Machine 'parent'")
            && error.contains("fixture.xml")
            && error.contains("rom")
            && error.contains("'name'"),
        "{error}"
    );

    let bad_size = r#"<mame build="fixture"><machine name="parent"><rom name="game.bin" size="nope"/><driver status="good" emulation="good"/></machine></mame>"#;
    let error = parse_mame_xml_from_str(source, bad_size, &categories)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("Machine 'parent'")
            && error.contains("rom 'game.bin' has unknown size 'nope'"),
        "{error}"
    );

    let missing_device = r#"<mame build="fixture"><machine name="parent"><device_ref/><driver status="good" emulation="good"/></machine></mame>"#;
    let error = parse_mame_xml_from_str(source, missing_device, &categories)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("Machine 'parent'")
            && error.contains("device_ref")
            && error.contains("'name'"),
        "{error}"
    );

    let entry_error = parse_categories("[Category]\nparent\n", Path::new("notes.ini")).unwrap_err();
    assert!(
        entry_error.to_string().contains("notes.ini:2")
            && entry_error.to_string().contains("expected name=category"),
        "{entry_error}"
    );
}

#[test]
fn selects_good_and_imperfect_arcade_machines() {
    let source = Path::new("selection.xml");
    let categories = parse_categories(
        "[Category]\n\
         both=Shooter / Gallery\n\
         unemulated=Puzzle / Maze\n\
         early=Shooter / Gallery\n\
         rough=Shooter / Gallery\n\
         pins=Shooter / Gallery\n\
         slots=Slot Machine / Video Slot\n\
         sy888b=Shooter / Gallery\n\
         home=Computer / Home\n\
         labeled=Shooter / Computer\n",
        Path::new("catver.ini"),
    )
    .unwrap();
    let xml_contents = r#"<mame build="fixture">
            <machine name="both">
                <feature type="sound" status="imperfect"/>
                <feature type="graphics" status="imperfect"/>
                <driver status="good" emulation="imperfect"/>
            </machine>
            <machine name="unemulated">
                <feature type="sound" status="unemulated"/>
                <feature type="graphics" status="unemulated"/>
                <driver status="imperfect" emulation="good"/>
            </machine>
            <machine name="early"><driver status="preliminary" emulation="good"/></machine>
            <machine name="rough"><driver status="good" emulation="preliminary"/></machine>
            <machine name="pins" ismechanical="yes"><driver status="good" emulation="good"/></machine>
            <machine name="slots">
                <feature type="sound" status="imperfect"/>
                <feature type="graphics" status="imperfect"/>
                <driver status="good" emulation="imperfect"/>
            </machine>
            <machine name="sy888b"><driver status="good" emulation="good"/></machine>
            <machine name="home"><driver status="good" emulation="good"/></machine>
            <machine name="labeled"><driver status="good" emulation="good"/></machine>
        </mame>"#;

    let parsed = parse_mame_xml_from_str(source, xml_contents, &categories).unwrap();
    let playable = parsed.categorize().unwrap();
    assert!(matches!(playable.roms["both"].category, Working));
    assert!(matches!(playable.roms["unemulated"].category, Working));
    assert!(matches!(playable.roms["slots"].category, Working));
    assert!(matches!(playable.roms["labeled"].category, Working));
    assert!(matches!(playable.roms["early"].category, NotWorking));
    assert!(matches!(playable.roms["rough"].category, NotWorking));
    assert!(matches!(playable.roms["pins"].category, NotWorking));
    assert!(playable.roms["sy888b"].data.is_system);
    assert!(matches!(playable.roms["sy888b"].category, System));
    assert!(playable.roms["home"].data.is_system);
    assert!(matches!(playable.roms["home"].category, System));

    let parsed = parse_mame_xml_from_str(source, xml_contents, &categories).unwrap();
    let strict = parsed.categorize_with(EmulationPolicy::Strict).unwrap();
    assert!(matches!(strict.roms["both"].category, NotWorking));
    assert!(matches!(strict.roms["unemulated"].category, NotWorking));
    assert!(matches!(strict.roms["labeled"].category, Working));
    assert!(matches!(strict.roms["home"].category, System));
}

fn classification(data: &RomData) -> RomData {
    let mut data = data.clone();
    data.devices.clear();
    data.roms.clear();
    data
}

#[test]
fn keeps_selected_clones_when_support_files_are_not_games() {
    let source = Path::new("tests/resources/listxml_metadata.xml");
    let categories = read_categories(Path::new("tests/resources/catver_sections.ini")).unwrap();
    let contents = fs::read_to_string(source).unwrap();
    let catalog = parse_mame_xml_from_str(source, &contents, &categories)
        .unwrap()
        .categorize()
        .unwrap();

    // The parent fails the playable policy. The clone does not, and it stays selected.
    assert!(!catalog.roms["parent"].selected);
    assert!(matches!(catalog.roms["parent"].category, NotWorking));
    assert!(catalog.roms["clone"].selected);
    assert!(matches!(catalog.roms["clone"].category, Working));
    assert_eq!(
        catalog.roms["clone"].data.clone_of.as_deref(),
        Some("parent")
    );
    assert_eq!(catalog.roms["clone"].data.rom_of.as_deref(), Some("parent"));

    // BIOS and devices stay in the catalog and off the visible list.
    assert!(matches!(catalog.roms["bios"].category, Bios));
    assert!(!catalog.roms["bios"].selected);
    assert!(matches!(catalog.roms["cpu"].category, System));
    assert!(!catalog.roms["cpu"].selected);
    assert!(catalog.roms["usesbios"].selected);
    assert!(!catalog
        .selected
        .iter()
        .any(|name| name == "bios" || name == "cpu" || name == "parent"));
    assert_eq!(
        catalog.roms["usesbios"].data.rom_of.as_deref(),
        Some("bios")
    );
    assert_eq!(
        catalog.roms["usesbios"].data.devices,
        vec!["cpu".to_string()]
    );
    // The preliminary BIOS stays off the list and remains a required archive.
    assert!(matches!(catalog.roms["usesbios"].category, Working));
    assert!(catalog.dependencies.edges.iter().any(|edge| {
        edge.game == "usesbios" && edge.archive == "bios" && edge.kind == RomParent
    }));
    assert!(catalog
        .dependencies
        .edges
        .iter()
        .any(|edge| { edge.game == "clone" && edge.archive == "parent" && edge.kind == Parent }));
    assert!(!catalog
        .dependencies
        .archives
        .iter()
        .any(|name| name == "clone"));

    assert_eq!(
        catalog.roms["parent"].data.roms,
        vec![
            RomFile {
                name: "parent.bin".to_string(),
                size: Some(16),
                crc: Some("abcdabcd".to_string()),
                sha1: Some("dddddddddddddddddddddddddddddddddddddddd".to_string()),
                merge: None,
                bios: None,
                status: Good,
                optional: false,
            },
            RomFile {
                name: "merged.bin".to_string(),
                size: Some(8),
                crc: None,
                sha1: None,
                merge: Some("parent.bin".to_string()),
                bios: None,
                status: Good,
                optional: false,
            },
        ]
    );
    assert_eq!(catalog.roms["parent"].data.devices, vec!["z80".to_string()]);
    assert_eq!(
        catalog.roms["nodump"].data.roms,
        vec![RomFile {
            name: "missing.bin".to_string(),
            size: None,
            crc: None,
            sha1: None,
            merge: None,
            bios: None,
            status: NoDump,
            optional: false,
        }]
    );
    // A nodump disk does not change the copy category. The game stays selected.
    assert!(catalog.roms["nodump"].selected);
    assert!(matches!(catalog.roms["nodump"].category, Working));
    assert_eq!(
        catalog.roms["usesbios"].data.roms,
        vec![RomFile {
            name: "game.bin".to_string(),
            size: Some(4),
            crc: None,
            sha1: Some("ffffffffffffffffffffffffffffffffffffffff".to_string()),
            merge: None,
            bios: Some("default".to_string()),
            status: Good,
            optional: false,
        }]
    );
    assert_eq!(
        catalog.selected,
        vec![
            "clone".to_string(),
            "nocat".to_string(),
            "nodump".to_string(),
            "spaced".to_string(),
            "usesbios".to_string(),
        ]
    );
}

fn catalog_of(xml: &str) -> crate::models::roms::Catalog {
    let categories = parse_categories("[Category]\n", Path::new("catver.ini")).unwrap();
    let contents = format!(r#"<mame build="deps">{xml}</mame>"#);
    parse_mame_xml_from_str(Path::new("deps.xml"), &contents, &categories)
        .unwrap()
        .categorize()
        .unwrap()
}

#[test]
fn resolves_merged_archives_without_demoting_games() {
    let catalog = catalog_of(
        r#"
        <machine name="parent">
            <driver status="preliminary" emulation="preliminary"/>
        </machine>
        <machine name="clone" cloneof="parent" romof="parent">
            <driver status="good" emulation="good"/>
        </machine>
        <machine name="orphan" cloneof="ghost" romof="ghost">
            <driver status="good" emulation="good"/>
        </machine>
        <machine name="bios" isbios="yes"/>
        <machine name="play" romof="bios">
            <device_ref name="board"/>
            <device_ref name="z80"/>
            <device_ref name="gone"/>
            <driver status="good" emulation="good"/>
        </machine>
        <machine name="also" romof="bios">
            <driver status="good" emulation="good"/>
        </machine>
        <machine name="board" isdevice="yes" romof="snd"/>
        <machine name="snd" isdevice="yes">
            <rom name="s.bin" size="1"/>
        </machine>
        <machine name="z80" isdevice="yes"/>
        <machine name="dsnd" isdevice="yes">
            <rom name="d.bin" size="1"/>
        </machine>
        <machine name="dsndclone" isdevice="yes" cloneof="dsnd"/>
        <machine name="needsdev">
            <device_ref name="dsndclone"/>
            <driver status="good" emulation="good"/>
        </machine>
        <machine name="grand">
            <driver status="preliminary" emulation="preliminary"/>
        </machine>
        <machine name="mid" cloneof="grand" romof="grand">
            <driver status="preliminary" emulation="preliminary"/>
        </machine>
        <machine name="leaf" cloneof="mid" romof="mid">
            <driver status="good" emulation="good"/>
        </machine>
        <machine name="early" romof="bios">
            <driver status="preliminary" emulation="preliminary"/>
        </machine>
        <machine name="shell" isdevice="yes" cloneof="lost"/>
        <machine name="useshell">
            <device_ref name="shell"/>
            <driver status="good" emulation="good"/>
        </machine>
        "#,
    );

    assert!(catalog.roms["clone"].selected);
    assert!(matches!(catalog.roms["clone"].category, Working));
    assert!(!catalog.roms["parent"].selected);
    assert_eq!(
        catalog
            .dependencies
            .edges
            .iter()
            .find(|edge| edge.game == "clone")
            .map(|edge| (edge.kind, edge.archive.as_str())),
        Some((Parent, "parent"))
    );
    assert!(!catalog
        .dependencies
        .archives
        .iter()
        .any(|name| name == "clone"));
    assert!(catalog.dependencies.missing.iter().any(|missing| {
        missing.game == "orphan" && missing.name == "ghost" && missing.kind == Parent
    }));
    assert!(!catalog
        .dependencies
        .edges
        .iter()
        .any(|edge| edge.game == "orphan"));

    assert!(catalog
        .dependencies
        .edges
        .iter()
        .any(|edge| { edge.game == "play" && edge.kind == Own && edge.archive == "play" }));
    assert!(catalog
        .dependencies
        .edges
        .iter()
        .any(|edge| { edge.game == "play" && edge.kind == RomParent && edge.archive == "bios" }));
    assert!(catalog
        .dependencies
        .edges
        .iter()
        .any(|edge| { edge.game == "also" && edge.kind == RomParent && edge.archive == "bios" }));
    assert_eq!(
        catalog
            .dependencies
            .archives
            .iter()
            .filter(|name| name.as_str() == "bios")
            .count(),
        1
    );
    assert!(catalog
        .dependencies
        .edges
        .iter()
        .any(|edge| { edge.game == "play" && edge.kind == Device && edge.archive == "snd" }));
    assert!(!catalog
        .dependencies
        .archives
        .iter()
        .any(|name| name == "board" || name == "z80"));
    assert!(catalog.dependencies.missing.iter().any(|missing| {
        missing.game == "play" && missing.name == "gone" && missing.kind == Device
    }));
    assert!(!catalog
        .dependencies
        .edges
        .iter()
        .any(|edge| edge.game == "early"));
    assert!(catalog
        .dependencies
        .edges
        .iter()
        .any(|edge| { edge.game == "needsdev" && edge.kind == Device && edge.archive == "dsnd" }));
    assert!(!catalog
        .dependencies
        .archives
        .iter()
        .any(|name| name == "dsndclone"));
    assert!(catalog
        .dependencies
        .edges
        .iter()
        .any(|edge| { edge.game == "leaf" && edge.kind == Parent && edge.archive == "grand" }));
    assert!(!catalog
        .dependencies
        .archives
        .iter()
        .any(|name| name == "leaf" || name == "mid"));
    assert!(catalog.dependencies.missing.iter().any(|missing| {
        missing.game == "useshell" && missing.name == "lost" && missing.kind == Device
    }));
    assert!(!catalog
        .dependencies
        .archives
        .iter()
        .any(|name| name == "shell" || name == "lost"));

    let cycle = catalog_of(
        r#"
        <machine name="a" romof="b"><driver status="good" emulation="good"/></machine>
        <machine name="b" romof="c"><driver status="good" emulation="good"/></machine>
        <machine name="c" romof="a"><driver status="good" emulation="good"/></machine>
        "#,
    );
    assert_eq!(
        cycle.dependencies.cycles,
        vec![vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "a".to_string()
        ]]
    );

    let looped = catalog_of(
        r#"
        <machine name="a" cloneof="b"><driver status="good" emulation="good"/></machine>
        <machine name="b" cloneof="a"><driver status="good" emulation="good"/></machine>
        "#,
    );
    assert_eq!(
        looped.dependencies.cycles,
        vec![vec!["a".to_string(), "b".to_string(), "a".to_string()]]
    );
    assert!(looped.dependencies.archives.is_empty());
}

#[test]
fn copy_report_selected_games_follows_subset_and_catalog_selection() {
    let catalog = catalog_of(
        r#"
        <machine name="alpha">
            <driver status="good" emulation="good"/>
        </machine>
        <machine name="beta">
            <driver status="good" emulation="good"/>
        </machine>
        <machine name="omega">
            <driver status="preliminary" emulation="preliminary"/>
        </machine>
        "#,
    );
    assert_eq!(catalog.selected.len(), 2);

    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("alpha.zip"), b"a").unwrap();
    fs::write(source.join("beta.zip"), b"b").unwrap();

    let mut args = ExportConfig::include_incomplete_dependencies();
    args.source_path = vec![source.to_str().unwrap().to_string()];
    args.destination_path = destination.to_str().unwrap().to_string();
    args.subset_start = "b".to_string();
    args.subset_end = "c".to_string();

    let report = catalog.copy_roms(&args).unwrap();
    assert_eq!(report.selected_games, 1);

    let mut output = Vec::new();
    report.write_to(&mut output).unwrap();
    assert!(String::from_utf8(output)
        .unwrap()
        .contains("- Selected games: 1"));
}

#[test]
fn subset_keeps_parent_archive_outside_the_range() {
    let catalog = catalog_of(
        r#"
        <machine name="parent">
            <driver status="preliminary" emulation="preliminary"/>
        </machine>
        <machine name="clone" cloneof="parent" romof="parent">
            <driver status="good" emulation="good"/>
        </machine>
        "#,
    );
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&destination).unwrap();
    fs::write(source.join("clone.zip"), b"clone").unwrap();
    fs::write(source.join("parent.zip"), b"parent").unwrap();
    fs::write(source.join("aaa.zip"), b"unrelated").unwrap();

    let mut args = ExportConfig::include_incomplete_dependencies();
    args.source_path = vec![source.to_str().unwrap().to_string()];
    args.destination_path = destination.to_str().unwrap().to_string();
    args.subset_start = "c".to_string();
    args.subset_end = "cz".to_string();

    let report = catalog.copy_roms(&args).unwrap();
    assert_eq!(report.selected_games, 1);
    assert!(report.roms_working.iter().any(|entry| {
        entry.rom_name == "parent.zip" && entry.status == OperationStatus::Copied
    }));
    assert!(report
        .roms_other
        .iter()
        .any(|entry| { entry.rom_name == "clone.zip" && entry.status == OperationStatus::Copied }));
    assert!(!report
        .roms_other
        .iter()
        .chain(&report.roms_working)
        .chain(&report.ignored_roms)
        .any(|entry| entry.rom_name == "aaa.zip"));
    assert_eq!(
        fs::read(destination.join("roms/parent.zip")).unwrap(),
        b"parent"
    );
    assert_eq!(
        fs::read(destination.join("rest/clone.zip")).unwrap(),
        b"clone"
    );
    assert!(!destination.join("roms/aaa.zip").exists());
    assert!(!destination.join("rest/aaa.zip").exists());
    let names = fs::read_to_string(destination.join("selected.txt")).unwrap();
    assert_eq!(names, "clone\n");
}

#[test]
fn resolves_shared_delta_and_device_disks_without_changing_category() {
    let catalog = catalog_of(
        r#"
        <machine name="parent">
            <disk name="base" sha1="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" status="baddump"/>
            <disk name="extra" sha1="bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"/>
            <driver status="preliminary" emulation="preliminary"/>
        </machine>
        <machine name="share" cloneof="parent" romof="parent">
            <disk name="child" merge="base" sha1="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"/>
            <driver status="good" emulation="good"/>
        </machine>
        <machine name="delta" cloneof="parent" romof="parent">
            <disk name="base" sha1="cccccccccccccccccccccccccccccccccccccccc"/>
            <driver status="good" emulation="good"/>
        </machine>
        <machine name="same" cloneof="parent" romof="parent">
            <disk name="renamed" sha1="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"/>
            <driver status="good" emulation="good"/>
        </machine>
        <machine name="nodump">
            <disk name="missing" sha1="dddddddddddddddddddddddddddddddddddddddd" status="nodump"/>
            <driver status="good" emulation="good"/>
        </machine>
        <machine name="optional">
            <disk name="opt" sha1="eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee" optional="yes"/>
            <driver status="good" emulation="good"/>
        </machine>
        <machine name="hdd" isdevice="yes">
            <disk name="disk" sha1="ffffffffffffffffffffffffffffffffffffffff"/>
        </machine>
        <machine name="gallery">
            <device_ref name="hdd"/>
            <device_ref name="missingdev"/>
            <driver status="good" emulation="good"/>
        </machine>
        <machine name="gap" cloneof="parent" romof="parent">
            <disk name="child" merge="nope"/>
            <driver status="good" emulation="good"/>
        </machine>
        <machine name="unused">
            <disk name="lonely" sha1="1111111111111111111111111111111111111111"/>
            <driver status="preliminary" emulation="preliminary"/>
        </machine>
        "#,
    );

    assert!(matches!(catalog.roms["share"].category, Working));
    assert!(catalog.roms["share"].selected);
    assert!(matches!(catalog.roms["parent"].category, NotWorking));
    assert!(!catalog.roms["parent"].selected);
    assert!(matches!(catalog.roms["nodump"].category, Working));

    let stored = |game: &str, kind: DiskKind, machine: &str, name: &str| {
        catalog.disks.edges.iter().any(|edge| {
            edge.game == game && edge.kind == kind && edge.machine == machine && edge.name == name
        })
    };
    assert!(stored("share", DiskKind::Shared, "parent", "base"));
    assert!(stored("same", DiskKind::Shared, "parent", "base"));
    assert!(stored("delta", DiskKind::Own, "delta", "base"));
    assert!(stored("delta", DiskKind::DeltaParent, "parent", "base"));
    assert!(stored("nodump", DiskKind::Own, "nodump", "missing"));
    assert!(catalog
        .disks
        .edges
        .iter()
        .any(|edge| { edge.game == "nodump" && edge.status == NoDump }));
    assert!(stored("gallery", DiskKind::Device, "hdd", "disk"));
    assert!(!catalog
        .disks
        .disks
        .iter()
        .any(|disk| disk.name == "extra" || disk.name == "opt" || disk.name == "lonely"));
    assert!(catalog.disks.missing.iter().any(|missing| {
        missing.game == "gap" && missing.kind == DiskKind::Shared && missing.name == "nope"
    }));
    assert!(catalog.disks.missing.iter().any(|missing| {
        missing.game == "gallery"
            && missing.kind == DiskKind::Device
            && missing.machine == "missingdev"
    }));
    assert_eq!(
        catalog
            .disks
            .edges
            .iter()
            .find(|edge| edge.game == "share")
            .map(|edge| edge.status),
        Some(BadDump)
    );

    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(source.join("parent")).unwrap();
    fs::write(source.join("parent/base.chd"), b"parent disk").unwrap();
    fs::create_dir_all(source.join("delta")).unwrap();
    fs::write(source.join("delta/base.chd"), b"delta disk").unwrap();
    fs::create_dir_all(source.join("nodump")).unwrap();
    fs::write(source.join("nodump/missing.chd"), b"nodump disk").unwrap();
    fs::create_dir_all(source.join("hdd")).unwrap();
    fs::write(source.join("hdd/disk.chd"), b"device disk").unwrap();
    fs::create_dir_all(source.join("optional")).unwrap();
    fs::write(source.join("optional/opt.chd"), b"optional disk").unwrap();
    fs::write(source.join("gallery.zip"), b"gallery").unwrap();

    let mut args = ExportConfig::include_incomplete_dependencies();
    args.source_path = vec![source.to_str().unwrap().to_string()];
    args.destination_path = destination.to_str().unwrap().to_string();
    // `parent` sorts after this end. `delta`, `nodump`, `hdd`, and `gallery` sort inside it.
    args.subset_start = "d".to_string();
    args.subset_end = "optional".to_string();
    args.ignore_not_working_chd = true;
    assert!(name_in_subset_excluded(&args, "parent"));
    assert!(!name_in_subset_excluded(&args, "delta"));
    assert!(!name_in_subset_excluded(&args, "nodump"));
    assert!(!name_in_subset_excluded(&args, "hdd"));

    let report = catalog.copy_roms(&args).unwrap();
    assert_eq!(
        fs::read(destination.join("roms/parent/base.chd")).unwrap(),
        b"parent disk"
    );
    assert_eq!(
        fs::read(destination.join("roms/delta/base.chd")).unwrap(),
        b"delta disk"
    );
    assert_eq!(
        fs::read(destination.join("roms/nodump/missing.chd")).unwrap(),
        b"nodump disk"
    );
    assert_eq!(
        fs::read(destination.join("roms/hdd/disk.chd")).unwrap(),
        b"device disk"
    );
    assert_eq!(
        fs::read(destination.join("rest/optional/opt.chd")).unwrap(),
        b"optional disk"
    );
    assert!(!destination.join("roms/optional").exists());
    assert!(report.roms_working.iter().any(|entry| {
        entry.rom_name == "gallery.zip" && entry.status == OperationStatus::Copied
    }));
    assert!(report
        .roms_other
        .iter()
        .any(|entry| { entry.rom_name == "optional" && entry.status == OperationStatus::Copied }));
}

#[test]
fn copies_an_unzipped_rom_directory_that_has_no_disks() {
    let catalog = catalog_of(
        r#"
        <machine name="plain">
            <driver status="good" emulation="good"/>
        </machine>
        "#,
    );
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(source.join("plain")).unwrap();
    fs::write(source.join("plain/rom.bin"), b"rom").unwrap();

    let mut args = ExportConfig::include_incomplete_dependencies();
    args.source_path = vec![source.to_str().unwrap().to_string()];
    args.destination_path = destination.to_str().unwrap().to_string();

    catalog.copy_roms(&args).unwrap();
    assert_eq!(
        fs::read(destination.join("roms/plain/rom.bin")).unwrap(),
        b"rom"
    );
}

#[test]
fn map_copy_keeps_disk_directories_without_a_resolved_disk_set() {
    let catalog = catalog_of(
        r#"
        <machine name="game">
            <disk name="disk" sha1="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"/>
            <driver status="good" emulation="good"/>
        </machine>
        "#,
    );
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(source.join("game")).unwrap();
    fs::write(source.join("game/disk.chd"), b"disk").unwrap();

    let mut args = ExportConfig::include_incomplete_dependencies();
    args.source_path = vec![source.to_str().unwrap().to_string()];
    args.destination_path = destination.to_str().unwrap().to_string();

    copy_all_to_rest(catalog.roms, &args).unwrap();
    assert_eq!(
        fs::read(destination.join("rest/game/disk.chd")).unwrap(),
        b"disk"
    );
}

#[test]
fn copy_report_lists_chds_absent_from_the_source() {
    let catalog = catalog_of(
        r#"
        <machine name="game">
            <disk name="disk" sha1="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"/>
            <driver status="good" emulation="good"/>
        </machine>
        "#,
    );
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("game.zip"), b"rom").unwrap();

    let mut args = ExportConfig::include_incomplete_dependencies();
    args.source_path = vec![source.to_str().unwrap().to_string()];
    args.destination_path = destination.to_str().unwrap().to_string();

    let report = catalog.copy_roms(&args).unwrap();
    assert_eq!(report.missing_chds.len(), 1);
    let entry = &report.missing_chds[0];
    assert_eq!(entry.game, "game");
    assert_eq!(entry.machine, "game");
    assert_eq!(entry.disk, "disk");
    assert_eq!(entry.reason, "Own disk");
}

#[test]
fn chd_pairs_include_second_source_root() {
    let root = tempfile::tempdir().unwrap();
    let roms = root.path().join("roms");
    let chds = root.path().join("chds");
    fs::create_dir_all(&roms).unwrap();
    fs::create_dir_all(chds.join("area51mx")).unwrap();
    fs::write(chds.join("area51mx").join("area51mx.chd"), b"chd").unwrap();

    let mut args = ExportConfig::default();
    args.source_path = vec![roms.to_str().unwrap().into(), chds.to_str().unwrap().into()];

    assert_eq!(
        crate::core::roms_service::indexed_chd_file_count(&args).unwrap(),
        1
    );
    let present = crate::models::plan::chd_pairs_in_sources(&args.source_path).unwrap();
    assert!(present.contains(&("area51mx".into(), "area51mx".into())));
}

#[test]
fn missing_bios_for_report_lists_absent_bios_zip() {
    let catalog = catalog_of(
        r#"
        <machine name="bios" isbios="yes">
            <driver status="good" emulation="good"/>
        </machine>
        <machine name="play" romof="bios">
            <driver status="good" emulation="good"/>
        </machine>
        "#,
    );
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("play.zip"), b"rom").unwrap();

    let mut args = ExportConfig::default();
    args.source_path = vec![source.to_str().unwrap().into()];
    args.destination_path = root.path().join("dest").to_str().unwrap().into();

    let missing = crate::core::roms_service::missing_bios_for_report(&catalog, &args).unwrap();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].archive, "bios");
    assert!(missing[0].needed_by.iter().any(|name| name == "play"));
}

#[test]
fn missing_chds_for_report_includes_held_back_games() {
    let catalog = catalog_of(
        r#"
        <machine name="game">
            <disk name="disk" sha1="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"/>
            <driver status="good" emulation="good"/>
        </machine>
        "#,
    );
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("game.zip"), b"rom").unwrap();

    let mut args = ExportConfig::default();
    args.source_path = vec![source.to_str().unwrap().into()];
    args.destination_path = root.path().join("dest").to_str().unwrap().into();

    let missing = missing_chds_for_report(&catalog, &args).unwrap();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].game, "game");
    assert_eq!(missing[0].disk, "disk");
}

#[test]
fn require_complete_dependencies_holds_back_missing_chd() {
    let catalog = catalog_of(
        r#"
        <machine name="game">
            <disk name="disk" sha1="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"/>
            <driver status="good" emulation="good"/>
        </machine>
        "#,
    );
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("game.zip"), b"rom").unwrap();

    let mut args = ExportConfig::default();
    args.source_path = vec![source.to_str().unwrap().into()];
    args.destination_path = root.path().join("dest").to_str().unwrap().into();

    let ready = crate::core::launch_ready::export_game_names(&catalog, &args).unwrap();
    assert!(ready.is_empty());
    let held = crate::core::launch_ready::held_back_games(&catalog, &args).unwrap();
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].machine, "game");
}

#[test]
fn missing_chds_for_report_lists_unresolved_disks() {
    let catalog = catalog_of(
        r#"
        <machine name="solo">
            <disk name="only" sha1="2222222222222222222222222222222222222222"/>
            <driver status="preliminary" emulation="preliminary"/>
        </machine>
        <machine name="alias" cloneof="solo" romof="solo">
            <disk name="other" sha1="3333333333333333333333333333333333333333"/>
            <driver status="good" emulation="good"/>
        </machine>
        "#,
    );
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("source")).unwrap();
    let mut args = ExportConfig::include_incomplete_dependencies();
    args.source_path = vec![root.path().join("source").to_str().unwrap().into()];

    let missing = missing_chds_for_report(&catalog, &args).unwrap();
    assert!(missing
        .iter()
        .any(|entry| entry.game == "alias" && entry.disk == "other"));
    assert!(missing
        .iter()
        .any(|entry| entry.reason.starts_with("Unresolved")));
}

#[test]
fn records_a_missing_delta_parent_when_no_same_named_disk_exists() {
    let catalog = catalog_of(
        r#"
        <machine name="solo">
            <disk name="only" sha1="2222222222222222222222222222222222222222"/>
            <driver status="preliminary" emulation="preliminary"/>
        </machine>
        <machine name="alias" cloneof="solo" romof="solo">
            <disk name="other" sha1="3333333333333333333333333333333333333333"/>
            <driver status="good" emulation="good"/>
        </machine>
        <machine name="optparent">
            <disk name="base" sha1="4444444444444444444444444444444444444444" optional="yes"/>
            <driver status="preliminary" emulation="preliminary"/>
        </machine>
        <machine name="optclone" cloneof="optparent" romof="optparent">
            <disk name="base" sha1="5555555555555555555555555555555555555555"/>
            <driver status="good" emulation="good"/>
        </machine>
        <machine name="empty">
            <driver status="preliminary" emulation="preliminary"/>
        </machine>
        <machine name="added" cloneof="empty" romof="empty">
            <disk name="own" sha1="6666666666666666666666666666666666666666"/>
            <driver status="good" emulation="good"/>
        </machine>
        "#,
    );

    assert!(catalog.disks.edges.iter().any(|edge| {
        edge.game == "alias" && edge.kind == DiskKind::Own && edge.machine == "alias"
    }));
    assert!(catalog.disks.missing.iter().any(|missing| {
        missing.game == "alias" && missing.kind == DiskKind::DeltaParent && missing.name == "other"
    }));
    assert!(!catalog
        .disks
        .disks
        .iter()
        .any(|disk| disk.machine == "solo"));
    assert!(catalog.disks.edges.iter().any(|edge| {
        edge.game == "optclone" && edge.kind == DiskKind::Own && edge.name == "base"
    }));
    assert!(catalog.disks.missing.iter().any(|missing| {
        missing.game == "optclone"
            && missing.kind == DiskKind::DeltaParent
            && missing.name == "base"
    }));
    assert!(!catalog
        .disks
        .disks
        .iter()
        .any(|disk| disk.machine == "optparent"));
    assert!(catalog
        .disks
        .edges
        .iter()
        .any(|edge| { edge.game == "added" && edge.kind == DiskKind::Own && edge.name == "own" }));
    assert!(!catalog
        .disks
        .missing
        .iter()
        .any(|missing| missing.game == "added"));

    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("alias.zip"), b"clone").unwrap();
    fs::write(source.join("optclone.zip"), b"clone").unwrap();
    fs::write(source.join("added.zip"), b"clone").unwrap();

    let mut args = ExportConfig::include_incomplete_dependencies();
    args.source_path = vec![source.to_str().unwrap().to_string()];
    args.destination_path = destination.to_str().unwrap().to_string();
    args.report_path = root.path().join("report.md").to_str().unwrap().to_string();

    let report = catalog.copy_roms(&args).unwrap();
    assert!(report.missing_chds.iter().any(|entry| {
        entry.game == "alias"
            && entry.machine == "solo"
            && entry.disk == "other"
            && entry.reason == "Unresolved delta parent"
    }));
    assert!(report.missing_chds.iter().any(|entry| {
        entry.game == "alias"
            && entry.machine == "alias"
            && entry.disk == "other"
            && entry.reason == "Own disk"
    }));
    assert!(report.missing_chds.iter().any(|entry| {
        entry.game == "optclone"
            && entry.machine == "optparent"
            && entry.disk == "base"
            && entry.reason == "Unresolved delta parent"
    }));
    assert!(report.missing_chds.iter().any(|entry| {
        entry.game == "added"
            && entry.machine == "added"
            && entry.disk == "own"
            && entry.reason == "Own disk"
    }));

    assert!(report.to_file(&args.report_path).unwrap());
    let markdown = fs::read_to_string(&args.report_path).unwrap();
    assert!(markdown.contains("### Missing CHDs"));
    assert!(markdown.contains("reason: Unresolved delta parent"));
}

#[test]
fn records_a_disk_merge_cycle() {
    let catalog = catalog_of(
        r#"
        <machine name="a" cloneof="b">
            <disk name="da" merge="db"/>
            <driver status="good" emulation="good"/>
        </machine>
        <machine name="b" cloneof="a">
            <disk name="db" merge="da"/>
            <driver status="preliminary" emulation="preliminary"/>
        </machine>
        "#,
    );
    assert!(
        !catalog.disks.cycles.is_empty(),
        "{:?}",
        catalog.disks.cycles
    );
    assert!(catalog.disks.disks.is_empty());
}

#[test]
fn plan_lists_games_missing_media_and_conflicts_and_rechecks_sizes() {
    let root = tempfile::tempdir().unwrap();
    let xml = root.path().join("mame.xml");
    let catver = root.path().join("catver.ini");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    let body = r#"
        <machine name="parent">
            <driver status="preliminary" emulation="preliminary"/>
        </machine>
        <machine name="clone" cloneof="parent" romof="parent">
            <driver status="good" emulation="good"/>
        </machine>
    "#;
    let document = format!(r#"<mame build="plan-test">{body}</mame>"#);
    let categories_text = "[Category]\nclone=Shooter / Gallery\n";
    fs::write(&xml, &document).unwrap();
    fs::write(&catver, categories_text).unwrap();
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("clone.zip"), b"clone").unwrap();

    let categories = parse_categories(categories_text, &catver).unwrap();
    let catalog = parse_mame_xml_from_str(&xml, &document, &categories)
        .unwrap()
        .categorize()
        .unwrap();

    let mut args = ExportConfig::include_incomplete_dependencies();
    args.mame_xml_path = xml.to_str().unwrap().into();
    args.catver_path = catver.to_str().unwrap().into();
    args.source_path = vec![source.to_str().unwrap().into()];
    args.destination_path = destination.to_str().unwrap().into();

    let plan = catalog.plan(&args).unwrap();
    assert_eq!(plan, catalog.plan(&args).unwrap());
    assert_eq!(plan.version, 2);
    assert_eq!(plan.metadata.mame_build, "plan-test");
    assert_eq!(plan.metadata.emulation_policy, "playable");
    assert_eq!(plan.metadata.mame_xml_sha1.len(), 40);
    assert_eq!(plan.metadata.catver_sha1.len(), 40);
    assert_eq!(
        plan.games
            .iter()
            .map(|game| game.machine.as_str())
            .collect::<Vec<_>>(),
        vec!["clone"]
    );
    assert!(plan.games[0].reason.contains("imperfect"));
    assert_eq!(plan.games[0].category, "Shooter / Gallery");
    assert!(plan.media.iter().any(|media| {
        media.machine == "clone"
            && media.role == MediaRole::Archive
            && media.availability == Availability::Present
            && media.bytes == Some(5)
            && media.destination.as_deref().is_some_and(|destination| {
                Path::new(destination).ends_with(Path::new("rest").join("clone.zip"))
            })
            && media.reasons == vec![REST_MEDIA_REASON.to_string()]
    }));
    assert!(plan.selected_names_path.ends_with("selected.txt"));
    assert!(plan.media.iter().any(|media| {
        media.machine == "parent"
            && media.role == MediaRole::Archive
            && media.availability == Availability::Missing
            && media.bytes.is_none()
            && media
                .reasons
                .iter()
                .any(|reason| reason == "Parent archive for clone")
    }));
    assert!(plan.conflicts.is_empty());
    plan.recheck().unwrap();

    let plan_path = root.path().join("plan.json");
    let copy_path = root.path().join("plan-copy.json");
    plan.write(plan_path.to_str().unwrap()).unwrap();
    plan.write(copy_path.to_str().unwrap()).unwrap();
    assert_eq!(fs::read(&plan_path).unwrap(), fs::read(&copy_path).unwrap());
    assert_eq!(Plan::read(&plan_path).unwrap(), plan);

    fs::write(source.join("clone.zip"), b"clone!").unwrap();
    let error = plan.recheck().unwrap_err().to_string();
    assert!(error.contains("Size changed"), "{error}");

    fs::create_dir_all(destination.join("rest")).unwrap();
    fs::write(destination.join("rest/clone.zip"), b"already").unwrap();
    let conflicted = catalog.plan(&args).unwrap();
    assert!(conflicted
        .conflicts
        .iter()
        .any(|conflict| conflict.contains("Destination already exists")));
    let error = catalog.copy_roms(&args).unwrap_err().to_string();
    assert!(error.contains("Destination already exists"), "{error}");
}

#[test]
fn plan_recheck_catches_metadata_new_files_disks_and_stable_conflicts() {
    let root = tempfile::tempdir().unwrap();
    let xml = root.path().join("mame.xml");
    let catver = root.path().join("catver.ini");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    let body = r#"
        <machine name="parent">
            <disk name="base" sha1="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"/>
            <driver status="preliminary" emulation="preliminary"/>
        </machine>
        <machine name="clone" cloneof="parent" romof="parent">
            <disk name="base" sha1="bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"/>
            <driver status="good" emulation="good"/>
        </machine>
    "#;
    let document = format!(r#"<mame build="plan-more">{body}</mame>"#);
    let categories_text = "[Category]\n";
    fs::write(&xml, &document).unwrap();
    fs::write(&catver, categories_text).unwrap();
    fs::create_dir_all(source.join("clone")).unwrap();
    fs::write(source.join("clone/base.chd"), b"clone disk").unwrap();

    let categories = parse_categories(categories_text, &catver).unwrap();
    let catalog = parse_mame_xml_from_str(&xml, &document, &categories)
        .unwrap()
        .categorize()
        .unwrap();
    let mut args = ExportConfig::include_incomplete_dependencies();
    args.mame_xml_path = xml.to_str().unwrap().into();
    args.catver_path = catver.to_str().unwrap().into();
    args.source_path = vec![source.to_str().unwrap().into()];
    args.destination_path = destination.to_str().unwrap().into();

    let plan = catalog.plan(&args).unwrap();
    let clone_disk = plan
        .media
        .iter()
        .find(|media| media.machine == "clone" && media.role == MediaRole::Disk)
        .unwrap();
    assert_eq!(clone_disk.availability, Availability::Present);
    assert!(clone_disk
        .reasons
        .iter()
        .any(|reason| reason == "Own disk base for clone"));
    assert!(clone_disk
        .reasons
        .iter()
        .all(|reason| !reason.contains("archive")));
    let parent_archive = plan
        .media
        .iter()
        .find(|media| media.machine == "parent" && media.role == MediaRole::Archive)
        .unwrap();
    assert!(parent_archive
        .reasons
        .iter()
        .any(|reason| reason == "Parent archive for clone"));
    assert!(parent_archive
        .reasons
        .iter()
        .all(|reason| !reason.contains("disk")));

    fs::write(&xml, format!("{document}\n")).unwrap();
    let error = plan.recheck().unwrap_err().to_string();
    assert!(error.contains("MAME XML changed"), "{error}");
    fs::write(&xml, &document).unwrap();

    fs::write(source.join("parent.zip"), b"parent").unwrap();
    let error = plan.recheck().unwrap_err().to_string();
    assert!(error.contains("is now at"), "{error}");
    fs::remove_file(source.join("parent.zip")).unwrap();
    plan.recheck().unwrap();

    let later = root.path().join("z-source");
    let earlier = root.path().join("a-source");
    fs::create_dir_all(&later).unwrap();
    fs::create_dir_all(&earlier).unwrap();
    fs::write(later.join("game.zip"), b"one").unwrap();
    fs::write(earlier.join("game.zip"), b"two").unwrap();
    let game_xml = root.path().join("game.xml");
    let game_document = r#"<mame build="clash"><machine name="game"><driver status="good" emulation="good"/></machine></mame>"#;
    fs::write(&game_xml, game_document).unwrap();
    let games = parse_mame_xml_from_str(&game_xml, game_document, &categories)
        .unwrap()
        .categorize()
        .unwrap();
    let mut clash_args = args.clone();
    clash_args.mame_xml_path = game_xml.to_str().unwrap().into();
    clash_args.source_path = vec![
        later.to_str().unwrap().into(),
        earlier.to_str().unwrap().into(),
    ];
    let clash = games.plan(&clash_args).unwrap();
    assert!(clash
        .conflicts
        .iter()
        .any(|conflict| conflict.contains("Conflicting destination")));
    clash.recheck().unwrap();
    let error = games
        .copy_with_plan(&clash_args, &clash, None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("Conflicting destination"), "{error}");
    assert!(!error.contains("Destination conflicts changed"), "{error}");
}

#[test]
fn copy_with_plan_refuses_a_run_that_does_not_match_the_plan() {
    let root = tempfile::tempdir().unwrap();
    let xml = root.path().join("mame.xml");
    let catver = root.path().join("catver.ini");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(&destination).unwrap();
    let document = r#"<mame build="match"><machine name="clone"><driver status="good" emulation="good"/></machine></mame>"#;
    fs::write(&xml, document).unwrap();
    fs::write(&catver, "[Category]\n").unwrap();
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("clone.zip"), b"clone").unwrap();
    let categories = parse_categories("[Category]\n", &catver).unwrap();

    let load = || parse_mame_xml_from_str(&xml, document, &categories).unwrap();
    let mut args = ExportConfig::include_incomplete_dependencies();
    args.mame_xml_path = xml.to_str().unwrap().into();
    args.catver_path = catver.to_str().unwrap().into();
    args.source_path = vec![source.to_str().unwrap().into()];
    args.destination_path = destination.to_str().unwrap().into();
    args.subset_start = "d".into();

    let catalog = load().categorize().unwrap();
    let plan = catalog.plan(&args).unwrap();
    assert!(plan.games.is_empty());
    let strict = load().categorize_with(EmulationPolicy::Strict).unwrap();
    let error = strict
        .copy_with_plan(&args, &plan, None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("emulation policy"), "{error}");

    let mut wider = args.clone();
    wider.subset_start.clear();
    let error = catalog
        .copy_with_plan(&wider, &plan, None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("subset"), "{error}");
    assert!(!destination.join("roms/clone.zip").exists());
    assert!(!destination.join("rest/clone.zip").exists());

    let catalog = load().categorize().unwrap();
    let mut open = args.clone();
    open.subset_start.clear();
    let plan = catalog.plan(&open).unwrap();
    let mut alias = open.clone();
    alias.destination_path = destination
        .join("..")
        .join("destination")
        .to_str()
        .unwrap()
        .into();
    catalog.copy_with_plan(&alias, &plan, None).unwrap();
    assert_eq!(
        fs::read(destination.join("roms/clone.zip")).unwrap(),
        b"clone"
    );
}

#[test]
fn copy_cancelled_after_first_file_keeps_that_file_and_skips_selected_txt() {
    use crate::CancelFlag;

    let catalog = catalog_of(
        r#"
        <machine name="alpha">
            <driver status="good" emulation="good"/>
        </machine>
        <machine name="beta">
            <driver status="good" emulation="good"/>
        </machine>
        "#,
    );
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("alpha.zip"), b"alpha").unwrap();
    fs::write(source.join("beta.zip"), b"beta").unwrap();

    let mut args = ExportConfig::include_incomplete_dependencies();
    args.source_path = vec![source.to_str().unwrap().to_string()];
    args.destination_path = destination.to_str().unwrap().to_string();

    let plan = catalog.plan(&args).unwrap();
    let cancel = CancelFlag::new();
    let gate = cancel.clone();
    let copied = destination.join("roms/alpha.zip");
    cancel.on_each_poll(move || {
        if copied.exists() {
            gate.cancel();
        }
    });

    let error = catalog
        .copy_with_plan(&args, &plan, Some(&cancel))
        .unwrap_err()
        .to_string();
    assert!(error.contains("cancelled"), "{error}");
    assert_eq!(
        fs::read(destination.join("roms/alpha.zip")).unwrap(),
        b"alpha"
    );
    assert!(!destination.join("roms/beta.zip").exists());
    assert!(!destination.join("selected.txt").exists());
}

#[test]
fn copy_cancelled_during_the_last_file_does_not_publish_selected_txt() {
    use crate::CancelFlag;

    let catalog = catalog_of(
        r#"
        <machine name="alpha">
            <driver status="good" emulation="good"/>
        </machine>
        "#,
    );
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("alpha.zip"), b"alpha").unwrap();

    let mut args = ExportConfig::include_incomplete_dependencies();
    args.source_path = vec![source.to_str().unwrap().to_string()];
    args.destination_path = destination.to_str().unwrap().to_string();

    let plan = catalog.plan(&args).unwrap();
    let cancel = CancelFlag::new();
    let gate = cancel.clone();
    let copied = destination.join("roms/alpha.zip");
    cancel.on_each_poll(move || {
        if copied.exists() {
            gate.cancel();
        }
    });

    let error = catalog
        .copy_with_plan(&args, &plan, Some(&cancel))
        .unwrap_err()
        .to_string();
    assert!(error.contains("cancelled"), "{error}");
    assert_eq!(
        fs::read(destination.join("roms/alpha.zip")).unwrap(),
        b"alpha"
    );
    assert!(!destination.join("selected.txt").exists());
}

#[test]
fn plan_cancelled_leaves_the_destination_unchanged() {
    use crate::CancelFlag;

    let catalog = catalog_of(
        r#"
        <machine name="alpha">
            <driver status="good" emulation="good"/>
        </machine>
        "#,
    );
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("alpha.zip"), b"alpha").unwrap();

    let mut args = ExportConfig::include_incomplete_dependencies();
    args.source_path = vec![source.to_str().unwrap().to_string()];
    args.destination_path = destination.to_str().unwrap().to_string();

    let cancel = CancelFlag::new();
    cancel.cancel();
    let error = catalog
        .plan_with_progress(&args, None, Some(&cancel))
        .unwrap_err()
        .to_string();
    assert!(error.contains("cancelled"), "{error}");
    assert!(!destination.join("roms").exists());
    assert!(!destination.join("rest").exists());
    assert!(!destination.join("selected.txt").exists());
    assert_eq!(fs::read(source.join("alpha.zip")).unwrap(), b"alpha");
}
