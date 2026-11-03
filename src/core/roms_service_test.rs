use std::fs;
use std::path::Path;

use crate::core::args::Args;
use crate::core::roms_service::{parse, RomsExt};
use crate::models::dependencies::DependencyKind::{Device, Own, Parent, RomParent};
use crate::models::report::OperationStatus;
use crate::models::roms::ChdStatus::{BadDump, Good, NoDump};
use crate::models::roms::RomCategory::{Bios, NotWorking, System, Working};
use crate::models::roms::{
    Chd, Feature, FeatureStatus, RomData, RomFile, RomStatus, Roms, Status, UNKNOWN_CATEGORY,
};
use crate::models::selection::EmulationPolicy;
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

    let missing_rom = read_mame_xml(
        r#"<mame build="fixture"><machine name="parent"><rom size="4"/><driver status="good" emulation="good"/></machine></mame>"#,
    )
    .unwrap();
    let error = parse(&missing_rom, &categories, source)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("Machine 'parent'")
            && error.contains("fixture.xml")
            && error.contains("rom")
            && error.contains("'name'"),
        "{error}"
    );

    let bad_size = read_mame_xml(
        r#"<mame build="fixture"><machine name="parent"><rom name="game.bin" size="nope"/><driver status="good" emulation="good"/></machine></mame>"#,
    )
    .unwrap();
    let error = parse(&bad_size, &categories, source)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("Machine 'parent'")
            && error.contains("rom 'game.bin' has unknown size 'nope'"),
        "{error}"
    );

    let missing_device = read_mame_xml(
        r#"<mame build="fixture"><machine name="parent"><device_ref/><driver status="good" emulation="good"/></machine></mame>"#,
    )
    .unwrap();
    let error = parse(&missing_device, &categories, source)
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
    let doc = read_mame_xml(
        r#"<mame build="fixture">
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
        </mame>"#,
    )
    .unwrap();

    let parsed = parse(&doc, &categories, source).unwrap();
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

    let parsed = parse(&doc, &categories, source).unwrap();
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
    let doc = read_mame_xml(&contents).unwrap();
    let catalog = parse(&doc, &categories, source)
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
    let doc = read_mame_xml(&contents).unwrap();
    parse(&doc, &categories, Path::new("deps.xml"))
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

    let mut args = Args::new();
    args.source_path = vec![source.to_str().unwrap().to_string()];
    args.destination_path = destination.to_str().unwrap().to_string();
    args.subset_start = "c".to_string();
    args.subset_end = "cz".to_string();

    let report = catalog.copy_roms(&args).unwrap();
    assert!(report.roms_other.iter().any(|entry| {
        entry.rom_name == "parent.zip" && entry.status == OperationStatus::Copied
    }));
    assert!(report
        .roms_working
        .iter()
        .any(|entry| { entry.rom_name == "clone.zip" && entry.status == OperationStatus::Copied }));
    assert!(!report
        .roms_other
        .iter()
        .chain(&report.roms_working)
        .chain(&report.ignored_roms)
        .any(|entry| entry.rom_name == "aaa.zip"));
    assert_eq!(
        fs::read(destination.join("other/parent.zip")).unwrap(),
        b"parent"
    );
    assert_eq!(
        fs::read(destination.join("working/clone.zip")).unwrap(),
        b"clone"
    );
    assert!(!destination.join("other/aaa.zip").exists());
    assert!(!destination.join("working/aaa.zip").exists());
}
