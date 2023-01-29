use std::fs;
use crate::{build_category_list, read_mame_xml};
use crate::core::roms_service::{parse, UnfilteredRomsExt};
use crate::models::roms::{Chd, Feature, FeatureStatus, RomData, Roms, RomStatus, Status};
use crate::models::roms::ChdStatus::{BadDump, NoStatus};
use crate::models::roms::RomCategory::{Working, NotWorking, Bios, System};

#[test]
fn should_properly_classify_roms() {
    let rom_categories = build_category_list("tests/resources/catver_0244.ini".to_string()).unwrap();

    let contents = fs::read_to_string("tests/resources/listxml_0244.xml").unwrap();

    let doc = read_mame_xml(&contents).unwrap();

    let roms = parse(doc, rom_categories).unwrap().categorize_roms().unwrap();

    let (working, not_working): (Roms, Roms) = roms
        .into_iter()
        .partition(|(_, rom)| matches!(rom.category, Working));

    assert_eq!(working.len(), 4);
    assert_eq!(not_working.len(), 7);

    // working roms

    let rom_005 = RomData {
        status: Some(RomStatus { driver: Status::Imperfect, emulation: Status::Good }),
        is_bios: false,
        is_system: false,
        is_mechanical: false,
        features: vec! {Feature { typ: "sound".to_string(), status: FeatureStatus::Imperfect }},
        clone_of: None,
        rom_of: None,
        chd: Vec::new(),
        category: "Maze / Shooter Small".to_string(),
    };
    assert_eq!(working["005"].data, rom_005);
    assert!(matches!(working["005"].category, Working));

    let rom_elevatora = RomData {
        status: Some(RomStatus { driver: Status::Good, emulation: Status::Good }),
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
    assert!(matches!(working["elevatora"].category, Working ));

    let rom_robocop = RomData {
        status: Some(RomStatus { driver: Status::Imperfect, emulation: Status::Good }),
        is_bios: false,
        is_system: false,
        is_mechanical: false,
        features: vec! {Feature { typ: "sound".to_string(), status: FeatureStatus::Imperfect }},
        clone_of: None,
        rom_of: None,
        chd: Vec::new(),
        category: "Platform / Shooter Scrolling".to_string(),
    };
    assert_eq!(working["robocop"].data, rom_robocop);
    assert!(matches!(working["robocop"].category, Working ));

    let rom_area51 = RomData {
        status: Some(RomStatus { driver: Status::Good, emulation: Status::Good }),
        is_bios: false,
        is_system: false,
        is_mechanical: false,
        features: Vec::new(),
        clone_of: None,
        rom_of: None,
        chd: vec! {Chd { name: "area51".to_string(), status: NoStatus }},
        category: "".to_string(),
    };
    assert_eq!(working["area51"].data, rom_area51);
    assert!(matches!(working["area51"].category, Working ));

    // not working roms

    let rom_100lions = RomData {
        status: Some(RomStatus { driver: Status::Preliminary, emulation: Status::Preliminary }),
        is_bios: false,
        is_system: false,
        is_mechanical: false,
        features: vec! {Feature { typ: "sound".to_string(), status: FeatureStatus::Unemulated }},
        clone_of: None,
        rom_of: Some("aristmk6".to_string()),
        chd: Vec::new(),
        category: "Slot Machine / Video Slot".to_string(),
    };
    assert_eq!(not_working["100lions"].data, rom_100lions);
    assert!(matches!(not_working["100lions"].category, NotWorking ));

    let rom_aristmk6 = RomData {
        status: Some(RomStatus { driver: Status::Preliminary, emulation: Status::Preliminary }),
        is_bios: true,
        is_system: true,
        is_mechanical: false,
        features: vec! {Feature { typ: "sound".to_string(), status: FeatureStatus::Unemulated }},
        clone_of: None,
        rom_of: None,
        chd: Vec::new(),
        category: "System / BIOS".to_string(),
    };
    assert_eq!(not_working["aristmk6"].data, rom_aristmk6);
    assert!(matches!(not_working["aristmk6"].category, Bios ));

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
    assert!(matches!(not_working["a24play"].category, System ));

    let rom_3dobios = RomData {
        status: Some(RomStatus { driver: Status::Preliminary, emulation: Status::Preliminary }),
        is_bios: true,
        is_system: true,
        is_mechanical: false,
        features: vec! {Feature { typ: "sound".to_string(), status: FeatureStatus::Unemulated }},
        clone_of: None,
        rom_of: None,
        chd: Vec::new(),
        category: "System / BIOS".to_string(),
    };
    assert_eq!(not_working["3dobios"].data, rom_3dobios);
    assert!(matches!(not_working["3dobios"].category, Bios ));

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
    assert!(matches!(not_working["sv801"].category, System ));

    let rom_99bottles = RomData {
        status: Some(RomStatus { driver: Status::Preliminary, emulation: Status::Preliminary }),
        is_bios: false,
        is_system: false,
        is_mechanical: false,
        features: vec! {Feature { typ: "sound".to_string(), status: FeatureStatus::Unemulated }},
        clone_of: Some("gammagic".to_string()),
        rom_of: Some("gammagic".to_string()),
        chd: vec! {Chd { name: "99bottles".to_string(), status: BadDump }},
        category: "MultiGame / Compilation".to_string(),
    };
    assert_eq!(not_working["99bottles"].data, rom_99bottles);
    assert!(matches!(not_working["99bottles"].category, NotWorking ));

    let rom_as_acp = RomData {
        status: Some(RomStatus { driver: Status::Preliminary, emulation: Status::Preliminary }),
        is_bios: false,
        is_system: false,
        is_mechanical: true,
        features: vec! {Feature { typ: "sound".to_string(), status: FeatureStatus::Unemulated }},
        clone_of: None,
        rom_of: None,
        chd: Vec::new(),
        category: "Slot Machine / Reels".to_string(),
    };
    assert_eq!(not_working["as_acp"].data, rom_as_acp);
    assert!(matches!(not_working["as_acp"].category, NotWorking ));
}
